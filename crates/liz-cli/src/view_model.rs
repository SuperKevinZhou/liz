//! View-model placeholders for the CLI.

/// Minimal CLI view-model metadata for the Phase 0 skeleton.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewModelSkeleton {
    /// The default surface the future reference client will render.
    pub primary_view: &'static str,
}

impl Default for ViewModelSkeleton {
    fn default() -> Self {
        Self { primary_view: "transcript" }
    }
}
