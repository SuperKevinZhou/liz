//! Node registry storage interfaces.

use crate::storage::fs::{ensure_layout, read_json, write_json};
use crate::storage::{StoragePaths, StorageResult};
use liz_protocol::{NodeRecord, WorkspaceMount};
use serde::{Deserialize, Serialize};

/// Persisted node and workspace-mount state.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeRegistrySnapshot {
    /// Registered runtime nodes.
    pub nodes: Vec<NodeRecord>,
    /// Workspace mounts owned by nodes.
    pub workspace_mounts: Vec<WorkspaceMount>,
}

/// Storage interface for node registry state.
pub trait NodeRegistryStore {
    /// Reads the full node registry snapshot.
    fn read_snapshot(&self) -> StorageResult<NodeRegistrySnapshot>;
    /// Writes the full node registry snapshot.
    fn write_snapshot(&self, snapshot: &NodeRegistrySnapshot) -> StorageResult<()>;
}

/// Filesystem-backed node registry store.
#[derive(Debug, Clone)]
pub struct FsNodeRegistryStore {
    paths: StoragePaths,
}

impl FsNodeRegistryStore {
    /// Creates a new node registry store.
    pub fn new(paths: StoragePaths) -> Self {
        Self { paths }
    }
}

impl NodeRegistryStore for FsNodeRegistryStore {
    fn read_snapshot(&self) -> StorageResult<NodeRegistrySnapshot> {
        ensure_layout(&self.paths)?;
        Ok(read_json(&self.paths.node_registry_file())?.unwrap_or_default())
    }

    fn write_snapshot(&self, snapshot: &NodeRegistrySnapshot) -> StorageResult<()> {
        ensure_layout(&self.paths)?;
        write_json(&self.paths.node_registry_file(), snapshot)
    }
}
