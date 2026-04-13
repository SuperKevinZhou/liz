//! Transport-facing server façade.

mod websocket;

use crate::events::EventBus;
use crate::handlers;
use crate::runtime::RuntimeCoordinator;
use crate::storage::StoragePaths;
use liz_protocol::{ClientRequestEnvelope, ServerEvent, ServerResponseEnvelope};
use std::sync::mpsc::Receiver;

pub use websocket::{spawn_loopback_websocket, LoopbackWebSocketClient, WebSocketTransportError};

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
    event_bus: EventBus,
}

impl AppServer {
    /// Creates a new app server rooted at the provided storage paths.
    pub fn new(paths: StoragePaths) -> Self {
        Self {
            runtime: RuntimeCoordinator::new(crate::runtime::RuntimeStores::new(paths)),
            event_bus: EventBus::new(),
        }
    }

    /// Creates an app server using the default `.liz` storage layout.
    pub fn from_default_layout() -> Self {
        Self { runtime: RuntimeCoordinator::default(), event_bus: EventBus::new() }
    }

    /// Handles a single protocol request and returns the matching response envelope.
    pub fn handle_request(&mut self, envelope: ClientRequestEnvelope) -> ServerResponseEnvelope {
        let handled = handlers::handle_request(&mut self.runtime, envelope);
        self.event_bus.publish_all(handled.events);
        handled.response
    }

    /// Subscribes to the server event stream.
    pub fn subscribe_events(&self) -> Receiver<ServerEvent> {
        self.event_bus.subscribe()
    }

    /// Returns a shared reference to the runtime coordinator for direct inspection in tests.
    pub fn runtime(&self) -> &RuntimeCoordinator {
        &self.runtime
    }
}
