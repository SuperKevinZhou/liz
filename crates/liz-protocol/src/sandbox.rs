//! Sandbox policy and execution-mode types shared across clients and servers.

use serde::{Deserialize, Serialize};

/// The file-system sandbox mode requested for tool execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SandboxMode {
    /// Allow reads but deny file writes.
    ReadOnly,
    /// Allow writes inside the working area while keeping broader file-system restrictions.
    WorkspaceWrite,
    /// Disable sandbox enforcement and run directly on the host.
    DangerFullAccess,
    /// Treat the current process as already sandboxed by an external runtime.
    ExternalSandbox,
}

impl SandboxMode {
    /// Returns the stable wire name for the sandbox mode.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::ReadOnly => "read-only",
            Self::WorkspaceWrite => "workspace-write",
            Self::DangerFullAccess => "danger-full-access",
            Self::ExternalSandbox => "external-sandbox",
        }
    }
}

/// The network posture associated with sandboxed tool execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SandboxNetworkAccess {
    /// Deny network access.
    Disabled,
    /// Allow only the narrow network surface managed by the runtime.
    Restricted,
    /// Allow unrestricted network access.
    Enabled,
}

impl SandboxNetworkAccess {
    /// Returns the stable wire name for the network posture.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::Restricted => "restricted",
            Self::Enabled => "enabled",
        }
    }
}

/// A per-request sandbox override for shell tools.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShellSandboxRequest {
    /// The sandbox mode to use for this shell command.
    pub mode: SandboxMode,
    /// The requested network posture for this command.
    pub network_access: SandboxNetworkAccess,
}
