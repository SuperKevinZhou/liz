//! Sandbox mode selection and platform backend planning for shell execution.

use crate::runtime::{RuntimeError, RuntimeResult};
use liz_protocol::{
    SandboxBackendKind, SandboxMode, SandboxNetworkAccess, ShellSandboxRequest, ShellSandboxSummary,
};
use std::env;

/// The concrete platform backend that will enforce sandbox restrictions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlatformSandboxBackend {
    /// No platform sandbox backend is active.
    None,
    /// macOS Seatbelt enforcement.
    MacosSeatbelt,
    /// Linux helper-backed sandbox enforcement.
    LinuxHelper,
    /// Windows restricted-token enforcement.
    WindowsRestrictedToken,
    /// Windows sandbox-user enforcement.
    WindowsSandboxUser,
}

impl PlatformSandboxBackend {
    /// Returns the stable metric tag used in traces and artifacts.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::MacosSeatbelt => "macos-seatbelt",
            Self::LinuxHelper => "linux-helper",
            Self::WindowsRestrictedToken => "windows-restricted-token",
            Self::WindowsSandboxUser => "windows-sandbox-user",
        }
    }

    /// Converts the backend into the protocol representation.
    pub const fn to_protocol(self) -> SandboxBackendKind {
        match self {
            Self::None => SandboxBackendKind::None,
            Self::MacosSeatbelt => SandboxBackendKind::MacosSeatbelt,
            Self::LinuxHelper => SandboxBackendKind::LinuxHelper,
            Self::WindowsRestrictedToken => SandboxBackendKind::WindowsRestrictedToken,
            Self::WindowsSandboxUser => SandboxBackendKind::WindowsSandboxUser,
        }
    }
}

/// The Windows backend variant to use when sandboxing is enabled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowsSandboxBackend {
    /// Use the host user's restricted token.
    RestrictedToken,
    /// Use a dedicated sandbox user.
    SandboxUser,
}

impl WindowsSandboxBackend {
    fn from_env() -> Self {
        match env::var("LIZ_WINDOWS_SANDBOX_BACKEND")
            .unwrap_or_else(|_| "sandbox-user".to_owned())
            .to_ascii_lowercase()
            .as_str()
        {
            "restricted-token" => Self::RestrictedToken,
            _ => Self::SandboxUser,
        }
    }
}

/// The Linux pipeline variant to prefer when sandboxing is enabled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinuxSandboxVariant {
    /// Use the default helper pipeline.
    Helper,
    /// Force the legacy Landlock fallback path.
    LegacyLandlock,
}

impl LinuxSandboxVariant {
    fn from_env() -> Self {
        match env::var("LIZ_LINUX_SANDBOX_VARIANT")
            .unwrap_or_else(|_| "helper".to_owned())
            .to_ascii_lowercase()
            .as_str()
        {
            "legacy-landlock" => Self::LegacyLandlock,
            _ => Self::Helper,
        }
    }
}

/// Runtime sandbox defaults for the local executor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SandboxConfig {
    /// The default sandbox mode when a request does not override it.
    pub default_mode: SandboxMode,
    /// The default network posture for sandboxed commands.
    pub default_network_access: SandboxNetworkAccess,
    /// The preferred Windows sandbox backend.
    pub windows_backend: WindowsSandboxBackend,
    /// The preferred Linux helper variant.
    pub linux_variant: LinuxSandboxVariant,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self::from_env()
    }
}

impl SandboxConfig {
    /// Loads sandbox defaults from environment variables.
    pub fn from_env() -> Self {
        Self {
            default_mode: parse_sandbox_mode("LIZ_SANDBOX_MODE").unwrap_or(SandboxMode::WorkspaceWrite),
            default_network_access: parse_network_access("LIZ_SANDBOX_NETWORK")
                .unwrap_or(SandboxNetworkAccess::Restricted),
            windows_backend: WindowsSandboxBackend::from_env(),
            linux_variant: LinuxSandboxVariant::from_env(),
        }
    }

    /// Resolves the effective sandbox request for one shell tool invocation.
    pub fn resolve_request(&self, request: Option<&ShellSandboxRequest>) -> EffectiveSandboxRequest {
        let override_request = request.cloned();
        let mode = override_request
            .as_ref()
            .map(|request| request.mode)
            .unwrap_or(self.default_mode);
        let network_access = override_request
            .as_ref()
            .map(|request| request.network_access)
            .unwrap_or(self.default_network_access);
        let backend = self.select_backend(mode);

        EffectiveSandboxRequest { mode, network_access, backend, request: override_request }
    }

