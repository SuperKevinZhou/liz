//! Foreground memory coverage for Phase 8.

use liz_app_server::runtime::RuntimeCoordinator;
use liz_app_server::storage::StoragePaths;
use liz_protocol::requests::{
    MemoryCompileNowRequest, MemoryListTopicsRequest, MemoryOpenEvidenceRequest,
    MemoryOpenSessionRequest, MemoryReadWakeupRequest, MemorySearchRequest, ThreadResumeRequest,
    ThreadStartRequest, TurnCancelRequest, TurnInputKind, TurnStartRequest,
};
use liz_protocol::{ArtifactKind, MemorySearchMode, ThreadStatus, TurnStatus};
use tempfile::TempDir;

#[test]
fn foreground_memory_compile_and_recall_flows_round_trip() {
    let temp_dir = TempDir::new().expect("temp dir should be created");
    let mut runtime =
        RuntimeCoordinator::from_storage_paths(StoragePaths::new(temp_dir.path().join(".liz")));

    let thread = runtime
        .start_thread(ThreadStartRequest {
            title: Some("Phase 8 memory".to_owned()),
            initial_goal: Some("Implement foreground recall".to_owned()),
            workspace_ref: None,
        })
        .expect("thread start should succeed")
        .thread;

    let turn = runtime
        .start_turn(TurnStartRequest {
            thread_id: thread.id.clone(),
            input: "Implement websocket memory wakeup search".to_owned(),
            input_kind: TurnInputKind::UserMessage,
        })
        .expect("turn start should succeed")
        .turn;
    assert_eq!(turn.status, TurnStatus::Running);

    let (_execution_turn_id, artifact_refs) = runtime
        .record_tool_execution(
            &thread.id,
            Some(&turn.id),
            "workspace.read",
            "Read memory context",
            vec![(
                ArtifactKind::MemoryCitation,
                "Memory context excerpt".to_owned(),
                "{\"excerpt\":\"foreground memory wakeup\"}".to_owned(),
            )],
        )
        .expect("tool execution should be recorded");

    runtime
        .complete_turn(
            &thread.id,
            &turn.id,
            "Foreground memory compile kept websocket recall stable".to_owned(),
        )
        .expect("turn completion should succeed");

    let compilation = runtime
        .compile_memory_now(MemoryCompileNowRequest { thread_id: thread.id.clone() })
        .expect("compile_now should succeed")
        .compilation;
    assert!(
        compilation.recent_topics.iter().any(|topic| topic == "memory" || topic == "websocket"),
        "expected compiled topics to include memory keywords: {:?}",
        compilation.recent_topics
    );

    let wakeup = runtime
        .read_memory_wakeup(MemoryReadWakeupRequest { thread_id: thread.id.clone() })
        .expect("read_wakeup should succeed");
    assert_eq!(
        wakeup.wakeup.active_state.as_deref(),
        Some("Foreground memory compile kept websocket recall stable")
    );
    assert!(
        wakeup
            .recent_conversation
            .recent_summaries
            .iter()
            .any(|summary| summary.contains("Read memory context")),
        "expected recent wake-up summaries to surface tool evidence"
    );

    let topics = runtime
        .list_memory_topics(MemoryListTopicsRequest { status: None, limit: None })
        .expect("list topics should succeed")
        .topics;
    assert!(
        topics.iter().any(|topic| topic.name == "memory" || topic.name == "websocket"),
        "expected topic index to retain memory topics: {topics:?}"
    );

    let keyword_hits = runtime
        .search_memory(MemorySearchRequest {
            query: "websocket memory".to_owned(),
            mode: MemorySearchMode::Keyword,
            limit: None,
        })
        .expect("keyword search should succeed")
        .hits;
    assert!(!keyword_hits.is_empty(), "expected keyword hits for foreground memory");

    let semantic_hits = runtime
        .search_memory(MemorySearchRequest {
            query: "recall flow for websocket".to_owned(),
            mode: MemorySearchMode::Semantic,
            limit: None,
        })
        .expect("semantic search should succeed")
        .hits;
    assert!(!semantic_hits.is_empty(), "expected semantic hits for foreground memory");

    let session = runtime
        .open_memory_session(MemoryOpenSessionRequest { thread_id: thread.id.clone() })
        .expect("open_session should succeed")
        .session;
    assert_eq!(session.thread_id, thread.id);
    assert!(session.recent_entries.len() >= 2);
    assert_eq!(session.artifacts.len(), artifact_refs.len());

    let artifact_evidence = runtime
        .open_memory_evidence(MemoryOpenEvidenceRequest {
            thread_id: thread.id.clone(),
            turn_id: None,
            artifact_id: Some(artifact_refs[0].id.clone()),
            fact_id: None,
        })
        .expect("artifact evidence should open")
        .evidence;
    assert!(artifact_evidence
        .artifact_body
        .as_deref()
        .expect("artifact body should be present")
        .contains("foreground memory wakeup"));

    let fact_id = wakeup
        .wakeup
        .citation_fact_ids
        .first()
        .cloned()
        .expect("compiled memory should cite at least one fact");
    let fact_evidence = runtime
        .open_memory_evidence(MemoryOpenEvidenceRequest {
            thread_id: thread.id.clone(),
            turn_id: None,
            artifact_id: None,
            fact_id: Some(fact_id.clone()),
        })
        .expect("fact evidence should open")
        .evidence;
    assert_eq!(fact_evidence.fact_id, Some(fact_id));
    assert!(fact_evidence.fact_value.is_some(), "compiled fact should resolve to a value");
}

