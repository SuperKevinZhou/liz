//! Artifact store interfaces and persisted data shapes.

use crate::storage::error::StorageResult;
use crate::storage::fs::{ensure_layout, read_json, write_json};
use crate::storage::paths::StoragePaths;
use liz_protocol::{ArtifactId, ArtifactRef};
use serde::{Deserialize, Serialize};

/// The persisted artifact payload stored behind an artifact reference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredArtifact {
    /// The reference metadata for the artifact.
    pub reference: ArtifactRef,
    /// The serialized artifact payload.
    pub body: String,
}

/// Minimal interface for artifact persistence.
pub trait ArtifactStore {
    /// Persists an artifact payload.
    fn put_artifact(&self, artifact: &StoredArtifact) -> StorageResult<()>;

    /// Reads a previously persisted artifact payload.
    fn get_artifact(&self, artifact_id: &ArtifactId) -> StorageResult<Option<StoredArtifact>>;
}

/// Filesystem-backed artifact store.
#[derive(Debug, Clone)]
pub struct FsArtifactStore {
    paths: StoragePaths,
}

impl FsArtifactStore {
    /// Creates a filesystem-backed artifact store.
    pub fn new(paths: StoragePaths) -> Self {
        Self { paths }
    }
}

impl ArtifactStore for FsArtifactStore {
    fn put_artifact(&self, artifact: &StoredArtifact) -> StorageResult<()> {
        ensure_layout(&self.paths)?;
        write_json(&self.paths.artifact_file(&artifact.reference.id), artifact)
    }

    fn get_artifact(&self, artifact_id: &ArtifactId) -> StorageResult<Option<StoredArtifact>> {
        ensure_layout(&self.paths)?;
        read_json(&self.paths.artifact_file(artifact_id))
    }
}
