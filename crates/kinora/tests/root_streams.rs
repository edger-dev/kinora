//! A root's versions as an append-only stream, end to end: commits append,
//! branches merged in git meet as two heads, readers take in both, the next
//! commit folds them, a genuine fork is refused, a legacy pointer migrates,
//! a clone carries every head.
//!
//! A git branch is modelled as a copy of `.kinora`, and a merge as the union
//! of the files under it — what git does with files that are only ever
//! added.
//!
//! implements: 02be1c37832e1c710030e7bc5a89441b55bdbf5a05ebe2c501f824708b7f3372@02be1c37832e1c710030e7bc5a89441b55bdbf5a05ebe2c501f824708b7f3372

#![allow(clippy::unwrap_used, clippy::expect_used, reason = "tests")]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use kinora::clone::{CloneParams, clone_repo};
use kinora::commit::{CommitParams, commit_all};
use kinora::init::init;
use kinora::kino::{StoreKinoParams, StoredKino, store_kino};
use kinora::paths::{kinora_root, root_pointer_path, roots_dir};
use kinora::resolve::Resolver;
use kinora::roots::root_heads;

fn repo() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    init(tmp.path(), "https://example.com/x.git").unwrap();
    let root = kinora_root(tmp.path());
    (tmp, root)
}

fn store(
    root: &Path,
    content: &str,
    name: &str,
    id: Option<&StoredKino>,
    parents: &[&StoredKino],
) -> StoredKino {
    store_kino(
        root,
        StoreKinoParams {
            kind: "markdown".into(),
            content: content.as_bytes().to_vec(),
            author: "t".into(),
            provenance: "test".into(),
            ts: "2026-10-24T00:00:00Z".into(),
            metadata: BTreeMap::from([("name".into(), name.into())]),
            id: id.map(|k| k.event.id.clone()),
            parents: parents.iter().map(|k| k.event.hash.clone()).collect(),
        },
    )
    .unwrap()
}

fn commit(root: &Path) -> Result<(), String> {
    commit_all(
        root,
        CommitParams {
            author: "t".into(),
            provenance: "test".into(),
            ts: "2026-10-24T00:00:01Z".into(),
        },
    )
    .map_err(|e| e.to_string())
    .and_then(|entries| {
        for (_, result) in entries {
            if let Err(err) = result {
                return Err(err.to_string());
            }
        }
        Ok(())
    })
}

fn resolves(root: &Path, name: &str) -> String {
    let r = Resolver::load(root).unwrap();
    String::from_utf8(r.resolve_by_name(name).unwrap().content).unwrap()
}

/// A branch: a copy of the whole `.kinora`.
fn branch(root: &Path, into: &Path) -> PathBuf {
    let dst = into.join(".kinora");
    copy_tree(root, &dst, true);
    dst
}

/// Copy every file under `from` into `to`; with `all` false, only files
/// `to` lacks (a merge of added files).
fn copy_tree(from: &Path, to: &Path, all: bool) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &target, all);
        } else if all || !target.exists() {
            fs::copy(entry.path(), &target).unwrap();
        }
    }
}

/// Merge `theirs` into `ours` as git would merge added files.
fn merge(theirs: &Path, ours: &Path) {
    for tree in ["store", "staged", "roots"] {
        if theirs.join(tree).is_dir() {
            copy_tree(&theirs.join(tree), &ours.join(tree), false);
        }
    }
}

#[test]
fn a_commit_appends_to_the_roots_stream() {
    let (_t, root) = repo();
    store(&root, "hello", "greeting", None, &[]);
    commit(&root).unwrap();
    let stream = roots_dir(&root).join("inbox");
    assert!(stream.is_dir(), "the inbox root is a stream");
    let entries: Vec<_> = fs::read_dir(&stream)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| !n.starts_with('.'))
        .collect();
    assert_eq!(entries.len(), 1, "{entries:?}");
    assert!(entries[0].ends_with(".styxl"));
    assert_eq!(root_heads(&root, "inbox").unwrap().len(), 1);
    assert_eq!(resolves(&root, "greeting"), "hello");

    // Nothing new: nothing appended.
    commit(&root).unwrap();
    let entries = fs::read_dir(&stream)
        .unwrap()
        .filter(|e| {
            !e.as_ref()
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with('.')
        })
        .count();
    assert_eq!(entries, 1);
}

