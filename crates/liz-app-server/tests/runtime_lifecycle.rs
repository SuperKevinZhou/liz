//! Runtime lifecycle coverage for Phase 3.

use liz_app_server::server::AppServer;
use liz_app_server::storage::{StoragePaths, TurnLog};
use liz_app_server::storage::FsTurnLog;
use liz_protocol::requests::{
    ClientRequest, ClientRequestEnvelope, ThreadForkRequest, ThreadResumeRequest,
    ThreadStartRequest, TurnCancelRequest, TurnInputKind, TurnStartRequest,
};
use liz_protocol::responses::{ResponsePayload, ServerResponseEnvelope};
use liz_protocol::{RequestId, ThreadStatus, TurnStatus};
use tempfile::TempDir;

#[test]
fn thread_can_start_turn_interrupt_and_resume() {
    let temp_dir = TempDir::new().expect("temp dir should be created");
    let storage_paths = StoragePaths::new(temp_dir.path().join(".liz"));
    let turn_log = FsTurnLog::new(storage_paths.clone());
    let mut server = AppServer::new(storage_paths);

    let thread = match server.handle_request(envelope(
        "request_01",
        ClientRequest::ThreadStart(ThreadStartRequest {
            title: Some("Phase 3 runtime".to_owned()),
            initial_goal: Some("Stand up runtime lifecycle".to_owned()),
            workspace_ref: None,
        }),
    )) {
        ServerResponseEnvelope::Success(success) => match success.response {
            ResponsePayload::ThreadStart(response) => response.thread,
            other => panic!("unexpected response payload: {other:?}"),
        },
        other => panic!("unexpected response envelope: {other:?}"),
    };

    assert_eq!(thread.status, ThreadStatus::Active);
    assert_eq!(thread.active_goal.as_deref(), Some("Stand up runtime lifecycle"));

    let turn = match server.handle_request(envelope(
        "request_02",
        ClientRequest::TurnStart(TurnStartRequest {
            thread_id: thread.id.clone(),
            input: "Implement thread and turn managers".to_owned(),
            input_kind: TurnInputKind::UserMessage,
        }),
    )) {
        ServerResponseEnvelope::Success(success) => match success.response {
            ResponsePayload::TurnStart(response) => response.turn,
            other => panic!("unexpected response payload: {other:?}"),
        },
        other => panic!("unexpected response envelope: {other:?}"),
    };

    assert_eq!(turn.status, TurnStatus::Running);
    assert_eq!(
        server
            .runtime()
            .read_turn(&turn.id)
            .expect("turn should remain active")
            .summary
            .as_deref(),
        Some("Started turn for: Implement thread and turn managers")
    );

    let cancelled_turn = match server.handle_request(envelope(
        "request_03",
        ClientRequest::TurnCancel(TurnCancelRequest {
            thread_id: thread.id.clone(),
            turn_id: turn.id.clone(),
        }),
    )) {
        ServerResponseEnvelope::Success(success) => match success.response {
            ResponsePayload::TurnCancel(response) => response.turn,
            other => panic!("unexpected response payload: {other:?}"),
        },
        other => panic!("unexpected response envelope: {other:?}"),
    };

    assert_eq!(cancelled_turn.status, TurnStatus::Cancelled);
    assert!(server.runtime().read_turn(&cancelled_turn.id).is_none());

    let interrupted_thread = server
        .runtime()
        .read_thread(&thread.id)
        .expect("thread read should succeed")
        .expect("thread should exist");
    assert_eq!(interrupted_thread.status, ThreadStatus::Interrupted);
    assert_eq!(interrupted_thread.latest_turn_id, Some(cancelled_turn.id.clone()));
    assert!(
        interrupted_thread
            .last_interruption
            .as_deref()
            .expect("interruption marker should be present")
            .contains("Implement thread and turn managers")
    );
    assert_eq!(
        interrupted_thread.pending_commitments,
        vec!["Resume interrupted work: Implement thread and turn managers".to_owned()]
    );

    let resumed = match server.handle_request(envelope(
        "request_04",
        ClientRequest::ThreadResume(ThreadResumeRequest {
            thread_id: thread.id.clone(),
        }),
    )) {
        ServerResponseEnvelope::Success(success) => match success.response {
            ResponsePayload::ThreadResume(response) => response,
            other => panic!("unexpected response payload: {other:?}"),
        },
        other => panic!("unexpected response envelope: {other:?}"),
    };

    assert_eq!(resumed.thread.status, ThreadStatus::Active);
    assert_eq!(
        resumed.resume_summary.expect("resume summary should exist").pending_commitments,
        vec!["Resume interrupted work: Implement thread and turn managers".to_owned()]
    );

    let entries = turn_log
        .read_entries(&thread.id)
        .expect("turn log entries should be readable");
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].event, "turn_started");
    assert_eq!(entries[1].event, "turn_cancelled");
}

#[test]
fn forked_thread_inherits_parent_state_without_reusing_latest_turn() {
    let temp_dir = TempDir::new().expect("temp dir should be created");
    let storage_paths = StoragePaths::new(temp_dir.path().join(".liz"));
    let mut server = AppServer::new(storage_paths);

    let parent = match server.handle_request(envelope(
        "request_parent",
        ClientRequest::ThreadStart(ThreadStartRequest {
            title: Some("Main thread".to_owned()),
            initial_goal: Some("Ship Phase 3".to_owned()),
            workspace_ref: None,
        }),
    )) {
        ServerResponseEnvelope::Success(success) => match success.response {
            ResponsePayload::ThreadStart(response) => response.thread,
            other => panic!("unexpected response payload: {other:?}"),
        },
        other => panic!("unexpected response envelope: {other:?}"),
    };

    let child = match server.handle_request(envelope(
        "request_child",
        ClientRequest::ThreadFork(ThreadForkRequest {
            thread_id: parent.id.clone(),
            title: Some("Experiment".to_owned()),
            fork_reason: Some("Try a different lifecycle projection".to_owned()),
        }),
    )) {
        ServerResponseEnvelope::Success(success) => match success.response {
            ResponsePayload::ThreadFork(response) => response.thread,
            other => panic!("unexpected response payload: {other:?}"),
        },
        other => panic!("unexpected response envelope: {other:?}"),
    };

    assert_eq!(child.parent_thread_id, Some(parent.id));
    assert_eq!(child.active_goal.as_deref(), Some("Ship Phase 3"));
    assert!(child.latest_turn_id.is_none());
    assert!(
        child.pending_commitments
            .contains(&"Fork created for: Try a different lifecycle projection".to_owned())
    );
}

fn envelope(request_id: &str, request: ClientRequest) -> ClientRequestEnvelope {
    ClientRequestEnvelope {
        request_id: RequestId::new(request_id),
        request,
    }
}
