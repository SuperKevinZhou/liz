//! Checkpoint resources and related enums.

use crate::ids::{CheckpointId, ThreadId, TurnId};
use crate::primitives::Timestamp;
use serde::{Deserialize, Serialize};

/// Describes what a checkpoint can restore.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckpointScope {
    /// The checkpoint restores only conversation state.
    ConversationOnly,
    /// The checkpoint restores only workspace state.
    WorkspaceOnly,
    /// The checkpoint restores both conversation and workspace state.
    ConversationAndWorkspace,
}

/// The authoritative representation of a recovery checkpoint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Checkpoint {
    /// The checkpoint identifier.
    pub id: CheckpointId,
    /// The thread associated with the checkpoint.
    pub thread_id: ThreadId,
    /// The turn that created the checkpoint.
    pub turn_id: TurnId,
    /// The restore scope covered by the checkpoint.
    pub scope: CheckpointScope,
    /// The user-visible reason the checkpoint exists.
    pub reason: String,
    /// The time when the checkpoint was created.
    pub created_at: Timestamp,
}
