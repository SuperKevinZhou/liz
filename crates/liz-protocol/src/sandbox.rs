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

/// The concrete platform backend that enforced sandbox policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SandboxBackendKind {
    /// No platform backend enforced the command.
    None,
    /// macOS Seatbelt enforcement.
    MacosSeatbelt,
    /// Linux helper-backed sandboxing.
    LinuxHelper,
    /// Windows restricted-token enforcement.
    WindowsRestrictedToken,
    /// Windows sandbox-user enforcement.
    WindowsSandboxUser,
}

impl SandboxBackendKind {
    /// Returns the stable wire name for the backend.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::MacosSeatbelt => "macos-seatbelt",
            Self::LinuxHelper => "linux-helper",
            Self::WindowsRestrictedToken => "windows-restricted-token",
            Self::WindowsSandboxUser => "windows-sandbox-user",
        }
    }
}

/// The effective sandbox settings used for a shell tool execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShellSandboxSummary {
    /// The effective sandbox mode.
    pub mode: SandboxMode,
    /// The effective network posture.
    pub network_access: SandboxNetworkAccess,
    /// The concrete backend selected for the command.
    pub backend: SandboxBackendKind,
}

/// A per-request sandbox override for shell tools.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShellSandboxRequest {
    /// The sandbox mode to use for this shell command.
    pub mode: SandboxMode,
    /// The requested network posture for this command.
    pub network_access: SandboxNetworkAccess,
}
