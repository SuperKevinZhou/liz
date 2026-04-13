//! Storage layout placeholders for the app server.

/// Minimal storage layout metadata for the Phase 0 skeleton.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageLayout {
    /// The default root directory name for on-disk state.
    pub root_dir: &'static str,
}

impl Default for StorageLayout {
    fn default() -> Self {
        Self { root_dir: ".liz" }
    }
}
