//! Content renderer placeholders for the CLI.

/// Minimal renderer metadata for the Phase 0 skeleton.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RendererSkeleton {
    /// The renderer stack reserved for future transcript and diff UIs.
    pub renderer_stack: &'static str,
}

impl Default for RendererSkeleton {
    fn default() -> Self {
        Self { renderer_stack: "transcript+approval+diff" }
    }
}
