//! Tool-surface coverage for the read-only workspace slice.

use liz_app_server::server::{spawn_loopback_websocket, AppServer};
use liz_app_server::storage::StoragePaths;
use liz_protocol::requests::{ClientRequest, ClientRequestEnvelope, ThreadStartRequest};
use liz_protocol::{
    RequestId, ResponsePayload, ServerEventPayload, ServerResponseEnvelope, ToolCallRequest,
    ToolInvocation, ToolResult, WorkspaceApplyPatchRequest, WorkspaceListRequest,
    WorkspaceReadRequest, WorkspaceSearchRequest, WorkspaceWriteTextRequest,
};
use std::fs;
use std::time::Duration;
use tempfile::TempDir;

#[test]
fn workspace_read_only_tools_return_results_and_artifacts() {
    let temp_dir = TempDir::new().expect("temp dir should be created");
    let workspace_root = temp_dir.path().join("workspace");
    fs::create_dir_all(workspace_root.join("src")).expect("workspace directories should exist");
    fs::write(workspace_root.join("src/lib.rs"), "fn main() {\n    println!(\"liz\");\n}\n")
        .expect("workspace file should be written");
    fs::write(workspace_root.join("README.md"), "liz keeps context forward\n")
        .expect("workspace file should be written");

    let server = AppServer::new(StoragePaths::new(temp_dir.path().join(".liz")));
    let client = spawn_loopback_websocket(server);

    client
        .send_request(envelope(
            "request_01",
            ClientRequest::ThreadStart(ThreadStartRequest {
                title: Some("Workspace tools".to_owned()),
                initial_goal: Some("Run read-only workspace tools".to_owned()),
                workspace_ref: Some(workspace_root.to_string_lossy().to_string()),
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

    let list_response = send_tool(
        &client,
        "request_02",
        ToolInvocation::WorkspaceList(WorkspaceListRequest {
            root: workspace_root.to_string_lossy().to_string(),
            recursive: true,
            include_hidden: false,
            max_entries: Some(10),
        }),
        &thread.id,
    );
    match list_response.result {
        ToolResult::WorkspaceList(result) => {
            assert!(
                result.entries.iter().any(|entry| entry.path == "src/lib.rs"),
                "workspace.list should include src/lib.rs: {result:?}"
            );
        }
        other => panic!("unexpected list result: {other:?}"),
    }
    assert_eq!(list_response.artifact_refs.len(), 2);
    assert!(matches!(
        client.recv_event_timeout(Duration::from_secs(1)).expect("artifact event"),
        event if matches!(event.payload, ServerEventPayload::ArtifactCreated(_))
    ));
    assert!(matches!(
        client.recv_event_timeout(Duration::from_secs(1)).expect("artifact event"),
        event if matches!(event.payload, ServerEventPayload::ArtifactCreated(_))
    ));
    assert!(matches!(
        client.recv_event_timeout(Duration::from_secs(1)).expect("tool completed event"),
        event if matches!(event.payload, ServerEventPayload::ToolCompleted(_))
    ));

    let search_response = send_tool(
        &client,
        "request_03",
        ToolInvocation::WorkspaceSearch(WorkspaceSearchRequest {
            root: workspace_root.to_string_lossy().to_string(),
            pattern: "println!".to_owned(),
            case_sensitive: true,
            include_hidden: false,
            max_results: Some(10),
        }),
        &thread.id,
    );
    match search_response.result {
        ToolResult::WorkspaceSearch(result) => {
            assert_eq!(result.matches.len(), 1);
            assert_eq!(result.matches[0].path, "src/lib.rs");
            assert_eq!(result.matches[0].line_number, 2);
        }
        other => panic!("unexpected search result: {other:?}"),
    }
    for _ in 0..3 {
        client
            .recv_event_timeout(Duration::from_secs(1))
            .expect("search tool events should arrive");
    }

    let read_response = send_tool(
        &client,
        "request_04",
        ToolInvocation::WorkspaceRead(WorkspaceReadRequest {
            path: workspace_root.join("src/lib.rs").to_string_lossy().to_string(),
            start_line: Some(2),
            end_line: Some(2),
        }),
        &thread.id,
    );
    match read_response.result {
        ToolResult::WorkspaceRead(result) => {
            assert_eq!(result.start_line, 2);
            assert_eq!(result.end_line, 2);
            assert!(result.content.contains("println!(\"liz\")"));
        }
        other => panic!("unexpected read result: {other:?}"),
    }
    for _ in 0..3 {
        client.recv_event_timeout(Duration::from_secs(1)).expect("read tool events should arrive");
    }
}

#[test]
fn workspace_mutating_tools_write_files_and_publish_diff_artifacts() {
    let temp_dir = TempDir::new().expect("temp dir should be created");
    let workspace_root = temp_dir.path().join("workspace");
    fs::create_dir_all(&workspace_root).expect("workspace root should exist");
    let file_path = workspace_root.join("notes.txt");
    fs::write(&file_path, "alpha\nbeta\n").expect("initial file should be written");

    let server = AppServer::new(StoragePaths::new(temp_dir.path().join(".liz")));
    let client = spawn_loopback_websocket(server);

    client
        .send_request(envelope(
            "request_11",
            ClientRequest::ThreadStart(ThreadStartRequest {
                title: Some("Workspace write tools".to_owned()),
                initial_goal: Some("Write and patch files".to_owned()),
                workspace_ref: Some(workspace_root.to_string_lossy().to_string()),
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

    let write_response = send_tool(
        &client,
        "request_12",
        ToolInvocation::WorkspaceWriteText(WorkspaceWriteTextRequest {
            path: file_path.to_string_lossy().to_string(),
            content: "gamma\ndelta\n".to_owned(),
        }),
        &thread.id,
    );
    match write_response.result {
        ToolResult::WorkspaceWriteText(result) => {
            assert!(result.changed);
            assert_eq!(result.path, file_path.to_string_lossy());
        }
        other => panic!("unexpected write result: {other:?}"),
    }
    assert_eq!(
        fs::read_to_string(&file_path).expect("written file should be readable"),
        "gamma\ndelta\n"
    );
    let write_events = collect_tool_events(&client, 5);
    assert!(write_events
        .iter()
        .any(|event| matches!(event.payload, ServerEventPayload::DiffAvailable(_))));

    let patch_response = send_tool(
        &client,
        "request_13",
        ToolInvocation::WorkspaceApplyPatch(WorkspaceApplyPatchRequest {
            path: file_path.to_string_lossy().to_string(),
            search: "delta".to_owned(),
            replace: "epsilon".to_owned(),
            replace_all: false,
        }),
        &thread.id,
    );
    match patch_response.result {
        ToolResult::WorkspaceApplyPatch(result) => {
            assert!(result.changed);
            assert_eq!(result.replacements, 1);
        }
        other => panic!("unexpected patch result: {other:?}"),
    }
    assert_eq!(
        fs::read_to_string(&file_path).expect("patched file should be readable"),
        "gamma\nepsilon\n"
    );
    let patch_events = collect_tool_events(&client, 5);
    assert!(patch_events
        .iter()
        .any(|event| matches!(event.payload, ServerEventPayload::DiffAvailable(_))));
}

fn send_tool(
    client: &liz_app_server::server::LoopbackWebSocketClient,
    request_id: &str,
    invocation: ToolInvocation,
    thread_id: &liz_protocol::ThreadId,
) -> liz_protocol::ToolCallResponse {
    client
        .send_request(envelope(
            request_id,
            ClientRequest::ToolCall(ToolCallRequest {
                thread_id: thread_id.clone(),
                turn_id: None,
                invocation,
            }),
        ))
        .expect("tool request should be sent");

    match client.recv_response().expect("tool response should arrive") {
        ServerResponseEnvelope::Success(success) => match success.response {
            ResponsePayload::ToolCall(response) => response,
            other => panic!("unexpected response payload: {other:?}"),
        },
        other => panic!("unexpected response envelope: {other:?}"),
    }
}

fn envelope(request_id: &str, request: ClientRequest) -> ClientRequestEnvelope {
    ClientRequestEnvelope { request_id: RequestId::new(request_id), request }
}

fn collect_tool_events(
    client: &liz_app_server::server::LoopbackWebSocketClient,
    count: usize,
) -> Vec<liz_protocol::ServerEvent> {
    (0..count)
        .map(|_| {
            client.recv_event_timeout(Duration::from_secs(1)).expect("tool event should arrive")
        })
        .collect()
}
