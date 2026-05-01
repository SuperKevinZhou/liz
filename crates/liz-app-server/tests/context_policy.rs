//! Context assembly and policy-engine coverage for Phase 6.

use liz_app_server::runtime::{ContextAssembler, RetrievalScope, RuntimeCoordinator};
use liz_app_server::server::{spawn_loopback_websocket, AppServer, WebSocketTransportError};
use liz_app_server::storage::{GlobalMemorySnapshot, StoragePaths, StoredMemoryFact};
use liz_protocol::requests::{
    ApprovalRespondRequest, ClientRequest, ClientRequestEnvelope, ThreadStartRequest,
    TurnInputKind, TurnStartRequest,
};
use liz_protocol::{
    ApprovalDecision, InfoBoundary, MemoryFactId, MemoryFactKind, ParticipantRef,
    RelationshipEntry, RequestId, ServerEventPayload, Thread, ThreadId, ThreadStatus, Timestamp,
    TrustLevel,
};
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
    assert!(context.system_prompt.contains("liz_identity:"));
    assert!(context.system_prompt.contains("resident_wakeup:"));
    assert!(context.developer_prompt.contains("turn_operating_contract:"));
    assert!(context.prompt.contains("resident_wakeup:"));
    assert!(context.prompt.contains("recent_conversation_wakeup:"));
    assert!(context.prompt.contains("executor_boundary:"));
    assert_eq!(context.user_prompt, "Only change one line in src/lib.rs");
    assert_eq!(context.executor_boundary.memory_owner, "liz");
    assert!(!context.executor_boundary.relationship_history_shared);
    assert_eq!(decision.scope, RetrievalScope::Focused);
    assert!(!decision.requires_approval);
}

#[test]
fn context_assembly_surfaces_recent_conversation_wakeup_and_executor_boundary() {
    let temp_dir = TempDir::new().expect("temp dir should be created");
    let mut runtime =
        RuntimeCoordinator::from_storage_paths(StoragePaths::new(temp_dir.path().join(".liz")));
    let thread = runtime
        .start_thread(ThreadStartRequest {
            title: Some("Wake-up context".to_owned()),
            initial_goal: Some("Carry recent conversation forward".to_owned()),
            workspace_ref: None,
        })
        .expect("thread start should succeed")
        .thread;

    let turn = runtime
        .start_turn(TurnStartRequest {
            thread_id: thread.id.clone(),
            input: "Patch the websocket transport retry logic".to_owned(),
            input_kind: TurnInputKind::UserMessage,
            channel: None,
            participant: None,
        })
        .expect("turn start should succeed")
        .turn;
    runtime
        .complete_turn(&thread.id, &turn.id, "Patched websocket transport retry logic".to_owned())
        .expect("turn should complete");

    let context = runtime
        .assemble_context(&thread.id, "Continue the websocket follow-up")
        .expect("context assembly should succeed");

    assert!(context
        .recent_conversation
        .recent_summaries
        .iter()
        .any(|summary| summary.contains("websocket transport retry logic")));
    assert!(context
        .recent_conversation
        .active_topics
        .iter()
        .any(|topic| topic == "websocket" || topic == "transport"));
    assert!(context.system_prompt.contains("personal agent and AI Twin"));
    assert!(context.system_prompt.contains("one continuous self"));
    assert!(context.developer_prompt.contains("first_meeting:"));
    assert!(context.developer_prompt.contains("First Meeting"));
    assert!(context.layers.recent_conversation.contains("recent_summaries:"));
    assert!(context.developer_prompt.contains("thread_projection:"));
    assert!(context.layers.executor_boundary.contains("memory_owner: liz"));
    assert!(context.prompt.contains("controlled task executor"));
}

#[test]
fn context_assembly_uses_conversation_only_surface_without_workspace() {
    let temp_dir = TempDir::new().expect("temp dir should be created");
    let mut runtime =
        RuntimeCoordinator::from_storage_paths(StoragePaths::new(temp_dir.path().join(".liz")));
    let thread = runtime
        .start_thread(ThreadStartRequest {
            title: Some("Conversation only".to_owned()),
            initial_goal: Some("Talk without workspace".to_owned()),
            workspace_ref: None,
        })
        .expect("thread start should succeed")
        .thread;

    let context = runtime
        .assemble_context(&thread.id, "Let's just talk")
        .expect("context assembly should succeed");

    assert!(context.developer_prompt.contains("mode: conversation_only"));
    assert!(context.developer_prompt.contains("canonical_runtime_tools: none"));
    assert!(!context.developer_prompt.contains("workspace.list"));
    assert!(!context.developer_prompt.contains("shell.exec"));
}

