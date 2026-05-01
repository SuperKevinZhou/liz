//! Approval resources and approval-related enums.

use crate::ids::{ApprovalId, NodeId, ThreadId, TurnId, WorkspaceMountId};
use crate::primitives::RiskLevel;
use serde::{Deserialize, Serialize};

/// Describes how an approval request was resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalDecision {
    /// Approves only the current action.
    ApproveOnce,
    /// Approves the current action and persists the resulting rule.
    ApproveAndPersist,
    /// Denies the action.
    Deny,
}

/// Describes how the runtime handles actions that would otherwise require approval.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ApprovalPolicy {
    /// Ask for explicit approval before high-risk actions continue.
    OnRequest,
    /// Continue high-risk actions without pausing for approval.
    DangerFullAccess,
}

impl ApprovalPolicy {
    /// Returns the stable wire name for this policy.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::OnRequest => "on-request",
            Self::DangerFullAccess => "danger-full-access",
        }
    }
}

/// Describes the lifecycle state of an approval request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalStatus {
    /// The request is still waiting for a decision.
    Pending,
    /// The request was approved.
    Approved,
    /// The request was denied.
    Denied,
    /// The request is no longer valid.
    Expired,
}

/// The authoritative representation of an approval request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalRequest {
    /// The approval request identifier.
    pub id: ApprovalId,
    /// The related thread identifier.
    pub thread_id: ThreadId,
    /// The related turn identifier.
    pub turn_id: TurnId,
    /// A coarse-grained action type such as shell execution or file mutation.
    pub action_type: String,
    /// The risk level that triggered the approval flow.
    pub risk_level: RiskLevel,
    /// The user-visible reason for the approval.
    pub reason: String,
    /// Optional sandbox context shown to the user.
    pub sandbox_context: Option<String>,
    /// The node where the action will run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_id: Option<NodeId>,
    /// The workspace mount affected by the action.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_mount_id: Option<WorkspaceMountId>,
    /// The current lifecycle state of the approval request.
    pub status: ApprovalStatus,
}
