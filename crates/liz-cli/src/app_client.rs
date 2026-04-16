//! Client-side websocket transport helpers.

use liz_protocol::{
    ClientRequestEnvelope, ClientTransportMessage, ServerEvent, ServerResponseEnvelope,
    ServerTransportMessage,
};
use std::error::Error;
use std::fmt;
use std::io::ErrorKind;
use std::net::{SocketAddr, TcpStream};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::thread;
use std::time::Duration;
use tungstenite::{client, Message, WebSocket};

const READ_POLL_INTERVAL: Duration = Duration::from_millis(50);

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

    /// Connects the reference client to a real websocket app-server endpoint.
    pub fn connect(url: &str) -> Result<Self, AppClientError> {
        let socket_addr = parse_socket_addr(url)?;
        let stream = TcpStream::connect(socket_addr).map_err(AppClientError::Io)?;
        let (socket, _) = client(url, stream).map_err(map_handshake_error)?;
        let (request_tx, request_rx) = mpsc::channel();
        let (response_tx, response_rx) = mpsc::channel();
        let (event_tx, event_rx) = mpsc::channel();

        thread::spawn(move || {
            let _ = run_socket_loop(socket, request_rx, response_tx, event_tx);
        });

        Ok(Self { request_tx, response_rx, event_rx })
    }

    /// Returns the transport name used by the CLI banner.
    pub fn transport_name() -> &'static str {
        "websocket"
    }

    /// Sends a typed request to the server.
    pub fn send_request(
        &self,
        request: ClientRequestEnvelope,
    ) -> Result<(), AppClientError> {
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
#[derive(Debug)]
pub enum AppClientError {
    /// The underlying websocket transport was closed.
    Disconnected,
    /// No event arrived in the requested interval.
    TimedOut,
    /// The client hit an I/O error while opening the connection.
    Io(std::io::Error),
    /// The websocket handshake or frame protocol failed.
    Protocol(String),
    /// The websocket payload could not be decoded.
    Json(serde_json::Error),
}

impl fmt::Display for AppClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Disconnected => f.write_str("websocket app client disconnected"),
            Self::TimedOut => f.write_str("websocket app client timed out"),
            Self::Io(error) => write!(f, "websocket app client I/O error: {error}"),
            Self::Protocol(message) => write!(f, "websocket app client protocol error: {message}"),
            Self::Json(error) => write!(f, "websocket app client json error: {error}"),
        }
    }
}

impl Error for AppClientError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Json(error) => Some(error),
            Self::Disconnected | Self::TimedOut | Self::Protocol(_) => None,
        }
    }
}

fn run_socket_loop(
    mut socket: WebSocket<TcpStream>,
    request_rx: Receiver<ClientRequestEnvelope>,
    response_tx: Sender<ServerResponseEnvelope>,
    event_tx: Sender<ServerEvent>,
) -> Result<(), AppClientError> {
    socket
        .get_mut()
        .set_read_timeout(Some(READ_POLL_INTERVAL))
        .map_err(AppClientError::Io)?;

    loop {
        flush_requests(&mut socket, &request_rx)?;

        match socket.read() {
            Ok(Message::Text(text)) => {
                let message =
                    serde_json::from_str::<ServerTransportMessage>(&text).map_err(AppClientError::Json)?;
                match message {
                    ServerTransportMessage::Response(response) => {
                        if response_tx.send(response).is_err() {
                            return Err(AppClientError::Disconnected);
                        }
                    }
                    ServerTransportMessage::Event(event) => {
                        if event_tx.send(event).is_err() {
                            return Err(AppClientError::Disconnected);
                        }
                    }
                }
            }
            Ok(Message::Binary(_) | Message::Ping(_) | Message::Pong(_) | Message::Frame(_)) => {
                continue;
            }
            Ok(Message::Close(_)) => return Err(AppClientError::Disconnected),
            Err(error) => match map_tungstenite_error(error) {
                AppClientError::TimedOut => continue,
                AppClientError::Disconnected => return Err(AppClientError::Disconnected),
                other => return Err(other),
            },
        }
    }
}

fn flush_requests(
    socket: &mut WebSocket<TcpStream>,
    request_rx: &Receiver<ClientRequestEnvelope>,
) -> Result<(), AppClientError> {
    loop {
        let request = match request_rx.try_recv() {
            Ok(request) => request,
            Err(TryRecvError::Empty) => return Ok(()),
            Err(TryRecvError::Disconnected) => return Err(AppClientError::Disconnected),
        };
        let payload = serde_json::to_string(&ClientTransportMessage::request(request))
            .map_err(AppClientError::Json)?;
        socket
            .send(Message::Text(payload.into()))
            .map_err(map_tungstenite_error)?;
    }
}

fn parse_socket_addr(url: &str) -> Result<SocketAddr, AppClientError> {
    url.trim_start_matches("ws://")
        .parse::<SocketAddr>()
        .map_err(|error| AppClientError::Protocol(format!("invalid websocket url {url}: {error}")))
}

fn map_tungstenite_error(error: tungstenite::Error) -> AppClientError {
    match error {
        tungstenite::Error::ConnectionClosed
        | tungstenite::Error::AlreadyClosed
        | tungstenite::Error::Protocol(tungstenite::error::ProtocolError::ResetWithoutClosingHandshake) => {
            AppClientError::Disconnected
        }
        tungstenite::Error::Io(error)
            if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) =>
        {
            AppClientError::TimedOut
        }
        tungstenite::Error::Io(error) => AppClientError::Io(error),
        other => AppClientError::Protocol(other.to_string()),
    }
}

fn map_handshake_error(
    error: tungstenite::HandshakeError<tungstenite::handshake::client::ClientHandshake<TcpStream>>,
) -> AppClientError {
    match error {
        tungstenite::HandshakeError::Failure(error) => map_tungstenite_error(error),
        tungstenite::HandshakeError::Interrupted(_) => {
            AppClientError::Protocol("websocket client handshake was interrupted".to_owned())
        }
    }
}