#[test]
fn foreground_memory_marks_superseded_commitments_as_invalidated() {
    let temp_dir = TempDir::new().expect("temp dir should be created");
    let mut runtime =
        RuntimeCoordinator::from_storage_paths(StoragePaths::new(temp_dir.path().join(".liz")));

    let thread = runtime
        .start_thread(ThreadStartRequest {
            title: Some("Invalidate commitments".to_owned()),
            initial_goal: Some("Track commitment invalidation".to_owned()),
            workspace_ref: None,
        })
        .expect("thread start should succeed")
        .thread;

    let turn = runtime
        .start_turn(TurnStartRequest {
            thread_id: thread.id.clone(),
            input: "Refine invalidation marker".to_owned(),
            input_kind: TurnInputKind::UserMessage,
        })
        .expect("turn start should succeed")
        .turn;

    runtime
        .cancel_turn(TurnCancelRequest { thread_id: thread.id.clone(), turn_id: turn.id.clone() })
        .expect("turn cancel should succeed");
    let first_compile = runtime
        .compile_memory_now(MemoryCompileNowRequest { thread_id: thread.id.clone() })
        .expect("first compile should succeed")
        .compilation;
    assert!(
        !first_compile.updated_fact_ids.is_empty(),
        "interrupted work should create a commitment fact"
    );

    let resumed = runtime
        .resume_thread(ThreadResumeRequest { thread_id: thread.id.clone() })
        .expect("thread resume should succeed");
    assert_eq!(resumed.thread.status, ThreadStatus::Active);

    let resumed_turn = runtime
        .start_turn(TurnStartRequest {
            thread_id: thread.id.clone(),
            input: "Refine invalidation marker".to_owned(),
            input_kind: TurnInputKind::ResumeCommand,
        })
        .expect("resumed turn should start")
        .turn;
    runtime
        .complete_turn(&thread.id, &resumed_turn.id, "Resolved invalidation marker".to_owned())
        .expect("resumed turn should complete");

    let second_compile = runtime
        .compile_memory_now(MemoryCompileNowRequest { thread_id: thread.id.clone() })
        .expect("second compile should succeed")
        .compilation;
    assert!(
        !second_compile.invalidated_fact_ids.is_empty(),
        "resolved commitments should invalidate the stale commitment fact"
    );
}
