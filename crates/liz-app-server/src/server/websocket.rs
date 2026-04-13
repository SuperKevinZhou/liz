//! Loopback websocket-style transport for tests and the reference client.

use crate::server::AppServer;
use liz_protocol::{ClientRequestEnvelope, ServerEvent, ServerResponseEnvelope};
use std::error::Error;
use std::fmt;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::Duration;

/// A loopback websocket client that shares a duplex request/event channel with an app server.
#[derive(Debug)]
pub struct LoopbackWebSocketClient {
    request_tx: Sender<ClientRequestEnvelope>,
    response_rx: Receiver<ServerResponseEnvelope>,
    event_rx: Receiver<ServerEvent>,
}

impl LoopbackWebSocketClient {
    /// Sends a request over the loopback websocket transport.
    pub fn send_request(
        &self,
        request: ClientRequestEnvelope,
    ) -> Result<(), WebSocketTransportError> {
        self.request_tx
            .send(request)
            .map_err(|_| WebSocketTransportError::Disconnected)
    }

    /// Blocks until the next response is available.
    pub fn recv_response(&self) -> Result<ServerResponseEnvelope, WebSocketTransportError> {
        self.response_rx
            .recv()
            .map_err(|_| WebSocketTransportError::Disconnected)
    }

    /// Waits for the next server event for up to the provided duration.
    pub fn recv_event_timeout(
        &self,
        timeout: Duration,
    ) -> Result<ServerEvent, WebSocketTransportError> {
        self.event_rx.recv_timeout(timeout).map_err(|error| match error {
            RecvTimeoutError::Timeout => WebSocketTransportError::TimedOut,
            RecvTimeoutError::Disconnected => WebSocketTransportError::Disconnected,
        })
    }
}

/// Errors emitted by the loopback websocket transport.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebSocketTransportError {
    /// The server side is no longer available.
    Disconnected,
    /// No event was emitted within the requested timeout.
    TimedOut,
}

impl fmt::Display for WebSocketTransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Disconnected => f.write_str("loopback websocket disconnected"),
            Self::TimedOut => f.write_str("loopback websocket timed out waiting for an event"),
        }
    }
}

impl Error for WebSocketTransportError {}

/// Spawns a background loop that handles requests over a loopback websocket-style channel.
pub fn spawn_loopback_websocket(server: AppServer) -> LoopbackWebSocketClient {
    let (request_tx, request_rx) = mpsc::channel();
    let (response_tx, response_rx) = mpsc::channel();
    let event_rx = server.subscribe_events();

    thread::spawn(move || {
        let mut server = server;
        while let Ok(request) = request_rx.recv() {
            let response = server.handle_request(request);
            if response_tx.send(response).is_err() {
                break;
            }
        }
    });

    LoopbackWebSocketClient { request_tx, response_rx, event_rx }
}
