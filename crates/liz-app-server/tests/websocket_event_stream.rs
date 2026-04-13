//! Event-stream coverage for the loopback websocket transport.

use liz_app_server::server::{spawn_loopback_websocket, AppServer};
use liz_app_server::storage::StoragePaths;
use liz_protocol::requests::{
    ClientRequest, ClientRequestEnvelope, ThreadStartRequest, TurnCancelRequest, TurnInputKind,
    TurnStartRequest,
};
use liz_protocol::{RequestId, ServerEventPayload};
use std::time::Duration;
use tempfile::TempDir;

#[test]
fn loopback_websocket_streams_lifecycle_events_without_polling() {
    let temp_dir = TempDir::new().expect("temp dir should be created");
    let server = AppServer::new(StoragePaths::new(temp_dir.path().join(".liz")));
    let client = spawn_loopback_websocket(server);

    client
        .send_request(envelope(
            "request_01",
            ClientRequest::ThreadStart(ThreadStartRequest {
                title: Some("Thread over websocket".to_owned()),
                initial_goal: Some("Emit lifecycle events".to_owned()),
                workspace_ref: None,
            }),
        ))
        .expect("request should be sent");
    let response = client.recv_response().expect("response should arrive");
    let thread = match response {
        liz_protocol::ServerResponseEnvelope::Success(success) => match success.response {
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

    client
        .send_request(envelope(
            "request_02",
            ClientRequest::TurnStart(TurnStartRequest {
                thread_id: thread.id.clone(),
                input: "Start the long-running work".to_owned(),
                input_kind: TurnInputKind::UserMessage,
            }),
        ))
        .expect("turn request should be sent");
    let response = client.recv_response().expect("turn response should arrive");
    let turn = match response {
        liz_protocol::ServerResponseEnvelope::Success(success) => match success.response {
            liz_protocol::ResponsePayload::TurnStart(response) => response.turn,
            other => panic!("unexpected response payload: {other:?}"),
        },
        other => panic!("unexpected response envelope: {other:?}"),
    };
    let first_turn_event = client
        .recv_event_timeout(Duration::from_secs(1))
        .expect("turn_started event should arrive");
    let second_turn_event = client
        .recv_event_timeout(Duration::from_secs(1))
        .expect("thread_updated event should arrive");
    assert!(matches!(first_turn_event.payload, ServerEventPayload::TurnStarted(_)));
    assert!(matches!(second_turn_event.payload, ServerEventPayload::ThreadUpdated(_)));

    client
        .send_request(envelope(
            "request_03",
            ClientRequest::TurnCancel(TurnCancelRequest {
                thread_id: thread.id.clone(),
                turn_id: turn.id.clone(),
            }),
        ))
        .expect("cancel request should be sent");
    client.recv_response().expect("cancel response should arrive");
    let cancelled = client
        .recv_event_timeout(Duration::from_secs(1))
        .expect("turn_cancelled event should arrive");
    let interrupted = client
        .recv_event_timeout(Duration::from_secs(1))
        .expect("thread_interrupted event should arrive");
    assert!(matches!(cancelled.payload, ServerEventPayload::TurnCancelled(_)));
    assert!(matches!(interrupted.payload, ServerEventPayload::ThreadInterrupted(_)));
}

fn envelope(request_id: &str, request: ClientRequest) -> ClientRequestEnvelope {
    ClientRequestEnvelope {
        request_id: RequestId::new(request_id),
        request,
    }
}
