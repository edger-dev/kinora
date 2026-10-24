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

use std::collections::{BTreeSet, HashMap, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use moco_place::{FsPlace, Key, Stream, read_all};

use crate::hash::Hash;
use crate::paths::{root_pointer_path, roots_dir};
use crate::root::RootKinograph;
use crate::store::{ContentStore, StoreError};

/// Root versions are root kinographs in the styxl format.
const EXT: &str = "styxl";
/// An interrupted migration's pointer copy: `.<name>.legacy`.
const ASIDE: &str = ".legacy";

/// Why a root's versions could not be read or written.
#[derive(Debug, thiserror::Error)]
pub enum RootsError {
    #[error("roots i/o: {0}")]
    Io(#[from] std::io::Error),
    #[error("roots place: {0}")]
    Place(#[from] moco_place::PlaceError),
    #[error("roots content store: {0}")]
    Store(#[from] StoreError),
    #[error("roots kinograph: {0}")]
    Root(#[from] crate::root::RootError),
    #[error("invalid root pointer at {}: {body:?}", .path.display())]
    InvalidPointer { path: PathBuf, body: String },
    #[error("root {name:?} cannot be a stream name: {reason}")]
    InvalidName { name: String, reason: String },
}

/// Every root with committed versions, sorted: streams and legacy pointers
/// (an interrupted migration's aside copy, and git's conflict copies
/// `<name>~<branch>`, counted under their root).
pub fn root_names(kinora_root: &Path) -> Result<Vec<String>, RootsError> {
    let entries = match fs::read_dir(roots_dir(kinora_root)) {
        Ok(entries) => entries,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.into()),
    };
    let mut names = BTreeSet::new();
    for entry in entries {
        let entry = entry?;
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if let Some(rest) = name.strip_prefix('.') {
            if let Some(aside) = rest.strip_suffix(ASIDE) {
                names.insert(aside.to_owned());
            }
            continue;
        }
        let kind = entry.file_type()?;
        if !(kind.is_dir() || kind.is_file()) {
            continue;
        }
        match name.split_once('~') {
            Some((root, _)) if kind.is_file() && !root.is_empty() => {
                names.insert(root.to_owned());
            }
            Some(_) => {}
            None => {
                names.insert(name);
            }
        }
    }
    Ok(names.into_iter().collect())
}

/// A root's current versions: the stream's heads, then any legacy pointer's
/// version the stream neither holds nor supersedes. One normally, several
/// after concurrent commits, none before the first commit. Reading writes
/// nothing.
pub fn root_heads(kinora_root: &Path, name: &str) -> Result<Vec<Hash>, RootsError> {
    // Every version heard of — the stream's entries, then the legacy
    // pointers' — with the parents each names.
    let mut candidates: Vec<Hash> = Vec::new();
    let mut superseded: HashSet<Hash> = HashSet::new();
    if root_pointer_path(kinora_root, name).is_dir() {
        let place = place(kinora_root);
        for entry in stream_for(name)?.entries(&place)? {
            superseded.extend(parents_of(&read_all(&place, &entry.key)?));
            candidates.push(entry.hash);
        }
    }
    let store = ContentStore::new(kinora_root);
    for path in legacy_pointers(kinora_root, name)? {
        let hash = read_pointer(&path)?;
        // A legacy version missing from the store is still a head: nothing
        // is known to supersede it.
        if let Ok(bytes) = store.read(&hash) {
            superseded.extend(parents_of(&bytes));
        }
        candidates.push(hash);
    }
    let mut heads: Vec<Hash> = Vec::new();
    for hash in candidates {
        if !superseded.contains(&hash) {
            heads.retain(|h| h != &hash);
            heads.push(hash);
        }
    }
    Ok(heads)
}

/// Append `bytes` (a root kinograph) as the root's newest version, migrating
/// legacy pointers first; the version's hash.
pub fn append_root_version(
    kinora_root: &Path,
    name: &str,
    bytes: &[u8],
) -> Result<Hash, RootsError> {
    migrate_root(kinora_root, name)?;
    Ok(stream_for(name)?.append(&place(kinora_root), bytes)?.hash)
}

/// Move a root's legacy pointers onto its stream — the pointer file (renamed
/// aside first, so a crash at any point leaves the version readable), an
/// earlier aside copy, git's conflict copies; whether there was any.
pub fn migrate_root(kinora_root: &Path, name: &str) -> Result<bool, RootsError> {
    let stream = stream_for(name)?;
    let pointer = root_pointer_path(kinora_root, name);
    let aside = aside_path(kinora_root, name);
    let mut moved = false;
    // An earlier aside first, so renaming the pointer cannot overwrite it.
    if aside.is_file() {
        stream_pointer(kinora_root, &stream, &aside)?;
        moved = true;
    }
    if pointer.is_file() {
        match fs::rename(&pointer, &aside) {
            Ok(()) => {
                stream_pointer(kinora_root, &stream, &aside)?;
                moved = true;
            }
            // Another commit on this machine moved it first.
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    for stray in conflict_copies(kinora_root, name)? {
        stream_pointer(kinora_root, &stream, &stray)?;
        moved = true;
    }
    Ok(moved)
}

/// Stream the version a pointer file names (unless the stream holds it),
/// then remove the file.
fn stream_pointer(kinora_root: &Path, stream: &Stream, path: &Path) -> Result<(), RootsError> {
    let hash = read_pointer(path)?;
    let place = place(kinora_root);
    if !stream.entries(&place)?.iter().any(|e| e.hash == hash) {
        let bytes = ContentStore::new(kinora_root).read(&hash)?;
        stream.append(&place, &bytes)?;
    }
    match fs::remove_file(path) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e.into()),
        _ => Ok(()),
    }
}

/// What a root's history tells about its heads: their common base version,
/// every root version between the heads and that base, and the kino-version
/// ancestry those versions (and any events added) record.
///
/// Root versions name their prior versions in their headers, so the walk
/// needs only the content store. Only walked when a root has several heads.
#[derive(Debug, Default)]
pub struct RootHistory {
    /// The latest version every head descends from; `None` when the heads
    /// share no history (two first commits).
    pub base: Option<RootKinograph>,
    /// The root versions from each head back to the base, inclusive.
    pub walked: Vec<RootKinograph>,
    /// Kino version → its parents.
    ancestry: HashMap<String, Vec<String>>,
}

impl RootHistory {
    /// Walk the history of `heads` through the content store. A version
    /// missing from the store ends its branch of the walk.
    pub fn walk(kinora_root: &Path, heads: &[Hash]) -> Result<Self, RootsError> {
        let store = ContentStore::new(kinora_root);
        let mut versions: HashMap<String, RootKinograph> = HashMap::new();
        let mut ancestors_of: Vec<HashSet<String>> = Vec::new();
        for head in heads {
            let mut seen = HashSet::new();
            let mut queue = vec![head.as_hex().to_owned()];
            while let Some(hex) = queue.pop() {
                if !seen.insert(hex.clone()) {
                    continue;
                }
                let kg = match versions.get(&hex) {
                    Some(kg) => kg.clone(),
                    None => {
                        let Ok(hash) = Hash::from_str(&hex) else {
                            continue;
                        };
                        let Ok(bytes) = store.read(&hash) else {
                            continue;
                        };
                        let kg = RootKinograph::parse(&bytes)?;
                        versions.insert(hex.clone(), kg.clone());
                        kg
                    }
                };
                queue.extend(kg.header.parents.iter().cloned());
            }
            ancestors_of.push(seen);
        }
        // The common ancestors, and among them the latest: one no other
        // common ancestor descends from. Ties go to the smallest hash.
        let mut common: Vec<String> = ancestors_of
            .first()
            .map(|first| {
                first
                    .iter()
                    .filter(|v| ancestors_of.iter().all(|a| a.contains(*v)))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        common.sort();
        let below = |v: &str| -> HashSet<String> {
            let mut seen = HashSet::new();
            let mut queue: Vec<String> = versions
                .get(v)
                .map(|kg| kg.header.parents.clone())
                .unwrap_or_default();
            while let Some(hex) = queue.pop() {
                if seen.insert(hex.clone())
                    && let Some(kg) = versions.get(&hex)
                {
                    queue.extend(kg.header.parents.iter().cloned());
                }
            }
            seen
        };
        let base_hex = common
            .iter()
            .find(|c| !common.iter().any(|d| d != *c && below(d).contains(*c)))
            .cloned();
        let older = base_hex.as_deref().map(below).unwrap_or_default();
        let mut walked_hex: Vec<String> = ancestors_of
            .iter()
            .flatten()
            .filter(|v| !older.contains(*v))
            .cloned()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        walked_hex.retain(|v| versions.contains_key(v));
        let mut history = Self {
            base: base_hex.and_then(|b| versions.get(&b).cloned()),
            walked: walked_hex
                .iter()
                .filter_map(|v| versions.get(v).cloned())
                .collect(),
            ancestry: HashMap::new(),
        };
        let walked = std::mem::take(&mut history.walked);
        for kg in &walked {
            for entry in &kg.entries {
                history.add(&entry.version, &entry.parents);
            }
        }
        history.walked = walked;
        Ok(history)
    }

    /// Record that `version` has `parents`.
    pub fn add(&mut self, version: &str, parents: &[String]) {
        let known = self.ancestry.entry(version.to_owned()).or_default();
        for p in parents {
            if !known.contains(p) {
                known.push(p.clone());
            }
        }
    }

    /// Whether kino version `a` is `b` or descends from it, as far as the
    /// recorded ancestry shows.
    pub fn descends(&self, a: &str, b: &str) -> bool {
        let mut seen = HashSet::new();
        let mut queue = vec![a.to_owned()];
        while let Some(v) = queue.pop() {
            if v == b {
                return true;
            }
            if seen.insert(v.clone())
                && let Some(parents) = self.ancestry.get(&v)
            {
                queue.extend(parents.iter().cloned());
            }
        }
        false
    }
}

/// The legacy pointer files of a root: the pointer itself, an aside copy,
/// git's conflict copies.
fn legacy_pointers(kinora_root: &Path, name: &str) -> Result<Vec<PathBuf>, RootsError> {
    let mut paths = Vec::new();
    let pointer = root_pointer_path(kinora_root, name);
    if pointer.is_file() {
        paths.push(pointer);
    }
    let aside = aside_path(kinora_root, name);
    if aside.is_file() {
        paths.push(aside);
    }
    paths.extend(conflict_copies(kinora_root, name)?);
    Ok(paths)
}

/// `roots/<name>~<anything>` files: what git leaves when one branch turned
/// the pointer into a stream directory and another rewrote the pointer.
fn conflict_copies(kinora_root: &Path, name: &str) -> Result<Vec<PathBuf>, RootsError> {
    let entries = match fs::read_dir(roots_dir(kinora_root)) {
        Ok(entries) => entries,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.into()),
    };
    let lead = format!("{name}~");
    let mut copies = Vec::new();
    for entry in entries {
        let entry = entry?;
        let is_copy = entry
            .file_name()
            .to_str()
            .is_some_and(|n| n.starts_with(&lead));
        if is_copy && entry.file_type()?.is_file() {
            copies.push(entry.path());
        }
    }
    copies.sort();
    Ok(copies)
}

/// The ledger as a place: a directory inside a repository, no marker.
fn place(kinora_root: &Path) -> FsPlace {
    FsPlace::within(kinora_root)
}

fn stream_for(name: &str) -> Result<Stream, RootsError> {
    let invalid = |e: moco_place::KeyError| RootsError::InvalidName {
        name: name.to_owned(),
        reason: e.to_string(),
    };
    let key = Key::new("roots")
        .map_err(invalid)?
        .join(name)
        .map_err(invalid)?;
    Ok(Stream::new(key).with_ext(EXT))
}

fn aside_path(kinora_root: &Path, name: &str) -> PathBuf {
    roots_dir(kinora_root).join(format!(".{name}{ASIDE}"))
}

fn read_pointer(path: &Path) -> Result<Hash, RootsError> {
    let body = fs::read_to_string(path)?;
    Hash::from_str(body.trim_end_matches(['\r', '\n'])).map_err(|_| RootsError::InvalidPointer {
        path: path.to_path_buf(),
        body,
    })
}

/// A root version's parents, from its header line alone; none if it does
/// not parse (a corrupt entry stays a head, for a human to see).
fn parents_of(bytes: &[u8]) -> Vec<Hash> {
    let header = bytes.split(|b| *b == b'\n').next().unwrap_or_default();
    let mut line = header.to_vec();
    line.push(b'\n');
    RootKinograph::parse(&line)
        .map(|kg| {
            kg.header
                .parents
                .iter()
                .filter_map(|p| p.parse().ok())
                .collect()
        })
        .unwrap_or_default()
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

    /// An entry named after `name` (its id and version are hashes of it).
    fn entry(name: &str) -> RootEntry {
        let id = Hash::of_content(name.as_bytes());
        let version = Hash::of_content(format!("{name} v1").as_bytes());
        RootEntry::new(
            id.as_hex(),
            version.as_hex(),
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

    /// A git merge where one branch migrated and the other rewrote the
    /// pointer leaves git's conflict copy `roots/<name>~<branch>`: its
    /// version is a head of `<name>`, and a migration streams it in.
    #[test]
    fn a_merge_conflict_copy_is_a_head_and_migrates() {
        let (_t, root) = kin();
        let base = append_root_version(&root, "inbox", &version(&[], &["a"])).unwrap();
        let theirs_bytes = version(&[&base], &["a", "t"]);
        let theirs = ContentStore::new(&root)
            .write("root", &theirs_bytes)
            .unwrap();
        fs::write(roots_dir(&root).join("inbox~theirs"), theirs.as_hex()).unwrap();
        assert_eq!(root_names(&root).unwrap(), vec!["inbox".to_owned()]);
        assert_eq!(root_heads(&root, "inbox").unwrap(), vec![theirs.clone()]);
        assert!(migrate_root(&root, "inbox").unwrap());
        assert!(!roots_dir(&root).join("inbox~theirs").exists());
        assert_eq!(root_heads(&root, "inbox").unwrap(), vec![theirs]);
    }

    /// An aside copy beside a stream that has heads is still a head, unless
    /// the stream holds it or supersedes it.
    #[test]
    fn an_aside_beside_a_stream_is_still_a_head() {
        let (_t, root) = kin();
        let a = append_root_version(&root, "inbox", &version(&[], &["a"])).unwrap();
        let z = ContentStore::new(&root)
            .write("root", &version(&[], &["z"]))
            .unwrap();
        fs::write(roots_dir(&root).join(".inbox.legacy"), z.as_hex()).unwrap();
        assert_eq!(
            root_heads(&root, "inbox").unwrap(),
            vec![a.clone(), z.clone()]
        );
        let fold = append_root_version(&root, "inbox", &version(&[&a, &z], &["a", "z"])).unwrap();
        assert_eq!(root_heads(&root, "inbox").unwrap(), vec![fold]);
    }

    /// Reading heads writes nothing into the ledger.
    #[test]
    fn reading_writes_nothing() {
        let (_t, root) = kin();
        append_root_version(&root, "inbox", &version(&[], &["a"])).unwrap();
        assert!(
            !root.join(".moco-place").exists(),
            "no marker in a repository"
        );
        let before: Vec<_> = fs::read_dir(roots_dir(&root).join("inbox"))
            .unwrap()
            .collect();
        root_heads(&root, "inbox").unwrap();
        root_names(&root).unwrap();
        let after: Vec<_> = fs::read_dir(roots_dir(&root).join("inbox"))
            .unwrap()
            .collect();
        assert_eq!(before.len(), after.len());
    }

    /// A migration whose version is missing from the store fails and keeps
    /// the aside copy, still readable.
    #[test]
    fn a_migration_missing_its_version_keeps_the_aside() {
        let (_t, root) = kin();
        let ghost = Hash::of_content(b"never stored");
        fs::create_dir_all(roots_dir(&root)).unwrap();
        fs::write(root_pointer_path(&root, "specs"), ghost.as_hex()).unwrap();
        assert!(migrate_root(&root, "specs").is_err());
        assert_eq!(root_heads(&root, "specs").unwrap(), vec![ghost]);
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
