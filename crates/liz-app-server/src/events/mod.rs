//! Event streaming skeletons for the app server.

/// Minimal event-stream metadata for Phase 0 wiring.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventStreamSkeleton {
    /// The transport that Phase 0 assumes for the future event stream.
    pub transport: &'static str,
}

impl Default for EventStreamSkeleton {
    fn default() -> Self {
        Self { transport: "websocket" }
    }
}
