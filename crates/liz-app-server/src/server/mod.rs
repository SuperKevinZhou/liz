//! Transport-facing server façade.

use crate::handlers;
use crate::runtime::RuntimeCoordinator;
use crate::storage::StoragePaths;
use liz_protocol::{ClientRequestEnvelope, ServerResponseEnvelope};

/// Minimal server configuration used by the app server.
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

/// High-level server façade used by tests and future transports.
#[derive(Debug)]
pub struct AppServer {
    runtime: RuntimeCoordinator,
}

impl AppServer {
    /// Creates a new app server rooted at the provided storage paths.
    pub fn new(paths: StoragePaths) -> Self {
        Self { runtime: RuntimeCoordinator::new(crate::runtime::RuntimeStores::new(paths)) }
    }

    /// Creates an app server using the default `.liz` storage layout.
    pub fn from_default_layout() -> Self {
        Self { runtime: RuntimeCoordinator::default() }
    }

    /// Handles a single protocol request and returns the matching response envelope.
    pub fn handle_request(&mut self, envelope: ClientRequestEnvelope) -> ServerResponseEnvelope {
        handlers::handle_request(&mut self.runtime, envelope)
    }

    /// Returns a shared reference to the runtime coordinator for direct inspection in tests.
    pub fn runtime(&self) -> &RuntimeCoordinator {
        &self.runtime
    }
}
