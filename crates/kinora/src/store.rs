use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::hash::{Hash, HashParseError};
use crate::namespace::ext_for_kind;
use crate::paths::{find_blob_path, store_blob_path_with_ext, store_dir};

#[derive(Debug)]
pub enum StoreError {
    Io(io::Error),
    HashMismatch { expected: Hash, got: Hash, path: PathBuf },
    InvalidStoredHash { path: PathBuf, err: HashParseError },
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StoreError::Io(e) => write!(f, "content store io error: {e}"),
            StoreError::HashMismatch { expected, got, path } => write!(
                f,
                "content hash mismatch at {}: expected {expected}, got {got}",
                path.display()
            ),
            StoreError::InvalidStoredHash { path, err } => write!(
                f,
                "invalid hash in stored path {}: {err}",
                path.display()
            ),
        }
    }
}

impl std::error::Error for StoreError {}

impl From<io::Error> for StoreError {
    fn from(e: io::Error) -> Self {
        StoreError::Io(e)
    }
}

pub struct ContentStore {
    kinora_root: PathBuf,
}

impl ContentStore {
    pub fn new(kinora_root: impl Into<PathBuf>) -> Self {
        Self { kinora_root: kinora_root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.kinora_root
    }

    /// Write `content` to the store, tagging the on-disk filename with the
    /// extension derived from `kind` (e.g. `markdown` → `<hash>.md`). Dedup
    /// semantics are unchanged: the hash of the content is the identity; if
    /// a blob with that hash already exists under any extension, no new file
    /// is written and the existing path stands. Extensions are advisory UX
    /// — the authoritative `kind` lives in the ledger event.
    pub fn write(&self, kind: &str, content: &[u8]) -> Result<Hash, StoreError> {
        let hash = Hash::of_content(content);
        if find_blob_path(&self.kinora_root, &hash).is_some() {
            return Ok(hash);
        }
        let path = store_blob_path_with_ext(&self.kinora_root, &hash, ext_for_kind(kind));
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let tmp = path.with_extension("tmp");
        fs::write(&tmp, content)?;
        fs::rename(&tmp, &path)?;
        Ok(hash)
    }

    pub fn read(&self, hash: &Hash) -> Result<Vec<u8>, StoreError> {
        let path = find_blob_path(&self.kinora_root, hash).ok_or_else(|| {
            StoreError::Io(io::Error::new(
                io::ErrorKind::NotFound,
                format!("no blob for hash {}", hash.as_hex()),
            ))
        })?;
        let bytes = fs::read(&path)?;
        let actual = Hash::of_content(&bytes);
        if &actual != hash {
            return Err(StoreError::HashMismatch {
                expected: hash.clone(),
                got: actual,
                path,
            });
        }
        Ok(bytes)
    }

    pub fn exists(&self, hash: &Hash) -> bool {
        find_blob_path(&self.kinora_root, hash).is_some()
    }

    pub fn ensure_layout(&self) -> Result<(), StoreError> {
        fs::create_dir_all(store_dir(&self.kinora_root))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn store() -> (TempDir, ContentStore) {
        let tmp = TempDir::new().unwrap();
        let store = ContentStore::new(tmp.path().to_path_buf());
        store.ensure_layout().unwrap();
        (tmp, store)
    }

    #[test]
    fn write_then_read_roundtrips_bytes() {
        let (_tmp, store) = store();
        let content = b"kinora test content";
        let hash = store.write("markdown", content).unwrap();
        let out = store.read(&hash).unwrap();
        assert_eq!(out, content);
    }

    #[test]
    fn write_is_sharded_by_first_two_hex() {
        let (_tmp, store) = store();
        let hash = store.write("markdown", b"hello").unwrap();
        let expected_dir = store.root().join("store").join(hash.shard());
        assert!(expected_dir.is_dir(), "shard dir missing: {}", expected_dir.display());
        let expected_file = expected_dir.join(format!("{}.md", hash.as_hex()));
        assert!(expected_file.is_file(), "blob file missing: {}", expected_file.display());
    }

    #[test]
    fn write_is_idempotent() {
        let (_tmp, store) = store();
        let h1 = store.write("markdown", b"same").unwrap();
        let h2 = store.write("markdown", b"same").unwrap();
        assert_eq!(h1, h2);
        assert!(store.exists(&h1));
    }

    #[test]
    fn exists_returns_false_for_absent() {
        let (_tmp, store) = store();
        let absent: Hash = "00".repeat(32).parse().unwrap();
        assert!(!store.exists(&absent));
    }

    #[test]
    fn read_verifies_hash_and_detects_corruption() {
        let (_tmp, store) = store();
        let hash = store.write("markdown", b"authentic").unwrap();
        let path = find_blob_path(store.root(), &hash).unwrap();
        fs::write(&path, b"tampered").unwrap();
        let err = store.read(&hash).unwrap_err();
        assert!(matches!(err, StoreError::HashMismatch { .. }));
    }

    #[test]
    fn content_is_pure_no_injected_metadata() {
        let (_tmp, store) = store();
        let content = b"exact bytes";
        let hash = store.write("markdown", content).unwrap();
        let path = find_blob_path(store.root(), &hash).unwrap();
        let raw = fs::read(&path).unwrap();
        assert_eq!(raw, content);
    }

    #[test]
    fn large_content_roundtrips() {
        let (_tmp, store) = store();
        let content = vec![0xABu8; 10_000];
        let hash = store.write("markdown", &content).unwrap();
        let out = store.read(&hash).unwrap();
        assert_eq!(out, content);
    }

    #[test]
    fn write_uses_kind_derived_extension() {
        let (_tmp, store) = store();
        for (kind, ext) in [("markdown", Some("md")), ("text", Some("txt")), ("kinograph", Some("styx"))] {
            let content = format!("content-for-{kind}").into_bytes();
            let hash = store.write(kind, &content).unwrap();
            let path = find_blob_path(store.root(), &hash).unwrap();
            let filename = path.file_name().unwrap().to_string_lossy().into_owned();
            let expected = match ext {
                Some(e) => format!("{}.{e}", hash.as_hex()),
                None => hash.as_hex().to_owned(),
            };
            assert_eq!(filename, expected, "kind {kind} produced unexpected filename");
        }
    }

    #[test]
    fn write_with_binary_kind_has_no_extension() {
        let (_tmp, store) = store();
        let hash = store.write("binary", b"opaque").unwrap();
        let path = find_blob_path(store.root(), &hash).unwrap();
        assert_eq!(
            path.file_name().unwrap().to_string_lossy(),
            hash.as_hex()
        );
    }

    #[test]
    fn write_with_namespaced_kind_falls_back_to_bin_extension() {
        let (_tmp, store) = store();
        let hash = store.write("team::sketch", b"weird").unwrap();
        let path = find_blob_path(store.root(), &hash).unwrap();
        assert_eq!(
            path.file_name().unwrap().to_string_lossy(),
            format!("{}.bin", hash.as_hex())
        );
    }

    #[test]
    fn same_content_different_kind_dedupes_to_first_writer_extension() {
        let (_tmp, store) = store();
        let bytes = b"identical content";
        let h1 = store.write("markdown", bytes).unwrap();
        let h2 = store.write("text", bytes).unwrap();
        assert_eq!(h1, h2);
        let path = find_blob_path(store.root(), &h1).unwrap();
        // First writer (markdown) wins the extension; the text-kind call
        // deduped to the existing file.
        assert_eq!(
            path.file_name().unwrap().to_string_lossy(),
            format!("{}.md", h1.as_hex())
        );
    }

    #[test]
    fn read_finds_extensionless_legacy_blob() {
        // Simulate a blob written by an older kinora that did not append
        // extensions. `read` must still locate it by scanning the shard dir.
        let (_tmp, store) = store();
        let bytes = b"legacy blob";
        let hash = Hash::of_content(bytes);
        let legacy_path = store.root().join("store").join(hash.shard()).join(hash.as_hex());
        fs::create_dir_all(legacy_path.parent().unwrap()).unwrap();
        fs::write(&legacy_path, bytes).unwrap();
        let out = store.read(&hash).unwrap();
        assert_eq!(out, bytes);
    }

    #[test]
    fn read_errors_when_blob_absent() {
        let (_tmp, store) = store();
        let missing: Hash = "ab".repeat(32).parse().unwrap();
        let err = store.read(&missing).unwrap_err();
        assert!(matches!(err, StoreError::Io(_)));
    }
}
