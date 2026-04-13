//! Global memory store interfaces and persisted data shapes.

use crate::storage::error::StorageResult;
use liz_protocol::{MemoryFactId, ThreadId, Timestamp};
use serde::{Deserialize, Serialize};

/// A stored memory fact kept in the global memory store.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredMemoryFact {
    /// The fact identifier.
    pub id: MemoryFactId,
    /// The stable subject of the fact.
    pub subject: String,
    /// The fact description or value.
    pub value: String,
    /// The timestamp when the fact was updated.
    pub updated_at: Timestamp,
}

/// The persisted shape used by the global memory store.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GlobalMemorySnapshot {
    /// The current identity summary, if known.
    pub identity_summary: Option<String>,
    /// The thread identifiers currently considered active.
    pub active_thread_ids: Vec<ThreadId>,
    /// The stored memory facts.
    pub facts: Vec<StoredMemoryFact>,
}

/// Minimal interface for the global memory store.
pub trait GlobalMemoryStore {
    /// Reads the persisted global memory snapshot.
    fn read_snapshot(&self) -> StorageResult<GlobalMemorySnapshot>;

    /// Persists the global memory snapshot atomically for future reads.
    fn write_snapshot(&self, snapshot: &GlobalMemorySnapshot) -> StorageResult<()>;
}
