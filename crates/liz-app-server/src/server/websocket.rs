//! WebSocket transports for tests and the reference client.

use crate::server::AppServer;
use liz_protocol::{
    ClientRequestEnvelope, ClientTransportMessage, ServerEvent, ServerResponseEnvelope,
    ServerTransportMessage,
};
use std::error::Error;
use std::fmt;
use std::io::ErrorKind;
use std::net::{SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::thread::JoinHandle;
use std::time::Duration;
use tungstenite::{accept, Message, WebSocket};

const READ_POLL_INTERVAL: Duration = Duration::from_millis(50);

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
        self.request_tx.send(request).map_err(|_| WebSocketTransportError::Disconnected)
    }

    /// Blocks until the next response is available.
    pub fn recv_response(&self) -> Result<ServerResponseEnvelope, WebSocketTransportError> {
        self.response_rx.recv().map_err(|_| WebSocketTransportError::Disconnected)
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

/// Handle to a background WebSocket server bound to a local TCP address.
#[derive(Debug)]
pub struct WebSocketServerHandle {
    local_addr: SocketAddr,
    shutdown_tx: Sender<()>,
    join_handle: Option<JoinHandle<()>>,
}

impl WebSocketServerHandle {
    /// Returns the socket address the server is listening on.
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    /// Returns the websocket URL that clients should connect to.
    pub fn ws_url(&self) -> String {
        format!("ws://{}", self.local_addr)
    }

    /// Stops the server and waits for the accept loop to finish.
    pub fn shutdown(mut self) {
        let _ = self.shutdown_tx.send(());
        if let Some(join_handle) = self.join_handle.take() {
            let _ = join_handle.join();
        }
    }
}

impl Drop for WebSocketServerHandle {
    fn drop(&mut self) {
        let _ = self.shutdown_tx.send(());
        if let Some(join_handle) = self.join_handle.take() {
            let _ = join_handle.join();
        }
    }
}

/// Errors emitted by websocket transports.
#[derive(Debug)]
pub enum WebSocketTransportError {
    /// The server side is no longer available.
    Disconnected,
    /// No event or frame was emitted within the requested timeout.
    TimedOut,
    /// The transport hit an I/O failure.
    Io(std::io::Error),
    /// The websocket protocol failed.
    Protocol(String),
    /// The JSON payload was malformed.
    Json(serde_json::Error),
}

impl fmt::Display for WebSocketTransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Disconnected => f.write_str("websocket transport disconnected"),
            Self::TimedOut => f.write_str("websocket transport timed out waiting for a frame"),
            Self::Io(error) => write!(f, "websocket transport I/O error: {error}"),
            Self::Protocol(message) => write!(f, "websocket transport protocol error: {message}"),
            Self::Json(error) => write!(f, "websocket transport json error: {error}"),
        }
    }
}

impl Error for WebSocketTransportError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Json(error) => Some(error),
            Self::Disconnected | Self::TimedOut | Self::Protocol(_) => None,
        }
    }
}

impl From<serde_json::Error> for WebSocketTransportError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

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

/// Spawns a real TCP-backed websocket server that exposes the app server over one bind address.
pub fn spawn_websocket_server<A>(
    server: AppServer,
    bind_addr: A,
) -> Result<WebSocketServerHandle, WebSocketTransportError>
where
    A: ToSocketAddrs,
{
    let listener = TcpListener::bind(bind_addr).map_err(WebSocketTransportError::Io)?;
    listener.set_nonblocking(true).map_err(WebSocketTransportError::Io)?;
    let local_addr = listener.local_addr().map_err(WebSocketTransportError::Io)?;
    let shared_server = Arc::new(Mutex::new(server));
    let (shutdown_tx, shutdown_rx) = mpsc::channel();

    let join_handle = thread::spawn(move || accept_loop(listener, shared_server, shutdown_rx));

    Ok(WebSocketServerHandle { local_addr, shutdown_tx, join_handle: Some(join_handle) })
}

fn accept_loop(listener: TcpListener, server: Arc<Mutex<AppServer>>, shutdown_rx: Receiver<()>) {
    loop {
        if shutdown_rx.try_recv().is_ok() {
            break;
        }

        match listener.accept() {
            Ok((stream, _)) => {
                let server = Arc::clone(&server);
                thread::spawn(move || {
                    let _ = serve_connection(server, stream);
                });
            }
            Err(error) if is_transient_accept_error(error.kind()) => {
                thread::sleep(READ_POLL_INTERVAL);
            }
            Err(_) => break,
        }
    }
}

fn is_transient_accept_error(kind: ErrorKind) -> bool {
    matches!(
        kind,
        ErrorKind::WouldBlock
            | ErrorKind::TimedOut
            | ErrorKind::Interrupted
            | ErrorKind::ConnectionAborted
            | ErrorKind::ConnectionReset
    )
}

