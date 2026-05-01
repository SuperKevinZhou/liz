//! Node and workspace mount protocol resources.

use crate::approval::ApprovalPolicy;
use crate::ids::{NodeId, WorkspaceMountId};
use crate::primitives::Timestamp;
use crate::sandbox::SandboxMode;
use serde::{Deserialize, Serialize};

/// The class of runtime node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    /// A desktop machine.
    Desktop,
    /// A server or VPS.
    Server,
    /// A laptop machine.
    Laptop,
    /// A phone or mobile device.
    Phone,
    /// A containerized runtime.
    Container,
    /// A browser-hosted runtime.
    BrowserHost,
}

/// Stable identity for a runtime node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeIdentity {
    /// The node identifier.
    pub node_id: NodeId,
    /// Owner-facing display name.
    pub display_name: String,
    /// Node kind.
    pub kind: NodeKind,
    /// Whether this node is an owner-controlled device.
    pub owner_device: bool,
}

/// Current node liveness and version information.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeStatus {
    /// Whether the node is currently considered online.
    pub online: bool,
    /// Last heartbeat timestamp.
    pub last_seen_at: Option<Timestamp>,
    /// App or node runtime version.
    pub app_version: Option<String>,
    /// Operating system label.
    pub os: Option<String>,
    /// Hostname, if known.
    pub hostname: Option<String>,
}

/// Capabilities exposed by a node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeCapabilities {
    /// Workspace tools are available.
    pub workspace_tools: bool,
    /// Shell tools are available.
    pub shell_tools: bool,
    /// Browser automation tools are available.
    pub browser_tools: bool,
    /// The node can host a Web UI.
    pub web_ui_host: bool,
    /// The node can deliver notifications.
    pub notifications: bool,
    /// Maximum concurrent tasks.
    pub max_concurrent_tasks: u32,
    /// Sandbox modes supported by this node.
    pub supported_sandbox_modes: Vec<SandboxMode>,
}

/// Policy applied to actions running on a node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodePolicy {
    /// Roots that may be attached as workspaces.
    pub allowed_roots: Vec<String>,
    /// Paths requiring special care or confirmation.
    pub protected_paths: Vec<String>,
    /// Default filesystem sandbox.
    pub default_sandbox: SandboxMode,
    /// Network policy label.
    pub network_policy: String,
    /// Default approval policy.
    pub approval_policy: ApprovalPolicy,
}

/// A registered runtime node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeRecord {
    /// Stable node identity.
    pub identity: NodeIdentity,
    /// Liveness and version state.
    pub status: NodeStatus,
    /// Exposed capabilities.
    pub capabilities: NodeCapabilities,
    /// Node policy.
    pub policy: NodePolicy,
}

/// Permissions for a workspace mount.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceMountPermissions {
    /// Files may be read.
    pub read: bool,
    /// Files may be written.
    pub write: bool,
    /// Shell commands may run in this workspace.
    pub shell: bool,
}

/// A workspace path mounted on a specific node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceMount {
    /// The workspace mount identifier.
    pub workspace_id: WorkspaceMountId,
    /// The node that owns the path.
    pub node_id: NodeId,
    /// Root path on the node.
    pub root_path: String,
    /// Owner-facing label.
    pub label: String,
    /// Mount permissions.
    pub permissions: WorkspaceMountPermissions,
}
