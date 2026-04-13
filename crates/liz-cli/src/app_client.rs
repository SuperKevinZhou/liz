//! Minimal client transport skeletons.

/// Minimal websocket client metadata for the Phase 0 skeleton.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppClientSkeleton {
    /// The transport that the CLI expects to use.
    pub transport: &'static str,
}

impl Default for AppClientSkeleton {
    fn default() -> Self {
        Self { transport: "websocket" }
    }
}
