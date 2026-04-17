//! Memory request event coverage for Phase 8.

use liz_app_server::server::AppServer;
use liz_app_server::storage::StoragePaths;
use liz_protocol::requests::{
    ClientRequest, ClientRequestEnvelope, MemoryCompileNowRequest, MemoryReadWakeupRequest,
    ThreadStartRequest,
};
use liz_protocol::{RequestId, ResponsePayload, ServerEventPayload, ServerResponseEnvelope};
use std::time::Duration;
use tempfile::TempDir;

#[test]
fn memory_requests_emit_wakeup_and_compilation_events() {
    let temp_dir = TempDir::new().expect("temp dir should be created");
    let mut server = AppServer::new(StoragePaths::new(temp_dir.path().join(".liz")));
    let events = server.subscribe_events();

    let thread = match server.handle_request(envelope(
        "req_thread",
        ClientRequest::ThreadStart(ThreadStartRequest {
            title: Some("Memory events".to_owned()),
            initial_goal: Some("Surface foreground memory".to_owned()),
            workspace_ref: None,
        }),
    )) {
        ServerResponseEnvelope::Success(success) => match success.response {
            ResponsePayload::ThreadStart(response) => response.thread,
            other => panic!("unexpected thread response: {other:?}"),
        },
        other => panic!("unexpected thread envelope: {other:?}"),
    };

    let _ = events
        .recv_timeout(Duration::from_secs(1))
        .expect("thread_started event should arrive");

    let wakeup_response = server.handle_request(envelope(
        "req_wakeup",
        ClientRequest::MemoryReadWakeup(MemoryReadWakeupRequest {
            thread_id: thread.id.clone(),
        }),
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
    assert!(matches!(
        wakeup_event.payload,
        ServerEventPayload::MemoryWakeupLoaded(_)
    ));

    let compile_response = server.handle_request(envelope(
        "req_compile",
        ClientRequest::MemoryCompileNow(MemoryCompileNowRequest {
            thread_id: thread.id.clone(),
        }),
    ));
    match compile_response {
        ServerResponseEnvelope::Success(success) => {
            assert!(matches!(success.response, ResponsePayload::MemoryCompileNow(_)));
        }
        other => panic!("unexpected compile response: {other:?}"),
    }

    let compilation_event = events
        .recv_timeout(Duration::from_secs(1))
        .expect("memory_compilation_applied event should arrive");
    assert!(matches!(
        compilation_event.payload,
        ServerEventPayload::MemoryCompilationApplied(_)
    ));
}

fn envelope(request_id: &str, request: ClientRequest) -> ClientRequestEnvelope {
    ClientRequestEnvelope { request_id: RequestId::new(request_id), request }
}