/// Two branches commit; merged, the root has two heads, both read; the next
/// commit folds them into one version naming both.
#[test]
fn commits_on_two_branches_meet_and_fold() {
    let (t, a) = repo();
    store(&a, "base", "base", None, &[]);
    commit(&a).unwrap();
    let other = tempfile::tempdir().unwrap();
    let b = branch(&a, other.path());

    store(&a, "from a", "left", None, &[]);
    commit(&a).unwrap();
    store(&b, "from b", "right", None, &[]);
    commit(&b).unwrap();

    merge(&b, &a);
    assert_eq!(
        root_heads(&a, "inbox").unwrap().len(),
        2,
        "two heads after the merge"
    );
    assert_eq!(resolves(&a, "left"), "from a");
    assert_eq!(resolves(&a, "right"), "from b");
    assert_eq!(resolves(&a, "base"), "base");

    commit(&a).unwrap();
    let heads = root_heads(&a, "inbox").unwrap();
    assert_eq!(heads.len(), 1, "folded");
    let folded = fs::read_to_string(
        fs::read_dir(roots_dir(&a).join("inbox"))
            .unwrap()
            .map(|e| e.unwrap().path())
            .find(|p| p.to_string_lossy().contains(heads[0].as_hex()))
            .unwrap(),
    )
    .unwrap();
    let header = folded.lines().next().unwrap();
    let parents = header
        .split("parents (")
        .nth(1)
        .unwrap()
        .split(')')
        .next()
        .unwrap();
    assert_eq!(
        parents.split([' ', ',']).filter(|p| !p.is_empty()).count(),
        2,
        "{header}"
    );
    assert_eq!(resolves(&a, "left"), "from a");
    assert_eq!(resolves(&a, "right"), "from b");
    drop(t);
}

/// Both branches revise one kino: a genuine fork. The fold is refused,
/// naming the kino; merging the kino (a version naming both) settles it.
#[test]
fn a_genuine_fork_is_refused_until_merged() {
    let (_t, a) = repo();
    let doc = store(&a, "v1", "doc", None, &[]);
    commit(&a).unwrap();
    let other = tempfile::tempdir().unwrap();
    let b = branch(&a, other.path());

    let left = store(&a, "v2 left", "doc", Some(&doc), &[&doc]);
    commit(&a).unwrap();
    let right = store(&b, "v2 right", "doc", Some(&doc), &[&doc]);
    commit(&b).unwrap();
    merge(&b, &a);

    let err = commit(&a).unwrap_err();
    assert!(err.contains(&doc.event.id), "names the kino: {err}");

    store(&a, "v3 merged", "doc", Some(&doc), &[&left, &right]);
    commit(&a).unwrap();
    assert_eq!(root_heads(&a, "inbox").unwrap().len(), 1);
    assert_eq!(resolves(&a, "doc"), "v3 merged");
}

/// A ledger from before streams: its pointer file is read, and the next
/// commit moves it onto the stream.
#[test]
fn a_legacy_pointer_is_read_then_migrated() {
    let (_t, root) = repo();
    store(&root, "old", "old", None, &[]);
    commit(&root).unwrap();
    // Turn the stream back into a legacy pointer file.
    let head = root_heads(&root, "inbox").unwrap().remove(0);
    fs::remove_dir_all(roots_dir(&root).join("inbox")).unwrap();
    fs::write(root_pointer_path(&root, "inbox"), head.as_hex()).unwrap();
    assert_eq!(resolves(&root, "old"), "old");

    store(&root, "new", "new", None, &[]);
    commit(&root).unwrap();
    assert!(roots_dir(&root).join("inbox").is_dir());
    assert!(!root_pointer_path(&root, "inbox").is_file());
    assert_eq!(resolves(&root, "old"), "old");
    assert_eq!(resolves(&root, "new"), "new");
}

/// A clone carries every head of a forked root.
#[test]
fn a_clone_carries_every_head() {
    let (_t, a) = repo();
    store(&a, "base", "base", None, &[]);
    commit(&a).unwrap();
    let other = tempfile::tempdir().unwrap();
    let b = branch(&a, other.path());
    store(&a, "from a", "left", None, &[]);
    commit(&a).unwrap();
    store(&b, "from b", "right", None, &[]);
    commit(&b).unwrap();
    merge(&b, &a);

    let target = tempfile::tempdir().unwrap();
    let dst = target.path().join(".kinora");
    clone_repo(
        &a,
        &dst,
        CloneParams {
            author: "t".into(),
            provenance: "test".into(),
            ts: "2026-10-24T00:00:02Z".into(),
        },
    )
    .unwrap();
    assert_eq!(root_heads(&dst, "inbox").unwrap().len(), 2);
    assert_eq!(resolves(&dst, "left"), "from a");
    assert_eq!(resolves(&dst, "right"), "from b");
}
