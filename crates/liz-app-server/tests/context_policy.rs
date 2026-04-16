//! Context assembly and policy-engine coverage for Phase 6.

use liz_app_server::runtime::{RetrievalScope, RuntimeCoordinator};
use liz_app_server::server::{spawn_loopback_websocket, AppServer, WebSocketTransportError};
use liz_app_server::storage::StoragePaths;
use liz_protocol::requests::{
    ApprovalRespondRequest, ClientRequest, ClientRequestEnvelope, ThreadStartRequest,
    TurnInputKind, TurnStartRequest,
};
use liz_protocol::{ApprovalDecision, RequestId, ServerEventPayload};
use std::time::Duration;
use tempfile::TempDir;

#[test]
fn focused_change_requests_keep_context_narrow() {
    let temp_dir = TempDir::new().expect("temp dir should be created");
    let mut runtime =
        RuntimeCoordinator::from_storage_paths(StoragePaths::new(temp_dir.path().join(".liz")));
    let thread = runtime
        .start_thread(ThreadStartRequest {
            title: Some("Policy scope".to_owned()),
            initial_goal: Some("Stay narrow by default".to_owned()),
            workspace_ref: None,
        })
        .expect("thread start should succeed")
        .thread;

    let context = runtime
        .assemble_context(&thread.id, "Only change one line in src/lib.rs")
        .expect("context assembly should succeed");
    let decision = runtime.evaluate_policy("Only change one line in src/lib.rs", &context);

    assert_eq!(context.scope, RetrievalScope::Focused);
    assert!(!context.retrieval.requires_full_repo_scan);
    assert_eq!(decision.scope, RetrievalScope::Focused);
    assert!(!decision.requires_approval);
}

#[test]
fn risky_turn_stops_for_checkpoint_and_approval_then_resumes_after_approval() {
    let temp_dir = TempDir::new().expect("temp dir should be created");
    let server = AppServer::new(StoragePaths::new(temp_dir.path().join(".liz")));
    let client = spawn_loopback_websocket(server);

    client
        .send_request(envelope(
            "request_01",
            ClientRequest::ThreadStart(ThreadStartRequest {
                title: Some("Approval flow".to_owned()),
                initial_goal: Some("Guard risky turns".to_owned()),
                workspace_ref: None,
            }),
        ))
        .expect("thread request should be sent");
    let response = client.recv_response().expect("thread response should arrive");
    let thread = match response {
        liz_protocol::ServerResponseEnvelope::Success(success) => match success.response {
            liz_protocol::ResponsePayload::ThreadStart(response) => response.thread,
            other => panic!("unexpected response payload: {other:?}"),
        },
        other => panic!("unexpected response envelope: {other:?}"),
    };
    client.recv_event_timeout(Duration::from_secs(1)).expect("thread_started event should arrive");

    client
        .send_request(envelope(
            "request_02",
            ClientRequest::TurnStart(TurnStartRequest {
                thread_id: thread.id.clone(),
                input: "Delete .env and reset Cargo.lock".to_owned(),
                input_kind: TurnInputKind::UserMessage,
            }),
        ))
        .expect("turn request should be sent");
    let response = client.recv_response().expect("turn response should arrive");
    let approval = {
        let first = client
            .recv_event_timeout(Duration::from_secs(1))
            .expect("turn_started event should arrive");
        let second = client
            .recv_event_timeout(Duration::from_secs(1))
            .expect("thread_updated event should arrive");
        let third = client
            .recv_event_timeout(Duration::from_secs(1))
            .expect("checkpoint_created event should arrive");
        let fourth = client
            .recv_event_timeout(Duration::from_secs(1))
            .expect("approval_requested event should arrive");

        assert!(matches!(first.payload, ServerEventPayload::TurnStarted(_)));
        assert!(matches!(second.payload, ServerEventPayload::ThreadUpdated(_)));
        assert!(matches!(third.payload, ServerEventPayload::CheckpointCreated(_)));
        match fourth.payload {
            ServerEventPayload::ApprovalRequested(event) => event.approval,
            other => panic!("unexpected event payload: {other:?}"),
        }
    };
    assert!(matches!(response, liz_protocol::ServerResponseEnvelope::Success(_)));
    assert!(matches!(
        client.recv_event_timeout(Duration::from_millis(150)),
        Err(WebSocketTransportError::TimedOut)
    ));

    client
        .send_request(envelope(
            "request_03",
            ClientRequest::ApprovalRespond(ApprovalRespondRequest {
                approval_id: approval.id,
                decision: ApprovalDecision::ApproveOnce,
            }),
        ))
        .expect("approval response should be sent");
    let response = client.recv_response().expect("approval response should eventually arrive");
    assert!(matches!(response, liz_protocol::ServerResponseEnvelope::Success(_)));
    let resolved = client
        .recv_event_timeout(Duration::from_secs(1))
        .expect("approval_resolved event should arrive");
    let mut saw_assistant_event = false;
    let mut saw_turn_completed = false;
    for _ in 0..4 {
        let event = client
            .recv_event_timeout(Duration::from_secs(1))
            .expect("follow-up event should arrive");
        match event.payload {
            ServerEventPayload::AssistantChunk(_) | ServerEventPayload::AssistantCompleted(_) => {
                saw_assistant_event = true;
            }
            ServerEventPayload::TurnCompleted(_) => {
                saw_turn_completed = true;
                break;
            }
            other => panic!("unexpected event payload: {other:?}"),
        }
    }

    assert!(matches!(resolved.payload, ServerEventPayload::ApprovalResolved(_)));
    assert!(saw_assistant_event);
    assert!(saw_turn_completed);
}

fn envelope(request_id: &str, request: ClientRequest) -> ClientRequestEnvelope {
    ClientRequestEnvelope { request_id: RequestId::new(request_id), request }
}
