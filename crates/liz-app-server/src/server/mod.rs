//! Transport-facing server skeletons.

/// Minimal server configuration used by the Phase 0 workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerConfig {
    /// The bind address reserved for the future websocket server.
    pub bind_address: &'static str,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self { bind_address: "127.0.0.1:7777" }
    }
}
