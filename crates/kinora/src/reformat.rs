//! Reformat legacy `.styx`-wrapped kinograph and root blobs into the new
//! styxl one-entry-per-line form.
//!
//! Strategy:
//!
//! 1. **Regular kinograph kinos** (`kind: "kinograph"`) reachable from any
//!    root's current root-kinograph entries are reformatted as *staged
//!    new-version events*. The reformat does not update pointers directly
//!    — the user's next `kinora commit` promotes the new versions to
//!    heads.
//! 2. **Root kinographs** (`kind: "root"`) are produced by commit, not
//!    staged. For those, reformat stores the new blob + records a store
//!    event + updates the root pointer in one step — the same shape that
//!    commit itself uses.
//! 3. Non-styx kinds (markdown/text/binary/…) are opaque byte streams and
//!    left untouched.
//! 4. Idempotent: re-running reformat on an already-styxl repo stages no
//!    events and updates no pointers.

use std::fs;
use std::io;
use std::path::Path;

use crate::commit::CommitError;
use crate::config::ConfigError;
use crate::event::EventError;
use crate::hash::{Hash, HashParseError};
use crate::kino::StoreKinoError;
use crate::kinograph::KinographError;
use crate::ledger::LedgerError;
use crate::paths::{root_pointer_path, roots_dir};
use crate::root::RootError;
use crate::store::StoreError;

