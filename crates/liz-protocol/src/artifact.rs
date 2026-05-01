//! Artifact resources and related enums.

use crate::ids::{ArtifactId, NodeId, ThreadId, TurnId, WorkspaceMountId};
use crate::primitives::Timestamp;
use serde::{Deserialize, Serialize};

/// Describes the type of artifact emitted by the runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    /// A diff artifact describing file changes.
    Diff,
    /// A tool trace artifact describing a tool invocation.
    ToolTrace,
    /// A snapshot artifact that captures point-in-time state.
    Snapshot,
    /// A command output artifact.
    CommandOutput,
    /// An approval record artifact.
    ApprovalRecord,
    /// A memory citation artifact.
    MemoryCitation,
}

/// A lightweight reference to a stored artifact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactRef {
    /// The artifact identifier.
    pub id: ArtifactId,
    /// The thread associated with the artifact.
    pub thread_id: ThreadId,
    /// The turn associated with the artifact.
    pub turn_id: TurnId,
    /// The kind of artifact that was persisted.
    pub kind: ArtifactKind,
    /// The node that produced the artifact.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_id: Option<NodeId>,
    /// The workspace mount associated with the artifact.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_mount_id: Option<WorkspaceMountId>,
    /// A short user-visible summary.
    pub summary: String,
    /// The locator or path that can later retrieve the artifact.
    pub locator: String,
    /// The time when the artifact was created.
    pub created_at: Timestamp,
}
