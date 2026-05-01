//! Path conventions for on-disk runtime state.

use crate::storage::StorageLayout;
use liz_protocol::{ArtifactId, CheckpointId, ThreadId};
use std::path::{Path, PathBuf};

/// Resolves the canonical storage paths used by the app server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoragePaths {
    root: PathBuf,
}

impl StoragePaths {
    /// Creates a new storage-path resolver rooted at the provided workspace path.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Creates a storage-path resolver rooted at the default `.liz` directory.
    pub fn from_default_layout() -> Self {
        Self::new(StorageLayout::default().root_dir)
    }

    /// Returns the storage root directory.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Returns the directory used to store thread records.
    pub fn threads_dir(&self) -> PathBuf {
        self.root.join("threads")
    }

    /// Returns the directory used to store append-only turn logs.
    pub fn turn_logs_dir(&self) -> PathBuf {
        self.root.join("turn_logs")
    }

    /// Returns the directory used to store artifact payloads.
    pub fn artifacts_dir(&self) -> PathBuf {
        self.root.join("artifacts")
    }

    /// Returns the directory used to store checkpoint payloads.
    pub fn checkpoints_dir(&self) -> PathBuf {
        self.root.join("checkpoints")
    }

    /// Returns the file used to persist global memory state.
    pub fn global_memory_file(&self) -> PathBuf {
        self.root.join("global_memory.json")
    }

    /// Returns the file used to persist provider auth profiles.
    pub fn auth_profiles_file(&self) -> PathBuf {
        self.root.join("auth_profiles.json")
    }

    /// Returns the file used to persist node and workspace-mount state.
    pub fn node_registry_file(&self) -> PathBuf {
        self.root.join("node_registry.json")
    }

    /// Returns the JSON file for a thread record.
    pub fn thread_file(&self, thread_id: &ThreadId) -> PathBuf {
        self.threads_dir().join(format!("{thread_id}.json"))
    }

    /// Returns the JSONL file for a thread turn log.
    pub fn turn_log_file(&self, thread_id: &ThreadId) -> PathBuf {
        self.turn_logs_dir().join(format!("{thread_id}.jsonl"))
    }

    /// Returns the JSON file for a stored artifact payload.
    pub fn artifact_file(&self, artifact_id: &ArtifactId) -> PathBuf {
        self.artifacts_dir().join(format!("{artifact_id}.json"))
    }

    /// Returns the JSON file for a stored checkpoint.
    pub fn checkpoint_file(&self, checkpoint_id: &CheckpointId) -> PathBuf {
        self.checkpoints_dir().join(format!("{checkpoint_id}.json"))
    }

    /// Returns every directory that should exist for the storage layout.
    pub fn required_directories(&self) -> Vec<PathBuf> {
        vec![
            self.root.clone(),
            self.threads_dir(),
            self.turn_logs_dir(),
            self.artifacts_dir(),
            self.checkpoints_dir(),
        ]
    }
}
