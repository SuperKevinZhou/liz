//! Transport-level envelopes shared by protocol clients and servers.

use crate::events::ServerEvent;
use crate::requests::ClientRequestEnvelope;
use crate::responses::ServerResponseEnvelope;
use serde::{Deserialize, Serialize};

/// A transport-level message sent from a client to the app server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "payload", rename_all = "snake_case")]
pub enum ClientTransportMessage {
    /// Wraps one protocol request envelope.
    Request(ClientRequestEnvelope),
}

impl ClientTransportMessage {
    /// Wraps a request envelope into a transport message.
    pub fn request(envelope: ClientRequestEnvelope) -> Self {
        Self::Request(envelope)
    }

    /// Returns the wrapped request envelope when this frame contains one.
    pub fn into_request(self) -> ClientRequestEnvelope {
        match self {
            Self::Request(envelope) => envelope,
        }
    }
}

/// A transport-level message sent from the app server to a client.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "payload", rename_all = "snake_case")]
pub enum ServerTransportMessage {
    /// Wraps one request response envelope.
    Response(ServerResponseEnvelope),
    /// Wraps one server event.
    Event(ServerEvent),
}

impl ServerTransportMessage {
    /// Wraps a response envelope into a transport message.
    pub fn response(envelope: ServerResponseEnvelope) -> Self {
        Self::Response(envelope)
    }

    /// Wraps a server event into a transport message.
    pub fn event(event: ServerEvent) -> Self {
        Self::Event(event)
    }
}
