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
//! implements: b4e072636ef26d99d9b0f32227da47d9c11e93d8b458e4631af57c4c31a9ba57@e7980f316714ef8cacb7d4f87058a644fde9a63d22ac4ca01bc828a99029e6b1
//! task: c650f51df78e4018da6bb0ec59a22aa08d52708167f3490e3194f9cd67d2383c@4657166511ff29b05c2582477054dc890fe6dd33e815fcd38f0203d2a4d035c4

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use futures::TryStreamExt;
use kinora::hash::Hash;
use kinora::paths::find_blob_path;
use object_store::aws::{AmazonS3Builder, AmazonS3ConfigKey};
use object_store::local::LocalFileSystem;
use object_store::memory::InMemory;
use object_store::path::Path as Key;
use object_store::{ClientOptions, RetryConfig};
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
    ///
    /// What this machine already sent (or fetched) through this mirror is
    /// remembered in `.kinora/remote/`, so a push with nothing new costs no
    /// request at all. Blobs go before the events that cite them.
    pub async fn push(&self, kinora_root: &Path) -> Result<SyncReport, RemoteError> {
        self.push_files(kinora_root, true).await
    }

    /// Put every write-once file the mirror lacks, ignoring what this
    /// machine remembers sending: for a mirror that lost files.
    pub async fn push_all(&self, kinora_root: &Path) -> Result<SyncReport, RemoteError> {
        self.push_files(kinora_root, false).await
    }

    async fn push_files(
        &self,
        kinora_root: &Path,
        remembered: bool,
    ) -> Result<SyncReport, RemoteError> {
        let mut synced = self.synced(kinora_root)?;
        let mut report = SyncReport::default();
        for rel in local_files(kinora_root)? {
            if remembered && synced.contains(&rel) {
                report.present += 1;
                continue;
            }
            let bytes = match std::fs::read(kinora_root.join(&rel)) {
                Ok(bytes) => bytes,
                // Renamed or removed under us: nothing to send.
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => return Err(e.into()),
            };
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
            synced.record(&rel)?;
        }
        Ok(report)
    }

    /// Fetch every mirrored write-once file the store at `kinora_root` lacks.
    /// A blob whose bytes do not hash to its name is refused, never written.
    pub async fn pull(&self, kinora_root: &Path) -> Result<SyncReport, RemoteError> {
        let mut synced = self.synced(kinora_root)?;
        let mut report = SyncReport::default();
        let mut remote: Vec<String> = self.remote_files().await?.into_iter().collect();
        remote.sort_by(|a, b| sync_order(a).cmp(&sync_order(b)));
        for rel in remote {
            if has_locally(kinora_root, &rel) {
                report.present += 1;
                synced.record(&rel)?;
                continue;
            }
            let bytes = self.store.get(&self.key(&rel)).await?.bytes().await?;
            if !content_matches(&rel, &bytes) {
                report.rejected.push(rel);
                continue;
            }
            write_atomically(&kinora_root.join(&rel), &bytes)?;
            synced.record(&rel)?;
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
                // Short budgets: a machine that cannot reach the mirror falls
                // back to its local store, and must find that out in seconds,
                // not after minutes of retries.
                let mut s3 = s3
                    .with_bucket_name(bucket)
                    .with_client_options(
                        ClientOptions::new()
                            .with_connect_timeout(Duration::from_secs(3))
                            .with_timeout(Duration::from_secs(8)),
                    )
                    .with_retry(RetryConfig {
                        max_retries: 1,
                        retry_timeout: Duration::from_secs(8),
                        ..RetryConfig::default()
                    });
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

    /// What this machine has sent or fetched through this mirror.
    fn synced(&self, kinora_root: &Path) -> Result<Synced, RemoteError> {
        let name = Hash::of_content(format!("{}\n{}", self.store, self.prefix).as_bytes());
        Synced::open(
            &kinora_root
                .join("remote")
                .join(format!("{}.synced", &name.as_hex()[..16])),
        )
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

/// A file kinora itself names: `store/<ab>/<hash>[.<ext>]` or
/// `staged/<ab>/<hash>.jsonl`, the shard being the hash's first two digits.
/// Temporary files (`.tmp-*`, `*.jsonl.tmp`, `.pull-*`) never match.
fn is_write_once(rel: &str) -> bool {
    let parts: Vec<&str> = rel.split('/').collect();
    let [tree, shard, name] = parts.as_slice() else {
        return false;
    };
    let (stem, ext) = match name.split_once('.') {
        Some((stem, ext)) => (stem, Some(ext)),
        None => (*name, None),
    };
    let hex = stem.len() == 64
        && stem
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase());
    let ext_ok = match (*tree, ext) {
        ("store", None) => true,
        ("store", Some(e)) => !e.is_empty() && e.bytes().all(|b| b.is_ascii_alphanumeric()),
        ("staged", Some("jsonl")) => true,
        _ => false,
    };
    hex && ext_ok && stem.get(..2) == Some(*shard)
}

/// Blobs before the events that cite them, on push and on pull: an
/// interrupted sync leaves blobs without events, never events without blobs.
fn sync_order(rel: &str) -> (u8, &str) {
    (u8::from(rel.starts_with("staged/")), rel)
}

/// The store's write-once files, as relative paths with `/` separators.
fn local_files(kinora_root: &Path) -> std::io::Result<Vec<String>> {
    let mut out = Vec::new();
    for tree in TREES {
        let base = kinora_root.join(tree);
        let shards = match std::fs::read_dir(&base) {
            Ok(shards) => shards,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e),
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
                // A name that is not UTF-8 is not one kinora wrote.
                let (Some(shard_name), Some(file_name)) = (
                    shard.file_name().to_str().map(str::to_owned),
                    file.file_name().to_str().map(str::to_owned),
                ) else {
                    continue;
                };
                let rel = format!("{tree}/{shard_name}/{file_name}");
                if is_write_once(&rel) {
                    out.push(rel);
                }
            }
        }
    }
    out.sort_by(|a, b| sync_order(a).cmp(&sync_order(b)));
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

/// Whether a mirrored file is what its name says: a blob's bytes hash to it;
/// an event is one canonical event line (plus its newline) that hashes to it.
fn content_matches(rel: &str, bytes: &[u8]) -> bool {
    let Some(name) = stem_hash(rel) else {
        return false;
    };
    if rel.starts_with("store/") {
        return Hash::of_content(bytes) == name;
    }
    let Some(line) = bytes.strip_suffix(b"\n") else {
        return false;
    };
    if line.contains(&b'\n') || Hash::of_content(line) != name {
        return false;
    }
    std::str::from_utf8(line)
        .ok()
        .and_then(|l| kinora::event::Event::from_json_line(l).ok())
        .and_then(|e| e.event_hash().ok())
        .is_some_and(|h| h == name)
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
    // A name of its own per pull, so two pulls into one store cannot write
    // the same temporary.
    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let tmp: PathBuf = dir.join(format!(".pull-{}-{n}-{name}", std::process::id()));
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(tmp, path)
}

/// The files a store has sent or fetched through one mirror: one path per
/// line, appended as each lands. A torn last line only costs a resend.
struct Synced {
    path: PathBuf,
    seen: HashSet<String>,
}

impl Synced {
    fn open(path: &Path) -> Result<Self, RemoteError> {
        let seen = match std::fs::read_to_string(path) {
            Ok(text) => text.lines().map(str::to_owned).collect(),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => HashSet::new(),
            Err(e) => return Err(e.into()),
        };
        Ok(Self {
            path: path.to_owned(),
            seen,
        })
    }

    fn contains(&self, rel: &str) -> bool {
        self.seen.contains(rel)
    }

    fn record(&mut self, rel: &str) -> Result<(), RemoteError> {
        use std::io::Write;
        if !self.seen.insert(rel.to_owned()) {
            return Ok(());
        }
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        f.write_all(format!("{rel}\n").as_bytes())?;
        Ok(())
    }
}
