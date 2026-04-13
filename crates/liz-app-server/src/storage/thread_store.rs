//! Thread store interfaces.

use crate::storage::error::StorageResult;
use liz_protocol::{Thread, ThreadId};

/// Minimal interface for thread persistence.
pub trait ThreadStore {
    /// Persists the provided thread record.
    fn put_thread(&self, thread: &Thread) -> StorageResult<()>;

    /// Reads a previously persisted thread record.
    fn get_thread(&self, thread_id: &ThreadId) -> StorageResult<Option<Thread>>;

    /// Lists all persisted thread records.
    fn list_threads(&self) -> StorageResult<Vec<Thread>>;
}
