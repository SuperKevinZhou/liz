//! Event-stream coverage for the real websocket transport.

use liz_app_server::server::{spawn_websocket_server, AppServer};
use liz_app_server::storage::StoragePaths;
use liz_protocol::requests::{
    ClientRequest, ClientRequestEnvelope, ThreadStartRequest, TurnInputKind, TurnStartRequest,
};
use liz_protocol::{
    ClientTransportMessage, RequestId, ServerEventPayload, ServerResponseEnvelope,
    ServerTransportMessage,
};
use std::net::TcpStream;
use std::time::{Duration, Instant};
use tempfile::TempDir;
use tungstenite::{client, Message, WebSocket};

#[test]
fn websocket_server_streams_lifecycle_events_without_polling() {
    let temp_dir = TempDir::new().expect("temp dir should be created");
    let server = AppServer::new_simulated(StoragePaths::new(temp_dir.path().join(".liz")));
    let handle =
        spawn_websocket_server(server, "127.0.0.1:0").expect("websocket server should bind");
    let mut client =
        TestWebSocketClient::connect(&handle.ws_url()).expect("websocket client should connect");

    let response = client
        .send_request(envelope(
            "request_01",
            ClientRequest::ThreadStart(ThreadStartRequest {
                title: Some("Thread over websocket".to_owned()),
                initial_goal: Some("Emit lifecycle events".to_owned()),
                workspace_ref: None,
            }),
        ))
        .expect("request should succeed");
    let thread = match response {
        ServerResponseEnvelope::Success(success) => match success.response {
            liz_protocol::ResponsePayload::ThreadStart(response) => response.thread,
            other => panic!("unexpected response payload: {other:?}"),
        },
        other => panic!("unexpected response envelope: {other:?}"),
    };
    let event = client
        .recv_event_timeout(Duration::from_secs(1))
        .expect("thread_started event should arrive");
    match event.payload {
        ServerEventPayload::ThreadStarted(payload) => assert_eq!(payload.thread.id, thread.id),
        other => panic!("unexpected event payload: {other:?}"),
    }

    let _response = client
        .send_request(envelope(
            "request_02",
            ClientRequest::TurnStart(TurnStartRequest {
                thread_id: thread.id.clone(),
                input: "Start the long-running work".to_owned(),
                input_kind: TurnInputKind::UserMessage,
            }),
        ))
        .expect("turn request should succeed");
    let first_turn_event = client
        .recv_event_timeout(Duration::from_secs(1))
        .expect("turn_started event should arrive");
    let second_turn_event = client
        .recv_event_timeout(Duration::from_secs(1))
        .expect("thread_updated event should arrive");
    let third_turn_event =
        client.recv_event_timeout(Duration::from_secs(1)).expect("assistant event should arrive");
    assert!(matches!(first_turn_event.payload, ServerEventPayload::TurnStarted(_)));
    assert!(matches!(second_turn_event.payload, ServerEventPayload::ThreadUpdated(_)));
    assert!(matches!(
        third_turn_event.payload,
        ServerEventPayload::AssistantChunk(_) | ServerEventPayload::AssistantCompleted(_)
    ));

    handle.shutdown();
}

fn envelope(request_id: &str, request: ClientRequest) -> ClientRequestEnvelope {
    ClientRequestEnvelope { request_id: RequestId::new(request_id), request }
}

struct TestWebSocketClient {
    socket: WebSocket<TcpStream>,
}

impl TestWebSocketClient {
    fn connect(url: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let address = url
            .trim_start_matches("ws://")
            .parse::<std::net::SocketAddr>()
            .expect("websocket test url should be a socket address");
        let deadline = Instant::now() + Duration::from_secs(2);

        loop {
            let attempt_error = match TcpStream::connect(address) {
                Ok(stream) => match client(url, stream) {
                    Ok((socket, _)) => return Ok(Self { socket }),
                    Err(error) => error.to_string(),
                },
                Err(error) => error.to_string(),
            };

            if Instant::now() >= deadline {
                return Err(format!(
                    "websocket client failed to connect to {url}: {}",
                    attempt_error
                )
                .into());
            }
            std::thread::sleep(Duration::from_millis(25));
        }
    }

    fn send_request(
        &mut self,
        request: ClientRequestEnvelope,
    ) -> Result<ServerResponseEnvelope, Box<dyn std::error::Error>> {
        let frame = serde_json::to_string(&ClientTransportMessage::request(request))?;
        self.socket.send(Message::Text(frame.into()))?;

        loop {
            match self.recv_transport_message(Duration::from_secs(1))? {
                ServerTransportMessage::Response(response) => return Ok(response),
                ServerTransportMessage::Event(_) => continue,
            }
        }
    }

    fn recv_event_timeout(
        &mut self,
        timeout: Duration,
    ) -> Result<liz_protocol::ServerEvent, Box<dyn std::error::Error>> {
        loop {
            match self.recv_transport_message(timeout)? {
                ServerTransportMessage::Event(event) => return Ok(event),
                ServerTransportMessage::Response(_) => continue,
            }
        }
    }

    fn recv_transport_message(
        &mut self,
        timeout: Duration,
    ) -> Result<ServerTransportMessage, Box<dyn std::error::Error>> {
        self.socket.get_mut().set_read_timeout(Some(timeout))?;

        loop {
            match self.socket.read()? {
                Message::Text(text) => return Ok(serde_json::from_str(&text)?),
                Message::Binary(_) | Message::Ping(_) | Message::Pong(_) | Message::Frame(_) => {
                    continue
                }
                Message::Close(frame) => {
                    return Err(format!("websocket closed unexpectedly: {frame:?}").into())
                }
            }
        }
    }
}
