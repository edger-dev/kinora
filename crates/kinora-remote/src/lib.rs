//! kinora-remote — mirror a kinora store's write-once files to an object
//! store (S3-compatible: Cloudflare R2, AWS S3, MinIO, or `object_store`'s
//! in-memory and local backends).
//!
//! Only the write-once trees sync: content blobs (`store/<ab>/<hash>.<ext>`)
//! and staged events (`staged/<ab>/<event-hash>.jsonl`). Every such file is
//! named by its hash, so `push` puts what the mirror lacks (put-if-absent,
//! never overwriting) and `pull` fetches what the store lacks; there is
//! nothing to merge.
//!
//! implements: b4e072636ef26d99d9b0f32227da47d9c11e93d8b458e4631af57c4c31a9ba57@b4e072636ef26d99d9b0f32227da47d9c11e93d8b458e4631af57c4c31a9ba57
//! task: c650f51df78e4018da6bb0ec59a22aa08d52708167f3490e3194f9cd67d2383c@4657166511ff29b05c2582477054dc890fe6dd33e815fcd38f0203d2a4d035c4

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use futures::TryStreamExt;
use kinora::hash::Hash;
use kinora::paths::find_blob_path;
use object_store::aws::{AmazonS3Builder, AmazonS3ConfigKey};
use object_store::local::LocalFileSystem;
use object_store::memory::InMemory;
use object_store::path::Path as Key;
use object_store::{ObjectStore, ObjectStoreExt, PutMode, PutOptions, PutPayload};

/// The write-once trees: the only parts of a store that sync.
const TREES: [&str; 2] = ["store", "staged"];

/// Why a sync failed.
#[derive(Debug, thiserror::Error)]
pub enum RemoteError {
    #[error("local store i/o: {0}")]
    Io(#[from] std::io::Error),
    #[error("object store: {0}")]
    Store(#[from] object_store::Error),
    /// The mirror URL cannot be used: why.
    #[error("mirror url: {0}")]
    Url(String),
}

/// What one push or pull did.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SyncReport {
    /// Files copied across.
    pub copied: usize,
    /// Files the other side already had.
    pub present: usize,
    /// Mirrored files refused because their bytes do not match their name.
    pub rejected: Vec<String>,
}

/// A store's mirror: an object store and a prefix inside it.
pub struct Mirror {
    store: Arc<dyn ObjectStore>,
    prefix: Key,
}

impl Mirror {
    pub fn new(store: Arc<dyn ObjectStore>, prefix: &str) -> Self {
        Self {
            store,
            prefix: Key::from(prefix),
        }
    }

    /// Put every write-once file of the store at `kinora_root` that the
    /// mirror lacks. A put is a conditional create: a file the mirror already
    /// holds is never overwritten, even when another machine put it a moment
    /// ago.
    pub async fn push(&self, kinora_root: &Path) -> Result<SyncReport, RemoteError> {
        let remote = self.remote_files().await?;
        let mut report = SyncReport::default();
        for rel in local_files(kinora_root)? {
            if remote.contains(&rel) {
                report.present += 1;
                continue;
            }
            let bytes = std::fs::read(kinora_root.join(&rel))?;
            let opts = PutOptions {
                mode: PutMode::Create,
                ..Default::default()
            };
            match self
                .store
                .put_opts(&self.key(&rel), PutPayload::from(bytes), opts)
                .await
            {
                Ok(_) => report.copied += 1,
                Err(object_store::Error::AlreadyExists { .. }) => report.present += 1,
                Err(e) => return Err(e.into()),
            }
        }
        Ok(report)
    }

    /// Fetch every mirrored write-once file the store at `kinora_root` lacks.
    /// A blob whose bytes do not hash to its name is refused, never written.
    pub async fn pull(&self, kinora_root: &Path) -> Result<SyncReport, RemoteError> {
        let mut report = SyncReport::default();
        let mut remote: Vec<String> = self.remote_files().await?.into_iter().collect();
        remote.sort();
        for rel in remote {
            if has_locally(kinora_root, &rel) {
                report.present += 1;
                continue;
            }
            let bytes = self.store.get(&self.key(&rel)).await?.bytes().await?;
            if rel.starts_with("store/") && !blob_matches(&rel, &bytes) {
                report.rejected.push(rel);
                continue;
            }
            write_atomically(&kinora_root.join(&rel), &bytes)?;
            report.copied += 1;
        }
        Ok(report)
    }

    /// A mirror named by URL, configured from the environment (`AWS_*` for
    /// `s3://`).
    pub fn from_url(url: &str) -> Result<Self, RemoteError> {
        Self::build(
            url,
            AmazonS3Builder::from_env(),
            Vec::<(String, String)>::new(),
        )
    }

    /// A mirror named by URL, with explicit options (for `s3://`, the
    /// `object_store` S3 config keys such as `aws_access_key_id`).
    pub fn from_url_with<K, V>(
        url: &str,
        options: impl IntoIterator<Item = (K, V)>,
    ) -> Result<Self, RemoteError>
    where
        K: AsRef<str>,
        V: Into<String>,
    {
        Self::build(url, AmazonS3Builder::new(), options)
    }

