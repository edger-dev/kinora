//! Push and pull through a mirror: put-if-absent, hash-checked, write-once
//! trees only — two stores meeting through one mirror end with the union.
//!
//! implements: b4e072636ef26d99d9b0f32227da47d9c11e93d8b458e4631af57c4c31a9ba57@e7980f316714ef8cacb7d4f87058a644fde9a63d22ac4ca01bc828a99029e6b1

#![allow(clippy::unwrap_used, clippy::expect_used, reason = "tests")]

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use futures::TryStreamExt;
use kinora::kino::{StoreKinoParams, store_kino};
use kinora::paths::kinora_root;
use kinora::resolve::Resolver;
use kinora_remote::Mirror;
use object_store::memory::InMemory;
use object_store::path::Path as Key;
use object_store::{ObjectStore, ObjectStoreExt, PutPayload};

fn scratch(name: &str) -> PathBuf {
    static N: AtomicU32 = AtomicU32::new(0);
    let dir = std::env::temp_dir().join(format!(
        "kinora-remote-{name}-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A fresh store with one kino per body; its `.kinora` root.
fn store_with(name: &str, bodies: &[&str]) -> PathBuf {
    let repo = scratch(name);
    kinora::init::init(&repo, "https://example.org/r").unwrap();
    let root = kinora_root(&repo);
    for body in bodies {
        store_kino(
            &root,
            StoreKinoParams {
                kind: "markdown".into(),
                content: body.as_bytes().to_vec(),
                author: "test".into(),
                provenance: "test".into(),
                ts: "2026-10-21T00:00:00Z".into(),
                metadata: Default::default(),
                id: None,
                parents: Vec::new(),
            },
        )
        .unwrap();
    }
    root
}

/// Relative paths of every file under `root`'s write-once trees.
fn write_once_files(root: &Path) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for tree in ["store", "staged"] {
        let base = root.join(tree);
        let mut stack = vec![base.clone()];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for e in entries {
                let p = e.unwrap().path();
                if p.is_dir() {
                    stack.push(p);
                } else {
                    let rel = p.strip_prefix(root).unwrap().to_string_lossy().into_owned();
                    out.insert(rel);
                }
            }
        }
    }
    out
}

async fn keys(store: &dyn ObjectStore, prefix: &str) -> BTreeSet<String> {
    let prefix = Key::from(prefix);
    store
        .list(Some(&prefix))
        .map_ok(|m| m.location.to_string())
        .try_collect()
        .await
        .unwrap()
}

#[tokio::test]
async fn push_sends_the_write_once_files_once_and_nothing_else() {
    let root = store_with("push", &["# one\n", "# two\n"]);
    std::fs::write(root.join("store").join(".tmp-orphan.md"), "half written").unwrap();
    let remote = Arc::new(InMemory::new());
    let mirror = Mirror::new(remote.clone(), "ctx");

    let first = mirror.push(&root).await.unwrap();
    let local = write_once_files(&root);
    let sent: BTreeSet<String> = keys(remote.as_ref(), "ctx")
        .await
        .into_iter()
        .map(|k| k.strip_prefix("ctx/").unwrap().to_owned())
        .collect();
    let expected: BTreeSet<String> = local
        .into_iter()
        .filter(|p| !p.rsplit('/').next().unwrap().starts_with('.'))
        .collect();
    assert_eq!(
        sent, expected,
        "store/ and staged/, no tmp, no config or roots"
    );
    assert_eq!(first.copied, expected.len());

    let second = mirror.push(&root).await.unwrap();
    assert_eq!(second.copied, 0);
    assert_eq!(second.present, expected.len());
}

#[tokio::test]
async fn two_stores_meeting_through_one_mirror_end_with_the_union() {
    let a = store_with("a", &["# from a\n", "# shared\n"]);
    let b = store_with("b", &["# from b\n", "# shared\n"]);
    let remote = Arc::new(InMemory::new());
    let mirror = Mirror::new(remote, "ctx");

    mirror.push(&a).await.unwrap();
    mirror.push(&b).await.unwrap();
    mirror.pull(&a).await.unwrap();
    let pulled = mirror.pull(&b).await.unwrap();
    assert!(pulled.copied > 0);

    assert_eq!(write_once_files(&a), write_once_files(&b));
    let ids = |root: &Path| -> BTreeSet<String> {
        Resolver::load(root)
            .unwrap()
            .identities()
            .keys()
            .cloned()
            .collect()
    };
    assert_eq!(ids(&a).len(), 3, "from a, from b, shared");
    assert_eq!(ids(&a), ids(&b));
    assert_eq!(mirror.pull(&a).await.unwrap().copied, 0, "nothing left");
}

#[tokio::test]
async fn push_never_overwrites_what_the_mirror_holds() {
    let root = store_with("no-clobber", &["# mine\n"]);
    let remote = Arc::new(InMemory::new());
    let rel = write_once_files(&root)
        .into_iter()
        .find(|p| p.starts_with("store/"))
        .unwrap();
    let key = Key::from(format!("ctx/{rel}"));
    remote
        .put(&key, PutPayload::from_static(b"already there"))
        .await
        .unwrap();

    Mirror::new(remote.clone(), "ctx")
        .push(&root)
        .await
        .unwrap();
    let held = remote.get(&key).await.unwrap().bytes().await.unwrap();
    assert_eq!(&held[..], b"already there");
}

#[tokio::test]
async fn pull_refuses_a_blob_whose_bytes_do_not_match_its_name() {
    let source = store_with("tamper-src", &["# honest\n"]);
    let remote = Arc::new(InMemory::new());
    let mirror = Mirror::new(remote.clone(), "ctx");
    mirror.push(&source).await.unwrap();
    let rel = write_once_files(&source)
        .into_iter()
        .find(|p| p.starts_with("store/"))
        .unwrap();
    remote
        .put(
            &Key::from(format!("ctx/{rel}")),
            PutPayload::from_static(b"forged"),
        )
        .await
        .unwrap();

    let target = store_with("tamper-dst", &[]);
    let report = mirror.pull(&target).await.unwrap();
    assert_eq!(report.rejected, std::slice::from_ref(&rel));
    assert!(!target.join(&rel).exists(), "a forged blob never lands");
}

#[tokio::test]
async fn pull_takes_only_the_write_once_trees() {
    let remote = Arc::new(InMemory::new());
    for key in ["ctx/roots/specs", "ctx/config.styx", "ctx/elsewhere/x"] {
        remote
            .put(
                &Key::from(key),
                PutPayload::from_static(b"not mine to take"),
            )
            .await
            .unwrap();
    }
    let target = store_with("only-trees", &[]);
    let report = Mirror::new(remote, "ctx").pull(&target).await.unwrap();
    assert_eq!(report.copied, 0);
    assert!(!target.join("roots/specs").exists());
    assert_ne!(
        std::fs::read_to_string(target.join("config.styx")).unwrap(),
        "not mine to take"
    );
}

#[tokio::test]
async fn a_mirror_on_the_local_filesystem_round_trips() {
    let source = store_with("fs-src", &["# on disk\n"]);
    let bucket = scratch("fs-bucket");
    let remote = Arc::new(object_store::local::LocalFileSystem::new_with_prefix(&bucket).unwrap());
    let mirror = Mirror::new(remote, "ctx");
    mirror.push(&source).await.unwrap();
    let target = store_with("fs-dst", &[]);
    mirror.pull(&target).await.unwrap();
    assert_eq!(write_once_files(&source), write_once_files(&target));
}

/// A staged event must be one canonical event line, named by its hash: a
/// garbage file or a forged event never lands, and the store still loads.
#[tokio::test]
async fn pull_refuses_a_staged_event_that_is_not_what_its_name_says() {
    let source = store_with("ev-src", &["# real\n"]);
    let remote = Arc::new(InMemory::new());
    let mirror = Mirror::new(remote.clone(), "ctx");
    mirror.push(&source).await.unwrap();

    // A real event's line, filed under another event's name.
    let events: Vec<String> = write_once_files(&source)
        .into_iter()
        .filter(|p| p.starts_with("staged/"))
        .collect();
    let real = std::fs::read(source.join(&events[0])).unwrap();
    let forged_name = format!("staged/ab/{}.jsonl", "ab".repeat(32));
    remote
        .put(
            &Key::from(format!("ctx/{forged_name}")),
            PutPayload::from(real),
        )
        .await
        .unwrap();
    let garbage_name = format!("staged/cd/{}.jsonl", "cd".repeat(32));
    remote
        .put(
            &Key::from(format!("ctx/{garbage_name}")),
            PutPayload::from_static(b"garbage\n"),
        )
        .await
        .unwrap();

    let target = store_with("ev-dst", &[]);
    let report = mirror.pull(&target).await.unwrap();
    let mut rejected = report.rejected.clone();
    rejected.sort();
    assert_eq!(rejected, [forged_name.clone(), garbage_name.clone()]);
    assert!(!target.join(&forged_name).exists());
    assert!(!target.join(&garbage_name).exists());
    assert_eq!(Resolver::load(&target).unwrap().identities().len(), 1);
}

/// kinora's own temporary files — `.tmp-*` blobs, `*.jsonl.tmp` events —
/// never leave the machine.
#[tokio::test]
async fn push_skips_kinora_temporary_files() {
    let root = store_with("tmp", &["# one\n"]);
    let shard = std::fs::read_dir(root.join("staged"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    std::fs::write(shard.join(format!("{}.jsonl.tmp", "ef".repeat(32))), "half").unwrap();
    let remote = Arc::new(InMemory::new());
    Mirror::new(remote.clone(), "ctx")
        .push(&root)
        .await
        .unwrap();
    let sent = keys(remote.as_ref(), "ctx").await;
    assert!(sent.iter().all(|k| !k.ends_with(".tmp")), "{sent:?}");
}

/// A blob in a shard folder that is not its hash's is not a blob kinora can
/// see: it is ignored, not fetched again on every pull.
#[tokio::test]
async fn pull_ignores_a_blob_in_the_wrong_shard() {
    let remote = Arc::new(InMemory::new());
    let bytes = b"# misplaced\n";
    let hash = kinora::hash::Hash::of_content(bytes);
    remote
        .put(
            &Key::from(format!("ctx/store/zz/{}.md", hash.as_hex())),
            PutPayload::from_static(bytes),
        )
        .await
        .unwrap();
    let target = store_with("shard", &[]);
    let mirror = Mirror::new(remote, "ctx");
    assert_eq!(mirror.pull(&target).await.unwrap().copied, 0);
    assert_eq!(mirror.pull(&target).await.unwrap().copied, 0);
}

/// Push sends only what this machine has not sent before: a second push
/// with nothing new does not even list the mirror.
#[tokio::test]
async fn a_push_with_nothing_new_lists_nothing() {
    let root = store_with("marker", &["# one\n"]);
    let counting = Arc::new(CountingLists::default());
    let mirror = Mirror::new(counting.clone(), "ctx");
    mirror.push(&root).await.unwrap();
    let before = counting.lists.load(Ordering::Relaxed);
    let again = mirror.push(&root).await.unwrap();
    assert_eq!(again.copied, 0);
    assert_eq!(counting.lists.load(Ordering::Relaxed), before, "no LIST");
}

/// An in-memory store that counts LIST calls.
#[derive(Debug, Default)]
struct CountingLists {
    inner: InMemory,
    lists: std::sync::atomic::AtomicUsize,
}

impl std::fmt::Display for CountingLists {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "CountingLists")
    }
}

#[async_trait::async_trait]
impl ObjectStore for CountingLists {
    async fn put_opts(
        &self,
        location: &Key,
        payload: PutPayload,
        opts: object_store::PutOptions,
    ) -> object_store::Result<object_store::PutResult> {
        self.inner.put_opts(location, payload, opts).await
    }
    async fn put_multipart_opts(
        &self,
        location: &Key,
        opts: object_store::PutMultipartOptions,
    ) -> object_store::Result<Box<dyn object_store::MultipartUpload>> {
        self.inner.put_multipart_opts(location, opts).await
    }
    async fn get_opts(
        &self,
        location: &Key,
        options: object_store::GetOptions,
    ) -> object_store::Result<object_store::GetResult> {
        self.inner.get_opts(location, options).await
    }
    fn delete_stream(
        &self,
        locations: futures::stream::BoxStream<'static, object_store::Result<Key>>,
    ) -> futures::stream::BoxStream<'static, object_store::Result<Key>> {
        self.inner.delete_stream(locations)
    }
    fn list(
        &self,
        prefix: Option<&Key>,
    ) -> futures::stream::BoxStream<'static, object_store::Result<object_store::ObjectMeta>> {
        self.lists.fetch_add(1, Ordering::Relaxed);
        self.inner.list(prefix)
    }
    async fn list_with_delimiter(
        &self,
        prefix: Option<&Key>,
    ) -> object_store::Result<object_store::ListResult> {
        self.lists.fetch_add(1, Ordering::Relaxed);
        self.inner.list_with_delimiter(prefix).await
    }
    async fn copy_opts(
        &self,
        from: &Key,
        to: &Key,
        options: object_store::CopyOptions,
    ) -> object_store::Result<()> {
        self.inner.copy_opts(from, to, options).await
    }
}

/// A mirror that lost files (a wiped bucket) is refilled by a full push,
/// which ignores what this machine remembers sending.
#[tokio::test]
async fn a_full_push_refills_a_mirror_that_lost_files() {
    let root = store_with("refill", &["# one\n"]);
    let remote = Arc::new(InMemory::new());
    let mirror = Mirror::new(remote.clone(), "ctx");
    mirror.push(&root).await.unwrap();
    let all = keys(remote.as_ref(), "ctx").await;
    for k in &all {
        remote.delete(&Key::from(k.as_str())).await.unwrap();
    }
    assert_eq!(mirror.push(&root).await.unwrap().copied, 0, "remembered");
    let refill = mirror.push_all(&root).await.unwrap();
    assert_eq!(refill.copied, all.len());
    assert_eq!(keys(remote.as_ref(), "ctx").await, all);
}
