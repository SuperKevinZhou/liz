//! Append-only turn log interfaces.

use crate::storage::error::StorageResult;
use crate::storage::paths::StoragePaths;
use liz_protocol::{ArtifactId, ThreadId, Timestamp, TurnId};
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, ErrorKind, Write};

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
    /// Related artifact identifiers created by the event.
    #[serde(default)]
    pub artifact_ids: Vec<ArtifactId>,
}

/// Minimal interface for append-only turn logs.
pub trait TurnLog {
    /// Appends a new log entry for a thread.
    fn append_entry(&self, entry: &TurnLogEntry) -> StorageResult<()>;

    /// Reads every log entry for a thread in append order.
    fn read_entries(&self, thread_id: &ThreadId) -> StorageResult<Vec<TurnLogEntry>>;
}

/// Filesystem-backed append-only turn log.
#[derive(Debug, Clone)]
pub struct FsTurnLog {
    paths: StoragePaths,
}

impl FsTurnLog {
    /// Creates a filesystem-backed turn log.
    pub fn new(paths: StoragePaths) -> Self {
        Self { paths }
    }
}

impl TurnLog for FsTurnLog {
    fn append_entry(&self, entry: &TurnLogEntry) -> StorageResult<()> {
        let path = self.paths.turn_log_file(&entry.thread_id);

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut file = OpenOptions::new().append(true).create(true).open(path)?;
        serde_json::to_writer(&mut file, entry)?;
        writeln!(file)?;
        Ok(())
    }

    fn read_entries(&self, thread_id: &ThreadId) -> StorageResult<Vec<TurnLogEntry>> {
        let path = self.paths.turn_log_file(thread_id);
        let file = match fs::File::open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error.into()),
        };

        let reader = BufReader::new(file);
        let mut entries = Vec::new();

        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }

            entries.push(serde_json::from_str(&line)?);
        }

        Ok(entries)
    }
}
