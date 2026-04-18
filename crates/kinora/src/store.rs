use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::hash::{Hash, HashParseError};
use crate::paths::{store_blob_path, store_dir};

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

    pub fn write(&self, content: &[u8]) -> Result<Hash, StoreError> {
        let hash = Hash::of_content(content);
        let path = store_blob_path(&self.kinora_root, &hash);
        if path.exists() {
            return Ok(hash);
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let tmp = path.with_extension("tmp");
        fs::write(&tmp, content)?;
        fs::rename(&tmp, &path)?;
        Ok(hash)
    }

    pub fn read(&self, hash: &Hash) -> Result<Vec<u8>, StoreError> {
        let path = store_blob_path(&self.kinora_root, hash);
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
        store_blob_path(&self.kinora_root, hash).is_file()
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
        let hash = store.write(content).unwrap();
        let out = store.read(&hash).unwrap();
        assert_eq!(out, content);
    }

    #[test]
    fn write_is_sharded_by_first_two_hex() {
        let (_tmp, store) = store();
        let hash = store.write(b"hello").unwrap();
        let expected_dir = store.root().join("store").join(hash.shard());
        assert!(expected_dir.is_dir(), "shard dir missing: {}", expected_dir.display());
        let expected_file = expected_dir.join(hash.as_hex());
        assert!(expected_file.is_file(), "blob file missing: {}", expected_file.display());
    }

    #[test]
    fn write_is_idempotent() {
        let (_tmp, store) = store();
        let h1 = store.write(b"same").unwrap();
        let h2 = store.write(b"same").unwrap();
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
        let hash = store.write(b"authentic").unwrap();
        let path = store_blob_path(store.root(), &hash);
        fs::write(&path, b"tampered").unwrap();
        let err = store.read(&hash).unwrap_err();
        assert!(matches!(err, StoreError::HashMismatch { .. }));
    }

    #[test]
    fn content_is_pure_no_injected_metadata() {
        let (_tmp, store) = store();
        let content = b"exact bytes";
        let hash = store.write(content).unwrap();
        let raw = fs::read(store_blob_path(store.root(), &hash)).unwrap();
        assert_eq!(raw, content);
    }

    #[test]
    fn large_content_roundtrips() {
        let (_tmp, store) = store();
        let content = vec![0xABu8; 10_000];
        let hash = store.write(&content).unwrap();
        let out = store.read(&hash).unwrap();
        assert_eq!(out, content);
    }
}
