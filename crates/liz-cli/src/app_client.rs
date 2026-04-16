//! Client-side websocket transport helpers.

use liz_protocol::{ClientRequestEnvelope, ServerEvent, ServerResponseEnvelope};
use std::error::Error;
use std::fmt;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::time::Duration;

/// A thin websocket client used by the reference CLI.
#[derive(Debug)]
pub struct WebSocketAppClient {
    request_tx: Sender<ClientRequestEnvelope>,
    response_rx: Receiver<ServerResponseEnvelope>,
    event_rx: Receiver<ServerEvent>,
}

impl WebSocketAppClient {
    /// Creates a websocket client from request, response, and event channels.
    pub fn new(
        request_tx: Sender<ClientRequestEnvelope>,
        response_rx: Receiver<ServerResponseEnvelope>,
        event_rx: Receiver<ServerEvent>,
    ) -> Self {
        Self { request_tx, response_rx, event_rx }
    }

    /// Returns the transport name used by the CLI banner.
    pub fn transport_name() -> &'static str {
        "websocket"
    }

    /// Sends a typed request to the server.
    pub fn send_request(&self, request: ClientRequestEnvelope) -> Result<(), AppClientError> {
        self.request_tx.send(request).map_err(|_| AppClientError::Disconnected)
    }

    /// Receives the next response from the server.
    pub fn recv_response(&self) -> Result<ServerResponseEnvelope, AppClientError> {
        self.response_rx.recv().map_err(|_| AppClientError::Disconnected)
    }

    /// Receives the next event from the server, waiting up to the provided timeout.
    pub fn recv_event_timeout(&self, timeout: Duration) -> Result<ServerEvent, AppClientError> {
        self.event_rx.recv_timeout(timeout).map_err(|error| match error {
            RecvTimeoutError::Timeout => AppClientError::TimedOut,
            RecvTimeoutError::Disconnected => AppClientError::Disconnected,
        })
    }
}

/// Errors emitted by the CLI app client.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppClientError {
    /// The underlying websocket transport was closed.
    Disconnected,
    /// No event arrived in the requested interval.
    TimedOut,
}

impl fmt::Display for AppClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Disconnected => f.write_str("websocket app client disconnected"),
            Self::TimedOut => f.write_str("websocket app client timed out"),
        }
    }
}

impl Error for AppClientError {}
