//! Turn resources and turn-related enums.

use crate::ids::{CheckpointId, ThreadId, TurnId};
use crate::primitives::Timestamp;
use serde::{Deserialize, Serialize};

/// Describes the lifecycle state of a turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnStatus {
    /// The turn is currently running.
    Running,
    /// The turn is blocked on an approval decision.
    WaitingApproval,
    /// The turn was cancelled before completion.
    Cancelled,
    /// The turn completed successfully.
    Completed,
    /// The turn failed.
    Failed,
}

/// Describes what kind of turn is being recorded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnKind {
    /// A user-initiated turn.
    User,
    /// An assistant-authored continuation turn.
    Assistant,
    /// A verification turn used to inspect or validate prior work.
    Verification,
    /// A compilation turn used to summarize or persist boundary state.
    Compilation,
    /// A rollback turn used to restore prior state.
    Rollback,
}

/// The authoritative summary of a turn within a thread.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Turn {
    /// The unique turn identifier.
    pub id: TurnId,
    /// The parent thread identifier.
    pub thread_id: ThreadId,
    /// The role of the turn within the thread lifecycle.
    pub kind: TurnKind,
    /// The current lifecycle state of the turn.
    pub status: TurnStatus,
    /// The time when the turn started.
    pub started_at: Timestamp,
    /// The time when the turn ended, if it has already finished.
    pub ended_at: Option<Timestamp>,
    /// The goal the turn is currently pursuing.
    pub goal: Option<String>,
    /// A minimal turn summary.
    pub summary: Option<String>,
    /// The checkpoint active before the turn started.
    pub checkpoint_before: Option<CheckpointId>,
    /// The checkpoint created after the turn completed.
    pub checkpoint_after: Option<CheckpointId>,
}