#[test]
fn context_assembly_uses_standard_surface_with_workspace() {
    let temp_dir = TempDir::new().expect("temp dir should be created");
    let workspace_ref = temp_dir.path().join("workspace").display().to_string();
    let mut runtime =
        RuntimeCoordinator::from_storage_paths(StoragePaths::new(temp_dir.path().join(".liz")));
    let thread = runtime
        .start_thread(ThreadStartRequest {
            title: Some("Workspace thread".to_owned()),
            initial_goal: Some("Use workspace tools".to_owned()),
            workspace_ref: Some(workspace_ref.clone()),
        })
        .expect("thread start should succeed")
        .thread;

    let context = runtime
        .assemble_context(&thread.id, "Read the project")
        .expect("context assembly should succeed");

    assert_eq!(thread.workspace_ref.as_deref(), Some(workspace_ref.as_str()));
    assert!(context.developer_prompt.contains("mode: standard"));
    assert!(context.developer_prompt.contains(&format!("workspace_root: {workspace_ref}")));
    assert!(context.developer_prompt.contains("workspace.list"));
    assert!(context.developer_prompt.contains("shell.exec"));
}

#[test]
fn context_assembly_filters_relationship_boundaries() {
    let assembler = ContextAssembler;
    let thread = Thread {
        id: ThreadId::new("thread_relationship"),
        title: "Relationship context".to_owned(),
        status: ThreadStatus::Active,
        created_at: Timestamp::new("2026-05-01T00:00:00Z"),
        updated_at: Timestamp::new("2026-05-01T00:00:00Z"),
        active_goal: Some("Discuss project status".to_owned()),
        active_summary: Some(
            "Private personal plans and project status are both active".to_owned(),
        ),
        last_interruption: None,
        workspace_ref: None,
        pending_commitments: vec![
            "Share project status update".to_owned(),
            "Keep personal plans private".to_owned(),
        ],
        latest_turn_id: None,
        latest_checkpoint_id: None,
        parent_thread_id: None,
    };
    let mut snapshot = GlobalMemorySnapshot::empty();
    snapshot.identity_summary = Some("Owner prefers direct Chinese updates".to_owned());
    snapshot.active_state_summary =
        Some("Project status is green; personal plans are private".to_owned());
    snapshot.facts.push(StoredMemoryFact {
        id: MemoryFactId::new("fact_project"),
        kind: MemoryFactKind::ActiveState,
        subject: "project status".to_owned(),
        value: "Project status is green".to_owned(),
        keywords: vec!["project".to_owned()],
        related_thread_ids: vec![thread.id.clone()],
        citations: Vec::new(),
        updated_at: Timestamp::new("2026-05-01T00:00:00Z"),
        invalidated_at: None,
        invalidated_by: None,
    });
    snapshot.facts.push(StoredMemoryFact {
        id: MemoryFactId::new("fact_private"),
        kind: MemoryFactKind::Identity,
        subject: "personal plans".to_owned(),
        value: "Personal plans include a private relocation".to_owned(),
        keywords: vec!["personal".to_owned()],
        related_thread_ids: vec![thread.id.clone()],
        citations: Vec::new(),
        updated_at: Timestamp::new("2026-05-01T00:00:00Z"),
        invalidated_at: None,
        invalidated_by: None,
    });
    snapshot.relationships.push(RelationshipEntry {
        person_id: "telegram_user_7".to_owned(),
        display_name: "Alice".to_owned(),
        trust_level: TrustLevel::Trusted,
        info_boundary: InfoBoundary {
            shared_topics: vec!["project status".to_owned()],
            forbidden_topics: vec!["personal plans".to_owned()],
            share_active_state: true,
            share_commitments: true,
        },
        interaction_stance: "friendly_bounded".to_owned(),
        notes: None,
    });

    let trusted = assembler.assemble(
        &snapshot,
        &thread,
        &[],
        "What can you tell Alice?",
        Some(&ParticipantRef {
            external_participant_id: "telegram_user_7".to_owned(),
            display_name: Some("Alice".to_owned()),
        }),
    );
    assert!(trusted.layers.resident.contains("Project status is green"));
    assert!(!trusted.layers.resident.contains("private relocation"));
    assert!(!trusted.layers.resident.contains("Owner prefers direct Chinese updates"));
    assert!(trusted.developer_prompt.contains("relationship_context:"));

    let stranger = assembler.assemble(
        &snapshot,
        &thread,
        &[],
        "What do you know about the owner?",
        Some(&ParticipantRef { external_participant_id: "unknown".to_owned(), display_name: None }),
    );
    assert!(!stranger.layers.resident.contains("Project status is green"));
    assert!(!stranger.layers.resident.contains("private relocation"));
    assert!(stranger.developer_prompt.contains("extremely conservative"));
}

#[test]
fn risky_turn_stops_for_checkpoint_and_approval_then_resumes_after_approval() {
    let temp_dir = TempDir::new().expect("temp dir should be created");
    let server = AppServer::new_simulated(StoragePaths::new(temp_dir.path().join(".liz")));
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
                channel: None,
                participant: None,
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
