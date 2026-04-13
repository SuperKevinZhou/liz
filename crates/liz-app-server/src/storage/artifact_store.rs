//! Artifact store interfaces and persisted data shapes.

use crate::storage::error::StorageResult;
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
