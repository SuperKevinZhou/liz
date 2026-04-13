//! Append-only turn log interfaces.

use crate::storage::error::StorageResult;
use liz_protocol::{ThreadId, Timestamp, TurnId};
use serde::{Deserialize, Serialize};

/// A single append-only record in a thread turn log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurnLogEntry {
    /// The thread the entry belongs to.
    pub thread_id: ThreadId,
    /// The monotonically increasing sequence number for the thread.
    pub sequence: u64,
    /// The related turn identifier, if any.
    pub turn_id: Option<TurnId>,
    /// The timestamp when the record was produced.
    pub recorded_at: Timestamp,
    /// A stable event label for the record.
    pub event: String,
    /// A short summary of what happened.
    pub summary: String,
}

/// Minimal interface for append-only turn logs.
pub trait TurnLog {
    /// Appends a new log entry for a thread.
    fn append_entry(&self, entry: &TurnLogEntry) -> StorageResult<()>;

    /// Reads every log entry for a thread in append order.
    fn read_entries(&self, thread_id: &ThreadId) -> StorageResult<Vec<TurnLogEntry>>;
}
