//! Global memory store interfaces and persisted data shapes.

use crate::storage::error::StorageResult;
use crate::storage::fs::{ensure_layout, read_json, write_json};
use crate::storage::paths::StoragePaths;
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

/// Filesystem-backed global memory store.
#[derive(Debug, Clone)]
pub struct FsGlobalMemoryStore {
    paths: StoragePaths,
}

impl FsGlobalMemoryStore {
    /// Creates a filesystem-backed global memory store.
    pub fn new(paths: StoragePaths) -> Self {
        Self { paths }
    }
}

impl GlobalMemoryStore for FsGlobalMemoryStore {
    fn read_snapshot(&self) -> StorageResult<GlobalMemorySnapshot> {
        ensure_layout(&self.paths)?;

        Ok(read_json(&self.paths.global_memory_file())?.unwrap_or(GlobalMemorySnapshot {
            identity_summary: None,
            active_thread_ids: Vec::new(),
            facts: Vec::new(),
        }))
    }

    fn write_snapshot(&self, snapshot: &GlobalMemorySnapshot) -> StorageResult<()> {
        ensure_layout(&self.paths)?;
        write_json(&self.paths.global_memory_file(), snapshot)
    }
}
