//! Runtime-facing storage bundle helpers.

use crate::storage::{
    ArtifactStore, CheckpointStore, FsArtifactStore, FsCheckpointStore, FsGlobalMemoryStore,
    FsThreadStore, FsTurnLog, GlobalMemorySnapshot, GlobalMemoryStore, StoragePaths,
    StorageResult, StoredArtifact, ThreadStore, TurnLog, TurnLogEntry,
};
use liz_protocol::{ArtifactId, Checkpoint, CheckpointId, Thread, ThreadId};

/// Bundles the concrete filesystem-backed stores used by the runtime.
#[derive(Debug, Clone)]
pub struct RuntimeStores {
    thread_store: FsThreadStore,
    turn_log: FsTurnLog,
    checkpoint_store: FsCheckpointStore,
    global_memory_store: FsGlobalMemoryStore,
    artifact_store: FsArtifactStore,
}

impl RuntimeStores {
    /// Creates a new runtime store bundle rooted at the provided storage paths.
    pub fn new(paths: StoragePaths) -> Self {
        Self {
            thread_store: FsThreadStore::new(paths.clone()),
            turn_log: FsTurnLog::new(paths.clone()),
            checkpoint_store: FsCheckpointStore::new(paths.clone()),
            global_memory_store: FsGlobalMemoryStore::new(paths.clone()),
            artifact_store: FsArtifactStore::new(paths),
        }
    }

    /// Creates the runtime store bundle using the default `.liz` layout.
    pub fn from_default_layout() -> Self {
        Self::new(StoragePaths::from_default_layout())
    }

    /// Persists a thread projection.
    pub fn put_thread(&self, thread: &Thread) -> StorageResult<()> {
        self.thread_store.put_thread(thread)
    }

    /// Reads a thread projection.
    pub fn get_thread(&self, thread_id: &ThreadId) -> StorageResult<Option<Thread>> {
        self.thread_store.get_thread(thread_id)
    }

    /// Appends a turn-log entry.
    pub fn append_turn_log(&self, entry: &TurnLogEntry) -> StorageResult<()> {
        self.turn_log.append_entry(entry)
    }

    /// Reads every turn-log entry for a thread.
    pub fn read_turn_log(&self, thread_id: &ThreadId) -> StorageResult<Vec<TurnLogEntry>> {
        self.turn_log.read_entries(thread_id)
    }

    /// Persists a checkpoint.
    pub fn put_checkpoint(&self, checkpoint: &Checkpoint) -> StorageResult<()> {
        self.checkpoint_store.put_checkpoint(checkpoint)
    }

    /// Reads a checkpoint when it exists.
    pub fn get_checkpoint(&self, checkpoint_id: &CheckpointId) -> StorageResult<Option<Checkpoint>> {
        self.checkpoint_store.get_checkpoint(checkpoint_id)
    }

    /// Reads the global memory snapshot.
    pub fn read_global_memory(&self) -> StorageResult<GlobalMemorySnapshot> {
        self.global_memory_store.read_snapshot()
    }

    /// Persists the global memory snapshot.
    pub fn write_global_memory(&self, snapshot: &GlobalMemorySnapshot) -> StorageResult<()> {
        self.global_memory_store.write_snapshot(snapshot)
    }

    /// Persists an artifact payload.
    pub fn put_artifact(&self, artifact: &StoredArtifact) -> StorageResult<()> {
        self.artifact_store.put_artifact(artifact)
    }

    /// Reads an artifact when it exists.
    pub fn get_artifact(&self, artifact_id: &ArtifactId) -> StorageResult<Option<StoredArtifact>> {
        self.artifact_store.get_artifact(artifact_id)
    }
}
