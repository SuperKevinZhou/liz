//! Request handler registry placeholders for the app server.

/// Describes which handler groups exist in the Phase 0 skeleton.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandlerRegistry {
    /// Thread lifecycle handlers are present.
    pub threads: bool,
    /// Turn lifecycle handlers are present.
    pub turns: bool,
    /// Memory maintenance handlers are present.
    pub memory: bool,
}

impl Default for HandlerRegistry {
    fn default() -> Self {
        Self { threads: true, turns: true, memory: true }
    }
}
