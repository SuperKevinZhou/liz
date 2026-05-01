//! Thread resources and state enums.

use crate::ids::{CheckpointId, ThreadId, TurnId, WorkspaceMountId};
use crate::primitives::Timestamp;
use serde::{Deserialize, Serialize};

/// Describes the lifecycle state of a thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThreadStatus {
    /// The thread is active and available for new turns.
    Active,
    /// The thread is blocked on an approval decision.
    WaitingApproval,
    /// The thread was interrupted and can later be resumed.
    Interrupted,
    /// The thread finished successfully.
    Completed,
    /// The thread reached a terminal failure state.
    Failed,
    /// The thread is archived and hidden from active work.
    Archived,
}

/// The authoritative summary of a work thread.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Thread {
    /// The unique thread identifier.
    pub id: ThreadId,
    /// The user-visible thread title.
    pub title: String,
    /// The current lifecycle state of the thread.
    pub status: ThreadStatus,
    /// The time when the thread was created.
    pub created_at: Timestamp,
    /// The time when the thread was last updated.
    pub updated_at: Timestamp,
    /// The currently active goal for the thread.
    pub active_goal: Option<String>,
    /// The minimal summary needed to resume the thread.
    pub active_summary: Option<String>,
    /// The most recent interruption marker for the thread.
    pub last_interruption: Option<String>,
    /// The optional workspace attached to this thread.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_ref: Option<String>,
    /// The workspace mount attached to this thread.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_mount_id: Option<WorkspaceMountId>,
    /// Commitments that are still pending on the thread.
    pub pending_commitments: Vec<String>,
    /// The latest turn known for the thread.
    pub latest_turn_id: Option<TurnId>,
    /// The latest checkpoint associated with the thread.
    pub latest_checkpoint_id: Option<CheckpointId>,
    /// The parent thread when this thread was forked from another line of work.
    pub parent_thread_id: Option<ThreadId>,
}
