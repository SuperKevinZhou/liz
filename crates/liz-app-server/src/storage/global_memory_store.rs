//! Global memory store interfaces and persisted data shapes.

use crate::storage::error::StorageResult;
use crate::storage::fs::{ensure_layout, read_json, write_json};
use crate::storage::paths::StoragePaths;
use liz_protocol::{
    ArtifactId, InfoBoundary, MemoryCitationRef, MemoryFactId, MemoryFactKind, MemoryTopicStatus,
    RelationshipEntry, ThreadId, Timestamp,
};
use serde::{Deserialize, Serialize};

/// A stored memory fact kept in the global memory store.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredMemoryFact {
    /// The fact identifier.
    pub id: MemoryFactId,
    /// The stable classification of the fact.
    pub kind: MemoryFactKind,
    /// The stable subject of the fact.
    pub subject: String,
    /// The fact description or value.
    pub value: String,
    /// The keywords associated with the fact for recall.
    pub keywords: Vec<String>,
    /// Threads most closely associated with this fact.
    pub related_thread_ids: Vec<ThreadId>,
    /// Citation pointers backing the fact.
    pub citations: Vec<MemoryCitationRef>,
    /// The timestamp when the fact was updated.
    pub updated_at: Timestamp,
    /// The timestamp when the fact was invalidated, if any.
    pub invalidated_at: Option<Timestamp>,
    /// The fact that superseded this one, if any.
    pub invalidated_by: Option<MemoryFactId>,
}

/// A topic record persisted in the topic index.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredTopicRecord {
    /// The primary topic name.
    pub name: String,
    /// Alternate names or aliases that can recall the topic.
    pub aliases: Vec<String>,
    /// A one-line summary for the topic.
    pub summary: String,
    /// The current lifecycle state of the topic.
    pub status: MemoryTopicStatus,
    /// The last time the topic was touched.
    pub last_active_at: Timestamp,
    /// Threads related to the topic.
    pub related_thread_ids: Vec<ThreadId>,
    /// Artifacts related to the topic.
    pub related_artifact_ids: Vec<ArtifactId>,
    /// Facts related to the topic.
    pub citation_fact_ids: Vec<MemoryFactId>,
    /// Recent keywords used to recall the topic.
    pub recent_keywords: Vec<String>,
    /// Evidence pointers that back the topic.
    pub citations: Vec<MemoryCitationRef>,
}

/// The persisted shape used by the global memory store.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GlobalMemorySnapshot {
    /// The current identity summary, if known.
    pub identity_summary: Option<String>,
    /// The current active world-model summary, if known.
    pub active_state_summary: Option<String>,
    /// The thread identifiers currently considered active.
    pub active_thread_ids: Vec<ThreadId>,
    /// Recent topics written by foreground compilation.
    pub recent_topics: Vec<String>,
    /// Recent keywords written by foreground compilation.
    pub recent_keywords: Vec<String>,
    /// The stored memory facts.
    pub facts: Vec<StoredMemoryFact>,
    /// The minimal topic index used by foreground recall.
    pub topic_index: Vec<StoredTopicRecord>,
    /// Owner-defined relationship entries used for context boundaries.
    #[serde(default)]
    pub relationships: Vec<RelationshipEntry>,
    /// The default boundary applied to unknown participants.
    #[serde(default)]
    pub default_stranger_boundary: InfoBoundary,
}

impl GlobalMemorySnapshot {
    /// Returns the default empty snapshot used on first boot.
    pub fn empty() -> Self {
        Self {
            identity_summary: None,
            active_state_summary: None,
            active_thread_ids: Vec::new(),
            recent_topics: Vec::new(),
            recent_keywords: Vec::new(),
            facts: Vec::new(),
            topic_index: Vec::new(),
            relationships: Vec::new(),
            default_stranger_boundary: InfoBoundary::stranger_default(),
        }
    }
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
        Ok(read_json(&self.paths.global_memory_file())?.unwrap_or_else(GlobalMemorySnapshot::empty))
    }

    fn write_snapshot(&self, snapshot: &GlobalMemorySnapshot) -> StorageResult<()> {
        ensure_layout(&self.paths)?;
        write_json(&self.paths.global_memory_file(), snapshot)
    }
}
