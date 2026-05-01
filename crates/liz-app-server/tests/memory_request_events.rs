//! Memory request event coverage for Phase 8.

use liz_app_server::server::AppServer;
use liz_app_server::storage::StoragePaths;
use liz_protocol::requests::{
    ClientRequest, ClientRequestEnvelope, MemoryCompileNowRequest, MemoryReadWakeupRequest,
    ThreadListRequest, ThreadStartRequest,
};
use liz_protocol::{
    RequestId, ResponsePayload, ServerEventPayload, ServerResponseEnvelope, ThreadStatus,
};
use std::time::Duration;
use tempfile::TempDir;

#[test]
fn memory_requests_emit_wakeup_and_compilation_events() {
    let temp_dir = TempDir::new().expect("temp dir should be created");
    let mut server = AppServer::new_simulated(StoragePaths::new(temp_dir.path().join(".liz")));
    let events = server.subscribe_events();

    let thread = match server.handle_request(envelope(
        "req_thread",
        ClientRequest::ThreadStart(ThreadStartRequest {
            title: Some("Memory events".to_owned()),
            initial_goal: Some("Surface foreground memory".to_owned()),
            workspace_ref: None,
            workspace_mount_id: None,
        }),
    )) {
        ServerResponseEnvelope::Success(success) => match success.response {
            ResponsePayload::ThreadStart(response) => response.thread,
            other => panic!("unexpected thread response: {other:?}"),
        },
        other => panic!("unexpected thread envelope: {other:?}"),
    };

    let _ =
        events.recv_timeout(Duration::from_secs(1)).expect("thread_started event should arrive");

    let wakeup_response = server.handle_request(envelope(
        "req_wakeup",
        ClientRequest::MemoryReadWakeup(MemoryReadWakeupRequest { thread_id: thread.id.clone() }),
    ));
    match wakeup_response {
        ServerResponseEnvelope::Success(success) => {
            assert!(matches!(success.response, ResponsePayload::MemoryReadWakeup(_)));
        }
        other => panic!("unexpected wakeup response: {other:?}"),
    }

    let wakeup_event = events
        .recv_timeout(Duration::from_secs(1))
        .expect("memory_wakeup_loaded event should arrive");
    assert!(matches!(wakeup_event.payload, ServerEventPayload::MemoryWakeupLoaded(_)));

    let compile_response = server.handle_request(envelope(
        "req_compile",
        ClientRequest::MemoryCompileNow(MemoryCompileNowRequest { thread_id: thread.id.clone() }),
    ));
    match compile_response {
        ServerResponseEnvelope::Success(success) => match success.response {
            ResponsePayload::MemoryCompileNow(response) => {
                assert!(!response.compilation.delta_summary.contains("Heuristic fallback"));
                assert!(response.compilation.delta_summary.contains("Updated memory"));
            }
            other => panic!("unexpected compile response: {other:?}"),
        },
        other => panic!("unexpected compile response: {other:?}"),
    }

    let compilation_event = events
        .recv_timeout(Duration::from_secs(1))
        .expect("memory_compilation_applied event should arrive");
    assert!(matches!(compilation_event.payload, ServerEventPayload::MemoryCompilationApplied(_)));
}

#[test]
fn thread_list_request_returns_threads_without_emitting_events() {
    let temp_dir = TempDir::new().expect("temp dir should be created");
    let mut server = AppServer::new(StoragePaths::new(temp_dir.path().join(".liz")));
    let events = server.subscribe_events();

    for (request_id, title) in [("req_thread_1", "First thread"), ("req_thread_2", "Second thread")]
    {
        let response = server.handle_request(envelope(
            request_id,
            ClientRequest::ThreadStart(ThreadStartRequest {
                title: Some(title.to_owned()),
                initial_goal: Some("Populate thread list".to_owned()),
                workspace_ref: None,
                workspace_mount_id: None,
            }),
        ));
        assert!(matches!(
            response,
            ServerResponseEnvelope::Success(success)
                if matches!(success.response, ResponsePayload::ThreadStart(_))
        ));
        let _ = events
            .recv_timeout(Duration::from_secs(1))
            .expect("thread_started event should arrive");
    }

    let response = server.handle_request(envelope(
        "req_list_threads",
        ClientRequest::ThreadList(ThreadListRequest {
            status: Some(ThreadStatus::Active),
            limit: Some(8),
        }),
    ));

    match response {
        ServerResponseEnvelope::Success(success) => match success.response {
            ResponsePayload::ThreadList(response) => {
                assert_eq!(response.threads.len(), 2);
                assert_eq!(response.threads[0].title, "Second thread");
                assert_eq!(response.threads[1].title, "First thread");
                assert!(response
                    .threads
                    .iter()
                    .all(|thread| thread.status == ThreadStatus::Active));
            }
            other => panic!("unexpected thread list response: {other:?}"),
        },
        other => panic!("unexpected thread list envelope: {other:?}"),
    }

    assert!(
        events.recv_timeout(Duration::from_millis(150)).is_err(),
        "thread list should not publish any additional events"
    );
}

fn envelope(request_id: &str, request: ClientRequest) -> ClientRequestEnvelope {
    ClientRequestEnvelope { request_id: RequestId::new(request_id), request }
}
