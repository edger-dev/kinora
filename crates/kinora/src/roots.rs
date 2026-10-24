//! A root's versions, as an append-only stream.
//!
//! A commit appends the root's new version — the root kinograph's bytes — as
//! an entry of moco's stream `roots/<name>/<ts>-<hash>.styxl`, instead of
//! overwriting a pointer file `roots/<name>`. Two commits made concurrently
//! (two branches, two machines) leave two heads; readers take in every
//! head, and the next commit folds them. Added files never conflict in git,
//! so commit is no longer main-only.
//!
//! A legacy pointer file is read as a one-head root until the next commit
//! migrates it: renamed aside to `.<name>.legacy`, streamed, removed. If a
//! crash interrupts, the aside copy is read until a migration completes.
//!
//! implements: 02be1c37832e1c710030e7bc5a89441b55bdbf5a05ebe2c501f824708b7f3372@02be1c37832e1c710030e7bc5a89441b55bdbf5a05ebe2c501f824708b7f3372
//! task: a1bac9ab38a30da7fd5d5dde31176490b567a3a9625d4eb119649175bac7e9b6@90d573a7ed7cedc54296d959ee6c4a67ad6d471804cf1397a361652155ab4987

use std::path::{Path, PathBuf};

use crate::hash::Hash;

/// Why a root's versions could not be read or written.
#[derive(Debug, thiserror::Error)]
pub enum RootsError {
    #[error("roots i/o: {0}")]
    Io(#[from] std::io::Error),
    #[error("roots place: {0}")]
    Place(#[from] moco_place::PlaceError),
    #[error("invalid root pointer at {}: {body:?}", .path.display())]
    InvalidPointer { path: PathBuf, body: String },
    #[error("root {name:?} cannot be a stream name: {reason}")]
    InvalidName { name: String, reason: String },
}

/// Every root with committed versions, sorted: streams and legacy pointers.
pub fn root_names(_kinora_root: &Path) -> Result<Vec<String>, RootsError> {
    Ok(Vec::new())
}

/// A root's current versions, oldest first: one normally, several after
/// concurrent commits, none before the first commit.
pub fn root_heads(_kinora_root: &Path, _name: &str) -> Result<Vec<Hash>, RootsError> {
    Ok(Vec::new())
}

/// Append `bytes` (a root kinograph) as the root's newest version, migrating
/// a legacy pointer first; the version's hash.
pub fn append_root_version(
    _kinora_root: &Path,
    _name: &str,
    bytes: &[u8],
) -> Result<Hash, RootsError> {
    Ok(Hash::of_content(bytes))
}

/// Move a legacy pointer onto the stream; whether there was one to move.
pub fn migrate_root(_kinora_root: &Path, _name: &str) -> Result<bool, RootsError> {
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::{root_pointer_path, roots_dir};
    use crate::root::{RootEntry, RootKinograph};
    use crate::store::ContentStore;
    use std::fs;
    use tempfile::TempDir;

    fn kin() -> (TempDir, PathBuf) {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().join(".kinora");
        fs::create_dir_all(&root).unwrap();
        ContentStore::new(&root).ensure_layout().unwrap();
        (tmp, root)
    }

    fn entry(id: &str) -> RootEntry {
        let h = Hash::of_content(id.as_bytes());
        RootEntry::new(
            id,
            h.as_hex(),
            "markdown",
            Default::default(),
            "2026-10-24T00:00:00Z",
        )
    }

    /// A root version: genesis when `parents` is empty, else a child.
    fn version(parents: &[&Hash], ids: &[&str]) -> Vec<u8> {
        let entries = ids.iter().map(|id| entry(id)).collect();
        let kg = if parents.is_empty() {
            RootKinograph::new_genesis(
                entries,
                "2026-10-24T00:00:00Z".into(),
                "t".into(),
                "test".into(),
            )
            .unwrap()
        } else {
            RootKinograph::new_child(
                "lineage".to_owned(),
                parents.iter().map(|h| h.as_hex().to_owned()).collect(),
                entries,
                "2026-10-24T00:00:01Z".into(),
                "t".into(),
                "test".into(),
            )
        };
        kg.to_styxl().unwrap().into_bytes()
    }

    /// A legacy pointer to a version stored in the content store.
    fn legacy(root: &Path, name: &str, bytes: &[u8]) -> Hash {
        let hash = ContentStore::new(root).write("root", bytes).unwrap();
        fs::create_dir_all(roots_dir(root)).unwrap();
        fs::write(root_pointer_path(root, name), hash.as_hex()).unwrap();
        hash
    }

    #[test]
    fn no_versions_no_heads() {
        let (_t, root) = kin();
        assert_eq!(root_heads(&root, "inbox").unwrap(), vec![]);
        assert_eq!(root_names(&root).unwrap(), Vec::<String>::new());
    }

    #[test]
    fn a_legacy_pointer_is_one_head() {
        let (_t, root) = kin();
        let h = legacy(&root, "specs", &version(&[], &["a"]));
        assert_eq!(root_heads(&root, "specs").unwrap(), vec![h]);
        assert_eq!(root_names(&root).unwrap(), vec!["specs".to_owned()]);
    }

    #[test]
    fn appended_versions_have_one_head_until_they_fork() {
        let (_t, root) = kin();
        let v1 = append_root_version(&root, "tasks", &version(&[], &["a"])).unwrap();
        assert_eq!(root_heads(&root, "tasks").unwrap(), vec![v1.clone()]);
        let v2 = append_root_version(&root, "tasks", &version(&[&v1], &["a", "b"])).unwrap();
        assert_eq!(root_heads(&root, "tasks").unwrap(), vec![v2.clone()]);
        // Another branch committed from v2 too.
        let v3 = append_root_version(&root, "tasks", &version(&[&v2], &["a", "b", "c"])).unwrap();
        let v4 = append_root_version(&root, "tasks", &version(&[&v2], &["a", "b", "d"])).unwrap();
        assert_eq!(
            root_heads(&root, "tasks").unwrap(),
            vec![v3.clone(), v4.clone()]
        );
        let fold =
            append_root_version(&root, "tasks", &version(&[&v3, &v4], &["a", "b", "c", "d"]))
                .unwrap();
        assert_eq!(root_heads(&root, "tasks").unwrap(), vec![fold]);
        assert!(roots_dir(&root).join("tasks").is_dir());
    }

    /// Versions are entries of the stream, named `<ts>-<hash>.styxl`.
    #[test]
    fn a_version_is_a_stream_entry() {
        let (_t, root) = kin();
        let bytes = version(&[], &["a"]);
        let h = append_root_version(&root, "inbox", &bytes).unwrap();
        let dir = roots_dir(&root).join("inbox");
        let names: Vec<String> = fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names.len(), 1, "{names:?}");
        assert!(names[0].ends_with(&format!("-{h}.styxl")), "{names:?}");
        assert_eq!(fs::read(dir.join(&names[0])).unwrap(), bytes);
    }

    /// The next append migrates a legacy pointer: its version becomes the
    /// stream's first entry, the pointer file is gone.
    #[test]
    fn an_append_migrates_a_legacy_pointer() {
        let (_t, root) = kin();
        let old = legacy(&root, "specs", &version(&[], &["a"]));
        let new = append_root_version(&root, "specs", &version(&[&old], &["a", "b"])).unwrap();
        assert!(roots_dir(&root).join("specs").is_dir());
        assert!(!root_pointer_path(&root, "specs").is_file());
        assert_eq!(root_heads(&root, "specs").unwrap(), vec![new]);
        assert_eq!(root_names(&root).unwrap(), vec!["specs".to_owned()]);
    }

    #[test]
    fn a_migration_alone_keeps_the_head() {
        let (_t, root) = kin();
        let old = legacy(&root, "specs", &version(&[], &["a"]));
        assert!(migrate_root(&root, "specs").unwrap());
        assert!(
            !migrate_root(&root, "specs").unwrap(),
            "nothing left to migrate"
        );
        assert_eq!(root_heads(&root, "specs").unwrap(), vec![old]);
    }

    /// Interrupted after renaming the pointer aside: the aside copy is read,
    /// and the next migration completes.
    #[test]
    fn an_interrupted_migration_reads_the_aside_copy() {
        let (_t, root) = kin();
        let old = legacy(&root, "specs", &version(&[], &["a"]));
        fs::rename(
            root_pointer_path(&root, "specs"),
            roots_dir(&root).join(".specs.legacy"),
        )
        .unwrap();
        assert_eq!(root_heads(&root, "specs").unwrap(), vec![old.clone()]);
        assert_eq!(root_names(&root).unwrap(), vec!["specs".to_owned()]);
        assert!(migrate_root(&root, "specs").unwrap());
        assert!(!roots_dir(&root).join(".specs.legacy").exists());
        assert_eq!(root_heads(&root, "specs").unwrap(), vec![old]);
    }

    /// Temporaries and other dot-names are not roots.
    #[test]
    fn temporaries_are_not_roots() {
        let (_t, root) = kin();
        legacy(&root, "a", &version(&[], &["x"]));
        fs::write(roots_dir(&root).join(".a.tmp"), "junk").unwrap();
        append_root_version(&root, "b", &version(&[], &["y"])).unwrap();
        assert_eq!(
            root_names(&root).unwrap(),
            vec!["a".to_owned(), "b".to_owned()]
        );
    }
}
