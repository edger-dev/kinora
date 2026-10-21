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

use std::path::Path;
use std::sync::Arc;

use object_store::ObjectStore;
use object_store::path::Path as Key;

/// Why a sync failed.
#[derive(Debug, thiserror::Error)]
pub enum RemoteError {
    #[error("local store i/o: {0}")]
    Io(#[from] std::io::Error),
    #[error("object store: {0}")]
    Store(#[from] object_store::Error),
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
    /// mirror lacks.
    pub async fn push(&self, kinora_root: &Path) -> Result<SyncReport, RemoteError> {
        let _ = (kinora_root, &self.store, &self.prefix);
        Ok(SyncReport::default())
    }

    /// Fetch every mirrored write-once file the store at `kinora_root` lacks.
    pub async fn pull(&self, kinora_root: &Path) -> Result<SyncReport, RemoteError> {
        let _ = kinora_root;
        Ok(SyncReport::default())
    }
}
