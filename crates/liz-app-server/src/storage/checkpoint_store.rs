//! Checkpoint store interfaces.

use crate::storage::error::StorageResult;
use liz_protocol::{Checkpoint, CheckpointId};

/// Minimal interface for checkpoint persistence.
pub trait CheckpointStore {
    /// Persists a checkpoint.
    fn put_checkpoint(&self, checkpoint: &Checkpoint) -> StorageResult<()>;

    /// Reads a previously persisted checkpoint.
    fn get_checkpoint(&self, checkpoint_id: &CheckpointId) -> StorageResult<Option<Checkpoint>>;
}
