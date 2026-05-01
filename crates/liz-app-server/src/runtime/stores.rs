//! Runtime-facing storage bundle helpers.

use crate::storage::FsNodeRegistryStore;
use crate::storage::{
    ArtifactStore, AuthProfileSnapshot, AuthProfileStore, CheckpointStore, FsArtifactStore,
    FsAuthProfileStore, FsCheckpointStore, FsGlobalMemoryStore, FsThreadStore, FsTurnLog,
    GlobalMemorySnapshot, GlobalMemoryStore, NodeRegistrySnapshot, NodeRegistryStore, StoragePaths,
    StorageResult, StoredArtifact, ThreadStore, TurnLog, TurnLogEntry,
};
use liz_protocol::{ArtifactId, Checkpoint, CheckpointId, Thread, ThreadId};

/// Bundles the concrete filesystem-backed stores used by the runtime.
#[derive(Debug, Clone)]
pub struct RuntimeStores {
    paths: StoragePaths,
    thread_store: FsThreadStore,
    turn_log: FsTurnLog,
    checkpoint_store: FsCheckpointStore,
    global_memory_store: FsGlobalMemoryStore,
    artifact_store: FsArtifactStore,
    auth_profile_store: FsAuthProfileStore,
    node_registry_store: FsNodeRegistryStore,
}

impl RuntimeStores {
    /// Creates a new runtime store bundle rooted at the provided storage paths.
    pub fn new(paths: StoragePaths) -> Self {
        Self {
            paths: paths.clone(),
            thread_store: FsThreadStore::new(paths.clone()),
            turn_log: FsTurnLog::new(paths.clone()),
            checkpoint_store: FsCheckpointStore::new(paths.clone()),
            global_memory_store: FsGlobalMemoryStore::new(paths.clone()),
            artifact_store: FsArtifactStore::new(paths.clone()),
            auth_profile_store: FsAuthProfileStore::new(paths.clone()),
            node_registry_store: FsNodeRegistryStore::new(paths),
        }
    }

    /// Creates the runtime store bundle using the default `.liz` layout.
    pub fn from_default_layout() -> Self {
        Self::new(StoragePaths::from_default_layout())
    }

    /// Returns the storage root for artifact locator generation.
    pub fn paths(&self) -> &StoragePaths {
        &self.paths
    }

    /// Persists a thread projection.
    pub fn put_thread(&self, thread: &Thread) -> StorageResult<()> {
        self.thread_store.put_thread(thread)
    }

    /// Reads a thread projection.
    pub fn get_thread(&self, thread_id: &ThreadId) -> StorageResult<Option<Thread>> {
        self.thread_store.get_thread(thread_id)
    }

    /// Lists all persisted thread projections.
    pub fn list_threads(&self) -> StorageResult<Vec<Thread>> {
        self.thread_store.list_threads()
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
    pub fn get_checkpoint(
        &self,
        checkpoint_id: &CheckpointId,
    ) -> StorageResult<Option<Checkpoint>> {
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

    /// Reads the persisted auth profile snapshot.
    pub fn read_auth_profiles(&self) -> StorageResult<AuthProfileSnapshot> {
        self.auth_profile_store.read_snapshot()
    }

    /// Writes the persisted auth profile snapshot.
    pub fn write_auth_profiles(&self, snapshot: &AuthProfileSnapshot) -> StorageResult<()> {
        self.auth_profile_store.write_snapshot(snapshot)
    }

    /// Reads the persisted node registry snapshot.
    pub fn read_node_registry(&self) -> StorageResult<NodeRegistrySnapshot> {
        self.node_registry_store.read_snapshot()
    }

    /// Writes the persisted node registry snapshot.
    pub fn write_node_registry(&self, snapshot: &NodeRegistrySnapshot) -> StorageResult<()> {
        self.node_registry_store.write_snapshot(snapshot)
    }
}
