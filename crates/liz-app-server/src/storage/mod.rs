//! Storage interfaces and filesystem layout primitives for the app server.

pub mod artifact_store;
pub mod auth_profile_store;
pub mod checkpoint_store;
pub mod error;
mod fs;
pub mod global_memory_store;
pub mod paths;
pub mod thread_store;
pub mod turn_log;

pub use artifact_store::{ArtifactStore, FsArtifactStore, StoredArtifact};
pub use auth_profile_store::{AuthProfileSnapshot, AuthProfileStore, FsAuthProfileStore};
pub use checkpoint_store::{CheckpointStore, FsCheckpointStore};
pub use error::{StorageError, StorageResult};
pub use global_memory_store::{
    FsGlobalMemoryStore, GlobalMemorySnapshot, GlobalMemoryStore, StoredMemoryFact,
    StoredTopicRecord,
};
pub use paths::StoragePaths;
pub use thread_store::{FsThreadStore, ThreadStore};
pub use turn_log::{FsTurnLog, TurnLog, TurnLogEntry};

/// Minimal storage layout metadata for the Phase 0 and Phase 2 skeletons.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageLayout {
    /// The default root directory name for on-disk state.
    pub root_dir: &'static str,
}

impl Default for StorageLayout {
    fn default() -> Self {
        Self { root_dir: ".liz" }
    }
}
