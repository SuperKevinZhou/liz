//! Checkpoint store interfaces.

use crate::storage::error::StorageResult;
use crate::storage::fs::{ensure_layout, read_json, write_json};
use crate::storage::paths::StoragePaths;
use liz_protocol::{Checkpoint, CheckpointId};

/// Minimal interface for checkpoint persistence.
pub trait CheckpointStore {
    /// Persists a checkpoint.
    fn put_checkpoint(&self, checkpoint: &Checkpoint) -> StorageResult<()>;

    /// Reads a previously persisted checkpoint.
    fn get_checkpoint(&self, checkpoint_id: &CheckpointId) -> StorageResult<Option<Checkpoint>>;
}

/// Filesystem-backed checkpoint store.
#[derive(Debug, Clone)]
pub struct FsCheckpointStore {
    paths: StoragePaths,
}

impl FsCheckpointStore {
    /// Creates a filesystem-backed checkpoint store.
    pub fn new(paths: StoragePaths) -> Self {
        Self { paths }
    }
}

impl CheckpointStore for FsCheckpointStore {
    fn put_checkpoint(&self, checkpoint: &Checkpoint) -> StorageResult<()> {
        ensure_layout(&self.paths)?;
        write_json(&self.paths.checkpoint_file(&checkpoint.id), checkpoint)
    }

    fn get_checkpoint(&self, checkpoint_id: &CheckpointId) -> StorageResult<Option<Checkpoint>> {
        ensure_layout(&self.paths)?;
        read_json(&self.paths.checkpoint_file(checkpoint_id))
    }
}