#[derive(Debug, thiserror::Error)]
pub enum ReformatError {
    #[error("reformat io error: {0}")]
    Io(#[from] io::Error),
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error(transparent)]
    Ledger(#[from] LedgerError),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    StoreKino(#[from] StoreKinoError),
    #[error(transparent)]
    Kinograph(#[from] KinographError),
    #[error(transparent)]
    Root(#[from] RootError),
    #[error(transparent)]
    Event(#[from] EventError),
    #[error(transparent)]
    Commit(#[from] CommitError),
    #[error("invalid hash `{value}`: {err}")]
    InvalidHash {
        value: String,
        #[source]
        err: HashParseError,
    },
    #[error("root pointer {name} references a version `{version}` with no matching store event")]
    PriorRootEventMissing { name: String, version: String },
    #[error("identity {id} has {} heads at reformat time: {}", .heads.len(), .heads.join(", "))]
    MultipleHeads { id: String, heads: Vec<String> },
}

#[derive(Debug, Clone)]
pub struct ReformatParams {
    pub author: String,
    pub provenance: String,
    pub ts: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReformattedKinograph {
    pub id: String,
    pub prior_version: String,
    pub new_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReformattedRoot {
    pub root_name: String,
    pub prior_version: String,
    pub new_version: String,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ReformatReport {
    pub reformatted_kinographs: Vec<ReformattedKinograph>,
    pub skipped_kinographs_already_formatted: usize,
    pub reformatted_roots: Vec<ReformattedRoot>,
    pub skipped_roots_already_formatted: usize,
}

/// Walk the repo's root pointers and reachable kinograph kinos, rewriting
/// any remaining legacy-styx content into styxl.
///
/// Stages version events for regular kinograph kinos and writes root
/// pointers + store events for root kinographs. Caller is expected to
/// run a subsequent `kinora commit` to surface the staged kinograph
/// versions as heads for render.
pub fn reformat_repo(
    _kinora_root: &Path,
    _params: ReformatParams,
) -> Result<ReformatReport, ReformatError> {
    // Stub — real body lands in the follow-up commit so the tests in this
    // commit fail at runtime (not compile time), validating the spec
    // before the implementation exists.
    Ok(ReformatReport::default())
}

/// Atomically write `.kinora/roots/<name>` with the given 64-hex hash.
/// Mirrors `commit::write_root_pointer` but is kept private to this
/// module so the reformat path can update pointers without taking a
/// `pub(crate)` dep on commit internals.
#[cfg_attr(not(test), allow(dead_code))]
fn write_root_pointer(kinora_root: &Path, root_name: &str, hash: &Hash) -> io::Result<()> {
    let dir = roots_dir(kinora_root);
    fs::create_dir_all(&dir)?;
    let path = root_pointer_path(kinora_root, root_name);
    let tmp = dir.join(format!(".{root_name}.tmp"));
    fs::write(&tmp, hash.as_hex())?;
    fs::rename(&tmp, &path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commit::{commit_all, commit_root, CommitParams};
    use crate::event::Event;
    use crate::init::init;
    use crate::kino::{store_kino, StoreKinoParams};
    use crate::kinograph::{is_styxl, Entry as KinographEntry, Kinograph};
    use crate::ledger::Ledger;
    use crate::paths::kinora_root;
    use crate::root::RootKinograph;
    use crate::store::ContentStore;
    use std::collections::BTreeMap;
    use std::path::PathBuf;
    use std::str::FromStr;
    use tempfile::TempDir;

    fn setup() -> (TempDir, PathBuf) {
        let tmp = TempDir::new().unwrap();
        init(tmp.path(), "https://example.com/x.git").unwrap();
        let root = kinora_root(tmp.path());
        (tmp, root)
    }

    fn reformat_params(ts: &str) -> ReformatParams {
        ReformatParams {
            author: "yj".into(),
            provenance: "reformat-test".into(),
            ts: ts.into(),
        }
    }

    fn commit_params(ts: &str) -> CommitParams {
        CommitParams {
            author: "yj".into(),
            provenance: "reformat-test".into(),
            ts: ts.into(),
        }
    }

    fn store_md(root: &Path, content: &[u8], name: &str, ts: &str) -> Event {
        let stored = store_kino(
            root,
            StoreKinoParams {
                kind: "markdown".into(),
                content: content.to_vec(),
                author: "yj".into(),
                provenance: "reformat-test".into(),
                ts: ts.into(),
                metadata: BTreeMap::from([("name".into(), name.into())]),
                id: None,
                parents: vec![],
            },
        )
        .unwrap();
        stored.event
    }

    /// Store a legacy-wrapped kinograph composing the given entry ids.
    fn store_legacy_kinograph(
        root: &Path,
        entry_ids: &[String],
        name: &str,
        ts: &str,
    ) -> Event {
        let entries: Vec<KinographEntry> = entry_ids
            .iter()
            .map(|id| KinographEntry::with_id(id.clone()))
            .collect();
        let k = Kinograph { entries };
        let content = k.to_styx().unwrap().into_bytes();
        assert!(
            !is_styxl(std::str::from_utf8(&content).unwrap()),
            "to_styx must emit legacy wrapped form for this test"
        );
        let stored = store_kino(
            root,
            StoreKinoParams {
                kind: "kinograph".into(),
                content,
                author: "yj".into(),
                provenance: "reformat-test".into(),
                ts: ts.into(),
                metadata: BTreeMap::from([("name".into(), name.into())]),
                id: None,
                parents: vec![],
            },
        )
        .unwrap();
        stored.event
    }

    /// Store a styxl-form kinograph composing the given entry ids.
    fn store_styxl_kinograph(
        root: &Path,
        entry_ids: &[String],
        name: &str,
        ts: &str,
    ) -> Event {
        let entries: Vec<KinographEntry> = entry_ids
            .iter()
            .map(|id| KinographEntry::with_id(id.clone()))
            .collect();
        let k = Kinograph { entries };
        let content = k.to_styxl().unwrap().into_bytes();
        assert!(
            is_styxl(std::str::from_utf8(&content).unwrap()),
            "to_styxl must emit styxl form for this test"
        );
        let stored = store_kino(
            root,
            StoreKinoParams {
                kind: "kinograph".into(),
                content,
                author: "yj".into(),
                provenance: "reformat-test".into(),
                ts: ts.into(),
                metadata: BTreeMap::from([("name".into(), name.into())]),
                id: None,
                parents: vec![],
            },
        )
        .unwrap();
        stored.event
    }

    /// Simulate a pre-migration root pointer: write a legacy-wrapped root
    /// blob directly via `store_kino(kind: "root")` and set the pointer to
    /// it. Returns the genesis root event.
    fn seed_legacy_root_pointer(
        root: &Path,
        root_name: &str,
        entries: Vec<crate::root::RootEntry>,
        ts: &str,
    ) -> Event {
        let rk = RootKinograph { entries };
        let content = rk.to_styx().unwrap().into_bytes();
        assert!(
            !is_styxl(std::str::from_utf8(&content).unwrap()),
            "RootKinograph::to_styx must emit legacy wrapped form for this test"
        );
        let stored = store_kino(
            root,
            StoreKinoParams {
                kind: "root".into(),
                content,
                author: "yj".into(),
                provenance: "reformat-test".into(),
                ts: ts.into(),
                metadata: BTreeMap::new(),
                id: None,
                parents: vec![],
            },
        )
        .unwrap();
        let hash = Hash::from_str(&stored.event.hash).unwrap();
        write_root_pointer(root, root_name, &hash).unwrap();
        stored.event
    }

    #[test]
    fn reformat_stages_new_version_for_legacy_kinograph_kino() {
        let (_t, root) = setup();
        let md = store_md(&root, b"hello", "hello", "2026-04-19T10:00:00Z");
        let kg_event = store_legacy_kinograph(
            &root,
            std::slice::from_ref(&md.id),
            "list",
            "2026-04-19T10:00:01Z",
        );

        // Commit so the kinograph kino is reachable from the inbox root.
        commit_all(&root, commit_params("2026-04-19T10:00:02Z")).unwrap();

        let report =
            reformat_repo(&root, reformat_params("2026-04-19T10:00:03Z")).unwrap();
        assert_eq!(
            report.reformatted_kinographs.len(),
            1,
            "expected exactly one reformatted kinograph, got {report:#?}"
        );
        let entry = &report.reformatted_kinographs[0];
        assert_eq!(entry.id, kg_event.id, "identity carried forward");
        assert_eq!(entry.prior_version, kg_event.hash, "parent = current head");
        assert_ne!(entry.new_version, kg_event.hash, "new version must differ");

        let new_hash = Hash::from_str(&entry.new_version).unwrap();
        let new_bytes = ContentStore::new(&root).read(&new_hash).unwrap();
        let new_text = std::str::from_utf8(&new_bytes).unwrap();
        assert!(
            is_styxl(new_text),
            "new blob should be styxl form; got {new_text:?}",
        );

        let events = Ledger::new(&root).read_all_events().unwrap();
        let new_event = events
            .iter()
            .find(|e| e.hash == entry.new_version)
            .expect("new event must be in staged ledger");
        assert_eq!(new_event.id, kg_event.id);
        assert_eq!(new_event.parents, vec![kg_event.hash.clone()]);
        assert_eq!(new_event.kind, "kinograph");
    }

    #[test]
    fn reformat_is_idempotent_on_already_styxl_kinograph_kino() {
        let (_t, root) = setup();
        let md = store_md(&root, b"hello", "hello", "2026-04-19T10:00:00Z");
        store_styxl_kinograph(
            &root,
            std::slice::from_ref(&md.id),
            "list",
            "2026-04-19T10:00:01Z",
        );
        commit_all(&root, commit_params("2026-04-19T10:00:02Z")).unwrap();

        let report =
            reformat_repo(&root, reformat_params("2026-04-19T10:00:03Z")).unwrap();
        assert!(report.reformatted_kinographs.is_empty());
        assert_eq!(report.skipped_kinographs_already_formatted, 1);
    }

    #[test]
    fn reformat_skips_markdown_and_text_kinos() {
        let (_t, root) = setup();
        store_md(&root, b"hello", "hello", "2026-04-19T10:00:00Z");
        let stored = store_kino(
            &root,
            StoreKinoParams {
                kind: "text".into(),
                content: b"plain text".to_vec(),
                author: "yj".into(),
                provenance: "reformat-test".into(),
                ts: "2026-04-19T10:00:01Z".into(),
                metadata: BTreeMap::from([("name".into(), "note".into())]),
                id: None,
                parents: vec![],
            },
        )
        .unwrap();
        let text_event = stored.event;
        commit_all(&root, commit_params("2026-04-19T10:00:02Z")).unwrap();

        let events_before = Ledger::new(&root).read_all_events().unwrap();

        let report =
            reformat_repo(&root, reformat_params("2026-04-19T10:00:03Z")).unwrap();
        assert!(report.reformatted_kinographs.is_empty());
        assert_eq!(report.skipped_kinographs_already_formatted, 0);

        let events_after = Ledger::new(&root).read_all_events().unwrap();
        assert_eq!(
            events_after.len(),
            events_before.len(),
            "no new events for markdown/text kinos"
        );
        let versions_for_text: Vec<&Event> = events_after
            .iter()
            .filter(|e| e.id == text_event.id && e.is_store_event())
            .collect();
        assert_eq!(versions_for_text.len(), 1);
    }

    #[test]
    fn reformat_rewrites_legacy_root_kinograph_and_updates_pointer() {
        let (_t, root) = setup();
        let md = store_md(&root, b"body", "body", "2026-04-19T10:00:00Z");
        let md_hash = Hash::from_str(&md.hash).unwrap();
        let entries = vec![crate::root::RootEntry::new(
            md.id.clone(),
            md_hash.as_hex(),
            "markdown",
            BTreeMap::from([("name".into(), "body".into())]),
        )];
        let prior_root = seed_legacy_root_pointer(
            &root,
            "inbox",
            entries,
            "2026-04-19T10:00:01Z",
        );

        let report =
            reformat_repo(&root, reformat_params("2026-04-19T10:00:02Z")).unwrap();
        assert_eq!(
            report.reformatted_roots.len(),
            1,
            "expected one reformatted root, got {report:#?}"
        );
        let reform = &report.reformatted_roots[0];
        assert_eq!(reform.root_name, "inbox");
        assert_eq!(reform.prior_version, prior_root.hash);
        assert_ne!(reform.new_version, prior_root.hash);

        let pointer_body = fs::read_to_string(root_pointer_path(&root, "inbox")).unwrap();
        assert_eq!(pointer_body.trim(), reform.new_version);

        let new_hash = Hash::from_str(&reform.new_version).unwrap();
        let new_bytes = ContentStore::new(&root).read(&new_hash).unwrap();
        assert!(is_styxl(std::str::from_utf8(&new_bytes).unwrap()));

        let events = Ledger::new(&root).read_all_events().unwrap();
        let new_event = events
            .iter()
            .find(|e| e.hash == reform.new_version)
            .expect("new root event should be staged");
        assert_eq!(new_event.kind, "root");
        assert_eq!(new_event.id, prior_root.id, "root identity carried forward");
        assert_eq!(new_event.parents, vec![prior_root.hash.clone()]);
    }

    #[test]
    fn reformat_is_idempotent_on_already_styxl_roots() {
        let (_t, root) = setup();
        store_md(&root, b"a", "a", "2026-04-19T10:00:00Z");
        commit_all(&root, commit_params("2026-04-19T10:00:01Z")).unwrap();

        let pointer_before =
            fs::read_to_string(root_pointer_path(&root, "inbox")).unwrap();

        let report =
            reformat_repo(&root, reformat_params("2026-04-19T10:00:02Z")).unwrap();
        assert!(report.reformatted_roots.is_empty());
        assert!(
            report.skipped_roots_already_formatted >= 1,
            "expected at least the inbox root to be counted as already-formatted",
        );

        let pointer_after =
            fs::read_to_string(root_pointer_path(&root, "inbox")).unwrap();
        assert_eq!(
            pointer_before, pointer_after,
            "pointer should not change on an already-styxl repo"
        );
    }

    #[test]
    fn reformat_recurses_into_nested_composition_entries() {
        let (_t, root) = setup();
        let leaf = store_md(&root, b"leaf", "leaf", "2026-04-19T10:00:00Z");
        let inner = store_legacy_kinograph(
            &root,
            std::slice::from_ref(&leaf.id),
            "inner",
            "2026-04-19T10:00:01Z",
        );
        let outer = store_legacy_kinograph(
            &root,
            std::slice::from_ref(&inner.id),
            "outer",
            "2026-04-19T10:00:02Z",
        );
        commit_all(&root, commit_params("2026-04-19T10:00:03Z")).unwrap();

        let report =
            reformat_repo(&root, reformat_params("2026-04-19T10:00:04Z")).unwrap();
        let mut ids: Vec<&str> = report
            .reformatted_kinographs
            .iter()
            .map(|e| e.id.as_str())
            .collect();
        ids.sort();
        let mut expected = vec![inner.id.as_str(), outer.id.as_str()];
        expected.sort();
        assert_eq!(
            ids, expected,
            "both outer and inner kinographs should have been reformatted",
        );
    }

    #[test]
    fn reformat_then_commit_makes_new_version_the_head() {
        let (_t, root) = setup();
        let md = store_md(&root, b"hello", "hello", "2026-04-19T10:00:00Z");
        let kg_event = store_legacy_kinograph(
            &root,
            std::slice::from_ref(&md.id),
            "list",
            "2026-04-19T10:00:01Z",
        );
        commit_all(&root, commit_params("2026-04-19T10:00:02Z")).unwrap();

        let _report =
            reformat_repo(&root, reformat_params("2026-04-19T10:00:03Z")).unwrap();
        let commit = commit_root(&root, "inbox", commit_params("2026-04-19T10:00:04Z"))
            .unwrap();
        let new_root_hash = commit.new_version.expect("inbox should advance");
        let new_root_bytes = ContentStore::new(&root).read(&new_root_hash).unwrap();
        let rk = RootKinograph::parse(&new_root_bytes).unwrap();
        let kg_entry = rk
            .entries
            .iter()
            .find(|e| e.id == kg_event.id)
            .expect("kinograph entry must be in the new root");
        assert_ne!(
            kg_entry.version, kg_event.hash,
            "post-reformat commit should bump the entry's version away from the legacy blob",
        );
    }
}
