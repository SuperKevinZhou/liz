//! Real websocket coverage for the CLI app client.

use liz_app_server::server::{spawn_websocket_server, AppServer};
use liz_app_server::storage::StoragePaths;
use liz_cli::app_client::WebSocketAppClient;
use liz_protocol::requests::{
    ClientRequest, ClientRequestEnvelope, ThreadStartRequest, TurnInputKind, TurnStartRequest,
};
use liz_protocol::{RequestId, ResponsePayload, ServerEventPayload, ServerResponseEnvelope};
use std::time::Duration;
use tempfile::TempDir;

#[test]
fn websocket_app_client_exchanges_requests_and_events_with_real_server() {
    let temp_dir = TempDir::new().expect("temp dir should be created");
    let server = AppServer::new_simulated(StoragePaths::new(temp_dir.path().join(".liz")));
    let handle =
        spawn_websocket_server(server, "127.0.0.1:0").expect("websocket server should bind");
    let client = WebSocketAppClient::connect(&handle.ws_url()).expect("cli client should connect");

    client
        .send_request(envelope(
            "request_01",
            ClientRequest::ThreadStart(ThreadStartRequest {
                title: Some("CLI websocket".to_owned()),
                initial_goal: Some("Talk to the app server".to_owned()),
                workspace_ref: None,
            }),
        ))
        .expect("thread request should be sent");
    let thread = match client.recv_response().expect("thread response should arrive") {
        ServerResponseEnvelope::Success(success) => match success.response {
            ResponsePayload::ThreadStart(response) => response.thread,
            other => panic!("unexpected response payload: {other:?}"),
        },
        other => panic!("unexpected response envelope: {other:?}"),
    };
    let event = client
        .recv_event_timeout(Duration::from_secs(1))
        .expect("thread_started event should arrive");
    assert!(matches!(event.payload, ServerEventPayload::ThreadStarted(_)));

    client
        .send_request(envelope(
            "request_02",
            ClientRequest::TurnStart(TurnStartRequest {
                thread_id: thread.id,
                input: "Plan a patch tool command for this task".to_owned(),
                input_kind: TurnInputKind::UserMessage,
                channel: None,
                participant: None,
            }),
        ))
        .expect("turn request should be sent");
    let response = client.recv_response().expect("turn response should arrive");
    assert!(matches!(response, ServerResponseEnvelope::Success(_)));
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