    fn select_backend(&self, mode: SandboxMode) -> PlatformSandboxBackend {
        if matches!(mode, SandboxMode::DangerFullAccess | SandboxMode::ExternalSandbox) {
            return PlatformSandboxBackend::None;
        }

        if cfg!(target_os = "macos") {
            PlatformSandboxBackend::MacosSeatbelt
        } else if cfg!(target_os = "linux") {
            let _ = self.linux_variant;
            PlatformSandboxBackend::LinuxHelper
        } else if cfg!(target_os = "windows") {
            match self.windows_backend {
                WindowsSandboxBackend::RestrictedToken => PlatformSandboxBackend::WindowsRestrictedToken,
                WindowsSandboxBackend::SandboxUser => PlatformSandboxBackend::WindowsSandboxUser,
            }
        } else {
            PlatformSandboxBackend::None
        }
    }
}

/// The fully resolved sandbox settings for a shell command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectiveSandboxRequest {
    /// The effective sandbox mode.
    pub mode: SandboxMode,
    /// The effective network posture.
    pub network_access: SandboxNetworkAccess,
    /// The concrete backend selected for this host.
    pub backend: PlatformSandboxBackend,
    /// The original request override, when provided.
    pub request: Option<ShellSandboxRequest>,
}

impl EffectiveSandboxRequest {
    /// Validates that the resolved sandbox settings can be executed on the current platform.
    pub fn ensure_supported(&self) -> RuntimeResult<()> {
        match self.mode {
            SandboxMode::DangerFullAccess | SandboxMode::ExternalSandbox => Ok(()),
            SandboxMode::ReadOnly | SandboxMode::WorkspaceWrite => match self.backend {
                PlatformSandboxBackend::None => Err(RuntimeError::invalid_state(
                    "sandbox_backend_unavailable",
                    format!(
                        "sandbox mode {} is enabled but no platform backend is available",
                        self.mode.as_str()
                    ),
                )),
                _ => Ok(()),
            },
        }
    }

    /// Converts the effective settings into the protocol shape stored in tool results.
    pub fn to_summary(&self) -> ShellSandboxSummary {
        ShellSandboxSummary {
            mode: self.mode,
            network_access: self.network_access,
            backend: self.backend.to_protocol(),
        }
    }
}

fn parse_sandbox_mode(key: &str) -> Option<SandboxMode> {
    match env::var(key).ok()?.to_ascii_lowercase().as_str() {
        "read-only" => Some(SandboxMode::ReadOnly),
        "workspace-write" => Some(SandboxMode::WorkspaceWrite),
        "danger-full-access" => Some(SandboxMode::DangerFullAccess),
        "external-sandbox" => Some(SandboxMode::ExternalSandbox),
        _ => None,
    }
}

fn parse_network_access(key: &str) -> Option<SandboxNetworkAccess> {
    match env::var(key).ok()?.to_ascii_lowercase().as_str() {
        "disabled" => Some(SandboxNetworkAccess::Disabled),
        "restricted" => Some(SandboxNetworkAccess::Restricted),
        "enabled" => Some(SandboxNetworkAccess::Enabled),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{PlatformSandboxBackend, SandboxConfig, WindowsSandboxBackend};
    use liz_protocol::{SandboxMode, SandboxNetworkAccess, ShellSandboxRequest};

    #[test]
    fn request_override_wins_over_defaults() {
        let config = SandboxConfig {
            default_mode: SandboxMode::WorkspaceWrite,
            default_network_access: SandboxNetworkAccess::Restricted,
            windows_backend: WindowsSandboxBackend::RestrictedToken,
            linux_variant: super::LinuxSandboxVariant::Helper,
        };

        let effective = config.resolve_request(Some(&ShellSandboxRequest {
            mode: SandboxMode::DangerFullAccess,
            network_access: SandboxNetworkAccess::Enabled,
        }));

        assert_eq!(effective.mode, SandboxMode::DangerFullAccess);
        assert_eq!(effective.network_access, SandboxNetworkAccess::Enabled);
        assert_eq!(effective.backend, PlatformSandboxBackend::None);
    }
}