    fn build<K, V>(
        url: &str,
        s3: AmazonS3Builder,
        options: impl IntoIterator<Item = (K, V)>,
    ) -> Result<Self, RemoteError>
    where
        K: AsRef<str>,
        V: Into<String>,
    {
        let parsed = url::Url::parse(url).map_err(|e| RemoteError::Url(format!("{url}: {e}")))?;
        let path = parsed.path().trim_matches('/').to_owned();
        match parsed.scheme() {
            "s3" => {
                let bucket = parsed
                    .host_str()
                    .filter(|b| !b.is_empty())
                    .ok_or_else(|| RemoteError::Url(format!("{url}: no bucket")))?;
                let mut s3 = s3.with_bucket_name(bucket);
                for (key, value) in options {
                    let key: AmazonS3ConfigKey = key
                        .as_ref()
                        .parse()
                        .map_err(|e| RemoteError::Url(format!("option {}: {e}", key.as_ref())))?;
                    s3 = s3.with_config(key, value);
                }
                Ok(Self::new(Arc::new(s3.build()?), &path))
            }
            "file" => {
                let dir = PathBuf::from(format!("/{path}"));
                std::fs::create_dir_all(&dir)?;
                Ok(Self::new(
                    Arc::new(LocalFileSystem::new_with_prefix(&dir)?),
                    "",
                ))
            }
            "memory" => Ok(Self::new(Arc::new(InMemory::new()), &path)),
            other => Err(RemoteError::Url(format!(
                "{url}: scheme {other} is not one of s3, file, memory"
            ))),
        }
    }

    /// The prefix inside the object store the mirror lives under.
    pub fn prefix(&self) -> &str {
        self.prefix.as_ref()
    }

    fn key(&self, rel: &str) -> Key {
        if self.prefix.as_ref().is_empty() {
            Key::from(rel)
        } else {
            Key::from(format!("{}/{rel}", self.prefix))
        }
    }

    /// The mirror's write-once files, as paths relative to a store root.
    async fn remote_files(&self) -> Result<HashSet<String>, RemoteError> {
        let prefix = (!self.prefix.as_ref().is_empty()).then_some(&self.prefix);
        let listed: Vec<object_store::ObjectMeta> = self.store.list(prefix).try_collect().await?;
        Ok(listed
            .into_iter()
            .filter_map(|meta| {
                let parts: Vec<String> = match prefix {
                    Some(p) => meta
                        .location
                        .prefix_match(p)?
                        .map(|x| x.as_ref().to_owned())
                        .collect(),
                    None => meta
                        .location
                        .parts()
                        .map(|x| x.as_ref().to_owned())
                        .collect(),
                };
                let rel = parts.join("/");
                is_write_once(&rel).then_some(rel)
            })
            .collect())
    }
}

/// The mirror for sync callers.
pub mod blocking {
    use std::path::Path;

    use crate::{Mirror, RemoteError, SyncReport};

    /// [`Mirror::push`], blocking.
    pub fn push(mirror: &Mirror, kinora_root: &Path) -> Result<SyncReport, RemoteError> {
        run(mirror.push(kinora_root))
    }

    /// [`Mirror::pull`], blocking.
    pub fn pull(mirror: &Mirror, kinora_root: &Path) -> Result<SyncReport, RemoteError> {
        run(mirror.pull(kinora_root))
    }

    /// Drive a future to completion on a runtime of its own, on a thread of
    /// its own: safe from plain code and from inside another runtime alike.
    fn run<F>(future: F) -> Result<SyncReport, RemoteError>
    where
        F: std::future::Future<Output = Result<SyncReport, RemoteError>> + Send,
    {
        std::thread::scope(|scope| {
            scope
                .spawn(|| {
                    tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()?
                        .block_on(future)
                })
                .join()
                .map_err(|_| RemoteError::Io(std::io::Error::other("the sync thread panicked")))?
        })
    }
}

/// `tree/<ab>/<name>` under a write-once tree, not a temporary file.
fn is_write_once(rel: &str) -> bool {
    let parts: Vec<&str> = rel.split('/').collect();
    matches!(parts.as_slice(), [tree, shard, name]
        if TREES.contains(tree)
            && shard.len() == 2
            && !name.starts_with('.')
            && !name.contains('\\'))
}

/// The store's write-once files, as relative paths with `/` separators.
fn local_files(kinora_root: &Path) -> std::io::Result<Vec<String>> {
    let mut out = Vec::new();
    for tree in TREES {
        let base = kinora_root.join(tree);
        let Ok(shards) = std::fs::read_dir(&base) else {
            continue;
        };
        for shard in shards {
            let shard = shard?;
            if !shard.file_type()?.is_dir() {
                continue;
            }
            for file in std::fs::read_dir(shard.path())? {
                let file = file?;
                if !file.file_type()?.is_file() {
                    continue;
                }
                let rel = format!(
                    "{tree}/{}/{}",
                    shard.file_name().to_string_lossy(),
                    file.file_name().to_string_lossy()
                );
                if is_write_once(&rel) {
                    out.push(rel);
                }
            }
        }
    }
    out.sort();
    Ok(out)
}

/// Whether the store already has this file: a staged event by its path, a
/// blob by its hash under any extension.
fn has_locally(kinora_root: &Path, rel: &str) -> bool {
    if rel.starts_with("store/")
        && let Some(hash) = stem_hash(rel)
    {
        return find_blob_path(kinora_root, &hash).is_some();
    }
    kinora_root.join(rel).exists()
}

fn stem_hash(rel: &str) -> Option<Hash> {
    let name = rel.rsplit('/').next()?;
    let stem = name.split('.').next()?;
    stem.parse().ok()
}

fn blob_matches(rel: &str, bytes: &[u8]) -> bool {
    stem_hash(rel).is_some_and(|h| Hash::of_content(bytes) == h)
}

/// Write then rename, under a dot-name the sync itself skips, so a crashed
/// pull never leaves a half file that looks real.
fn write_atomically(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let dir = path
        .parent()
        .ok_or_else(|| std::io::Error::other(format!("no parent for {}", path.display())))?;
    std::fs::create_dir_all(dir)?;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let tmp: PathBuf = dir.join(format!(".pull-{name}"));
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(tmp, path)
}
