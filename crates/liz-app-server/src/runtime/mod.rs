//! Runtime coordination skeleton for the app server.

/// Minimal runtime metadata captured by the Phase 0 skeleton.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeSkeleton {
    /// The intended lifecycle model for the runtime.
    pub mode: &'static str,
}

impl Default for RuntimeSkeleton {
    fn default() -> Self {
        Self { mode: "thread-turn-runtime" }
    }
}