fn serve_connection(
    server: Arc<Mutex<AppServer>>,
    stream: TcpStream,
) -> Result<(), WebSocketTransportError> {
    let mut websocket = accept(stream).map_err(map_handshake_error)?;
    websocket
        .get_mut()
        .set_read_timeout(Some(READ_POLL_INTERVAL))
        .map_err(WebSocketTransportError::Io)?;
    let event_rx = {
        let server = server.lock().expect("websocket server mutex should not be poisoned");
        server.subscribe_events()
    };
    let (outbound_tx, outbound_rx) = mpsc::channel();
    let is_running = Arc::new(AtomicBool::new(true));
    let event_running = Arc::clone(&is_running);
    let event_forwarder = thread::spawn(move || {
        while event_running.load(Ordering::Relaxed) {
            match event_rx.recv_timeout(READ_POLL_INTERVAL) {
                Ok(event) => {
                    if outbound_tx.send(ServerTransportMessage::event(event)).is_err() {
                        break;
                    }
                }
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => break,
            }
        }
    });

    let result = connection_loop(&server, &mut websocket, outbound_rx);
    is_running.store(false, Ordering::Relaxed);
    let _ = event_forwarder.join();
    result
}

fn connection_loop(
    server: &Arc<Mutex<AppServer>>,
    websocket: &mut WebSocket<TcpStream>,
    outbound_rx: Receiver<ServerTransportMessage>,
) -> Result<(), WebSocketTransportError> {
    loop {
        flush_outbound_messages(websocket, &outbound_rx)?;

        match websocket.read() {
            Ok(message) => {
                if !handle_incoming_message(server, websocket, message)? {
                    return Ok(());
                }
            }
            Err(error) => match map_tungstenite_error(error) {
                WebSocketTransportError::TimedOut => continue,
                WebSocketTransportError::Disconnected => return Ok(()),
                other => return Err(other),
            },
        }
    }
}

fn flush_outbound_messages(
    websocket: &mut WebSocket<TcpStream>,
    outbound_rx: &Receiver<ServerTransportMessage>,
) -> Result<(), WebSocketTransportError> {
    while let Ok(message) = outbound_rx.try_recv() {
        send_server_message(websocket, &message)?;
    }
    Ok(())
}

fn handle_incoming_message(
    server: &Arc<Mutex<AppServer>>,
    websocket: &mut WebSocket<TcpStream>,
    message: Message,
) -> Result<bool, WebSocketTransportError> {
    match message {
        Message::Text(text) => {
            let request = serde_json::from_str::<ClientTransportMessage>(&text)?.into_request();
            let response = {
                let mut server =
                    server.lock().expect("websocket server mutex should not be poisoned");
                server.handle_request(request)
            };
            send_server_message(websocket, &ServerTransportMessage::response(response))?;
            Ok(true)
        }
        Message::Binary(_) => Ok(true),
        Message::Ping(payload) => {
            websocket.send(Message::Pong(payload)).map_err(map_tungstenite_error)?;
            Ok(true)
        }
        Message::Pong(_) => Ok(true),
        Message::Close(_) => {
            let _ = websocket.close(None);
            Ok(false)
        }
        Message::Frame(_) => Ok(true),
    }
}

fn send_server_message(
    websocket: &mut WebSocket<TcpStream>,
    message: &ServerTransportMessage,
) -> Result<(), WebSocketTransportError> {
    let payload = serde_json::to_string(message)?;
    websocket.send(Message::Text(payload.into())).map_err(map_tungstenite_error)
}

fn map_tungstenite_error(error: tungstenite::Error) -> WebSocketTransportError {
    match error {
        tungstenite::Error::ConnectionClosed
        | tungstenite::Error::AlreadyClosed
        | tungstenite::Error::Protocol(
            tungstenite::error::ProtocolError::ResetWithoutClosingHandshake,
        ) => WebSocketTransportError::Disconnected,
        tungstenite::Error::Io(error)
            if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) =>
        {
            WebSocketTransportError::TimedOut
        }
        tungstenite::Error::Io(error) => WebSocketTransportError::Io(error),
        other => WebSocketTransportError::Protocol(other.to_string()),
    }
}

fn map_handshake_error(
    error: tungstenite::HandshakeError<
        tungstenite::handshake::server::ServerHandshake<
            TcpStream,
            tungstenite::handshake::server::NoCallback,
        >,
    >,
) -> WebSocketTransportError {
    match error {
        tungstenite::HandshakeError::Failure(error) => map_tungstenite_error(error),
        tungstenite::HandshakeError::Interrupted(_) => WebSocketTransportError::Protocol(
            "websocket server handshake was interrupted".to_owned(),
        ),
    }
}
