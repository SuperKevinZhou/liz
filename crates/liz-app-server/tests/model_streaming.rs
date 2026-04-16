//! Model-streaming coverage for the normalized turn stream.

use liz_app_server::server::{spawn_loopback_websocket, AppServer};
use liz_app_server::storage::StoragePaths;
use liz_protocol::requests::{
    ClientRequest, ClientRequestEnvelope, ThreadStartRequest, TurnInputKind, TurnStartRequest,
};
use liz_protocol::{RequestId, ResponsePayload, ServerEventPayload, ServerResponseEnvelope};
use std::time::Duration;
use tempfile::TempDir;

#[test]
fn turn_start_streams_assistant_and_tool_events_before_completion() {
    let temp_dir = TempDir::new().expect("temp dir should be created");
    let server = AppServer::new(StoragePaths::new(temp_dir.path().join(".liz")));
    let client = spawn_loopback_websocket(server);

    client
        .send_request(envelope(
            "request_01",
            ClientRequest::ThreadStart(ThreadStartRequest {
                title: Some("Model stream".to_owned()),
                initial_goal: Some("Test normalized tool streaming".to_owned()),
                workspace_ref: None,
            }),
        ))
        .expect("thread request should be sent");
    let response = client.recv_response().expect("thread response should arrive");
    let thread = match response {
        ServerResponseEnvelope::Success(success) => match success.response {
            ResponsePayload::ThreadStart(response) => response.thread,
            other => panic!("unexpected response payload: {other:?}"),
        },
        other => panic!("unexpected response envelope: {other:?}"),
    };
    client.recv_event_timeout(Duration::from_secs(1)).expect("thread_started event should arrive");

    client
        .send_request(envelope(
            "request_02",
            ClientRequest::TurnStart(TurnStartRequest {
                thread_id: thread.id,
                input: "Plan a patch tool command for this task".to_owned(),
                input_kind: TurnInputKind::UserMessage,
            }),
        ))
        .expect("turn request should be sent");

    let first = client
        .recv_event_timeout(Duration::from_secs(1))
        .expect("turn_started event should arrive");
    let second = client
        .recv_event_timeout(Duration::from_secs(1))
        .expect("thread_updated event should arrive");
    let third =
        client.recv_event_timeout(Duration::from_secs(1)).expect("assistant chunk should arrive");
    let fourth =
        client.recv_event_timeout(Duration::from_secs(1)).expect("assistant chunk should arrive");
    let fifth =
        client.recv_event_timeout(Duration::from_secs(1)).expect("tool call started should arrive");
    let sixth =
        client.recv_event_timeout(Duration::from_secs(1)).expect("tool call delta should arrive");
    let seventh = client
        .recv_event_timeout(Duration::from_secs(1))
        .expect("tool call committed should arrive");
    let eighth = client
        .recv_event_timeout(Duration::from_secs(1))
        .expect("assistant completed should arrive");
    let ninth =
        client.recv_event_timeout(Duration::from_secs(1)).expect("turn completed should arrive");

    assert!(matches!(first.payload, ServerEventPayload::TurnStarted(_)));
    assert!(matches!(second.payload, ServerEventPayload::ThreadUpdated(_)));
    assert!(matches!(third.payload, ServerEventPayload::AssistantChunk(_)));
    assert!(matches!(fourth.payload, ServerEventPayload::AssistantChunk(_)));
    assert!(matches!(fifth.payload, ServerEventPayload::ToolCallStarted(_)));
    assert!(matches!(sixth.payload, ServerEventPayload::ToolCallUpdated(_)));
    assert!(matches!(seventh.payload, ServerEventPayload::ToolCallCommitted(_)));
    assert!(matches!(eighth.payload, ServerEventPayload::AssistantCompleted(_)));
    assert!(matches!(ninth.payload, ServerEventPayload::TurnCompleted(_)));

    let response = client.recv_response().expect("turn response should eventually arrive");
    assert!(matches!(response, ServerResponseEnvelope::Success(_)));
}

#[test]
fn turn_start_executes_committed_shell_tool_calls() {
    let temp_dir = TempDir::new().expect("temp dir should be created");
    let server = AppServer::new(StoragePaths::new(temp_dir.path().join(".liz")));
    let client = spawn_loopback_websocket(server);

    client
        .send_request(envelope(
            "request_11",
            ClientRequest::ThreadStart(ThreadStartRequest {
                title: Some("Model exec".to_owned()),
                initial_goal: Some("Test committed tool execution".to_owned()),
                workspace_ref: None,
            }),
        ))
        .expect("thread request should be sent");
    let response = client.recv_response().expect("thread response should arrive");
    let thread = match response {
        ServerResponseEnvelope::Success(success) => match success.response {
            ResponsePayload::ThreadStart(response) => response.thread,
            other => panic!("unexpected response payload: {other:?}"),
        },
        other => panic!("unexpected response envelope: {other:?}"),
    };
    client.recv_event_timeout(Duration::from_secs(1)).expect("thread_started event should arrive");

    client
        .send_request(envelope(
            "request_12",
            ClientRequest::TurnStart(TurnStartRequest {
                thread_id: thread.id,
                input: "run command: echo from-turn".to_owned(),
                input_kind: TurnInputKind::UserMessage,
            }),
        ))
        .expect("turn request should be sent");

    let mut saw_tool_committed = false;
    let mut saw_tool_completed = false;
    let mut saw_executor_output = false;
    let mut saw_artifact = false;
    let mut saw_turn_completed = false;
    let mut seen_events = Vec::new();
    let mut committed_summary = None;

    for _ in 0..16 {
        let Ok(event) = client.recv_event_timeout(Duration::from_secs(5)) else {
            break;
        };
        seen_events.push(format!("{:?}", event.event_type()));
        match event.payload {
            ServerEventPayload::ToolCallCommitted(event) => {
                saw_tool_committed = true;
                committed_summary = Some((event.tool_name, event.arguments_summary));
            }
            ServerEventPayload::ToolCompleted(_) => saw_tool_completed = true,
            ServerEventPayload::ExecutorOutputChunk(chunk) => {
                saw_executor_output = saw_executor_output || chunk.chunk.contains("from-turn");
            }
            ServerEventPayload::ArtifactCreated(_) => saw_artifact = true,
            ServerEventPayload::TurnCompleted(_) => {
                saw_turn_completed = true;
                break;
            }
            _ => {}
        }
    }

    assert!(saw_tool_committed, "seen events: {seen_events:?}");
    assert!(saw_tool_completed, "seen events: {seen_events:?}, committed: {committed_summary:?}");
    assert!(saw_executor_output, "seen events: {seen_events:?}");
    assert!(saw_artifact, "seen events: {seen_events:?}");
    assert!(saw_turn_completed, "seen events: {seen_events:?}");

    let response = client.recv_response().expect("turn response should eventually arrive");
    assert!(matches!(response, ServerResponseEnvelope::Success(_)));
}

fn envelope(request_id: &str, request: ClientRequest) -> ClientRequestEnvelope {
    ClientRequestEnvelope { request_id: RequestId::new(request_id), request }
}
