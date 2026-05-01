//! Runtime lifecycle coverage for Phase 3.

use liz_app_server::runtime::RuntimeCoordinator;
use liz_app_server::storage::FsTurnLog;
use liz_app_server::storage::{StoragePaths, TurnLog};
use liz_protocol::requests::{
    MemorySurfaceAboutYouReadRequest, MemorySurfaceAboutYouUpdateRequest, ThreadForkRequest,
    ThreadResumeRequest, ThreadStartRequest, TurnCancelRequest, TurnInputKind, TurnStartRequest,
    WorkspaceMountListRequest,
};
use liz_protocol::{
    AboutYouItem, AboutYouUpdate, InboundEventAction, NodeHeartbeatRequest, NodeId, NodeStatus,
    ThreadStatus, Timestamp, TurnStatus,
};
use tempfile::TempDir;

#[test]
fn thread_can_start_turn_interrupt_and_resume() {
    let temp_dir = TempDir::new().expect("temp dir should be created");
    let storage_paths = StoragePaths::new(temp_dir.path().join(".liz"));
    let turn_log = FsTurnLog::new(storage_paths.clone());
    let mut runtime = RuntimeCoordinator::from_storage_paths(storage_paths);

    let thread = runtime
        .start_thread(ThreadStartRequest {
            title: Some("Phase 3 runtime".to_owned()),
            initial_goal: Some("Stand up runtime lifecycle".to_owned()),
            workspace_ref: None,
            workspace_mount_id: None,
        })
        .expect("thread start should succeed")
        .thread;

    assert_eq!(thread.status, ThreadStatus::Active);
    assert_eq!(thread.active_goal.as_deref(), Some("Stand up runtime lifecycle"));
    assert_eq!(thread.workspace_ref, None);

    let turn = runtime
        .start_turn(TurnStartRequest {
            thread_id: thread.id.clone(),
            input: "Implement thread and turn managers".to_owned(),
            input_kind: TurnInputKind::UserMessage,
            channel: None,
            participant: None,
            interaction_context: None,
        })
        .expect("turn start should succeed")
        .turn;

    assert_eq!(turn.status, TurnStatus::Running);
    assert_eq!(
        runtime.read_turn(&turn.id).expect("turn should remain active").summary.as_deref(),
        Some("Started turn for: Implement thread and turn managers")
    );

    let cancelled_turn = runtime
        .cancel_turn(TurnCancelRequest { thread_id: thread.id.clone(), turn_id: turn.id.clone() })
        .expect("turn cancel should succeed")
        .turn;

    assert_eq!(cancelled_turn.status, TurnStatus::Cancelled);
    assert!(runtime.read_turn(&cancelled_turn.id).is_none());

    let interrupted_thread = runtime
        .read_thread(&thread.id)
        .expect("thread read should succeed")
        .expect("thread should exist");
    assert_eq!(interrupted_thread.status, ThreadStatus::Interrupted);
    assert_eq!(interrupted_thread.latest_turn_id, Some(cancelled_turn.id.clone()));
    assert!(interrupted_thread
        .last_interruption
        .as_deref()
        .expect("interruption marker should be present")
        .contains("Implement thread and turn managers"));
    assert_eq!(
        interrupted_thread.pending_commitments,
        vec!["Resume interrupted work: Implement thread and turn managers".to_owned()]
    );

    let resumed = runtime
        .resume_thread(ThreadResumeRequest { thread_id: thread.id.clone() })
        .expect("thread resume should succeed");

    assert_eq!(resumed.thread.status, ThreadStatus::Active);
    assert_eq!(resumed.thread.workspace_ref, None);
    assert_eq!(resumed.thread.updated_at, interrupted_thread.updated_at);
    assert_eq!(
        resumed.resume_summary.expect("resume summary should exist").pending_commitments,
        vec!["Resume interrupted work: Implement thread and turn managers".to_owned()]
    );

    let entries = turn_log.read_entries(&thread.id).expect("turn log entries should be readable");
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].event, "turn_started");
    assert_eq!(entries[1].event, "turn_cancelled");
}

#[test]
fn forked_thread_inherits_parent_state_without_reusing_latest_turn() {
    let temp_dir = TempDir::new().expect("temp dir should be created");
    let storage_paths = StoragePaths::new(temp_dir.path().join(".liz"));
    let mut runtime = RuntimeCoordinator::from_storage_paths(storage_paths);

    let parent = runtime
        .start_thread(ThreadStartRequest {
            title: Some("Main thread".to_owned()),
            initial_goal: Some("Ship Phase 3".to_owned()),
            workspace_ref: Some("D:/workspace/main".to_owned()),
            workspace_mount_id: None,
        })
        .expect("thread start should succeed")
        .thread;

    let child = runtime
        .fork_thread(ThreadForkRequest {
            thread_id: parent.id.clone(),
            title: Some("Experiment".to_owned()),
            fork_reason: Some("Try a different lifecycle projection".to_owned()),
        })
        .expect("thread fork should succeed")
        .thread;

    assert_eq!(child.parent_thread_id, Some(parent.id.clone()));
    assert_eq!(child.workspace_ref.as_deref(), Some("D:/workspace/main"));
    assert_eq!(child.workspace_mount_id, parent.workspace_mount_id);
    assert_eq!(child.active_goal.as_deref(), Some("Ship Phase 3"));
    assert!(child.latest_turn_id.is_none());
    assert!(child
        .pending_commitments
        .contains(&"Fork created for: Try a different lifecycle projection".to_owned()));
}

#[test]
fn thread_start_resolves_workspace_ref_to_local_mount() {
    let temp_dir = TempDir::new().expect("temp dir should be created");
    let workspace_root = temp_dir.path().join("workspace");
    std::fs::create_dir_all(&workspace_root).expect("workspace root should be created");
    let mut runtime =
        RuntimeCoordinator::from_storage_paths(StoragePaths::new(temp_dir.path().join(".liz")));

    let thread = runtime
        .start_thread(ThreadStartRequest {
            title: Some("Mounted workspace".to_owned()),
            initial_goal: Some("Use a node-scoped workspace mount".to_owned()),
            workspace_ref: Some(workspace_root.display().to_string()),
            workspace_mount_id: None,
        })
        .expect("thread start should resolve workspace mount")
        .thread;

    let workspace_mount_id =
        thread.workspace_mount_id.clone().expect("thread should reference a workspace mount");
    let mount = runtime
        .read_workspace_mount(&workspace_mount_id)
        .expect("workspace mount should be readable");
    let (node_id, scoped_mount_id) =
        runtime.thread_execution_scope(&thread.id).expect("thread execution scope should resolve");

    assert_eq!(mount.node_id.as_str(), "local");
    assert_eq!(mount.root_path, workspace_root.display().to_string());
    assert_eq!(node_id.as_str(), "local");
    assert_eq!(scoped_mount_id, Some(workspace_mount_id));
}

#[test]
fn workspace_mount_registry_survives_runtime_restart() {
    let temp_dir = TempDir::new().expect("temp dir should be created");
    let storage_paths = StoragePaths::new(temp_dir.path().join(".liz"));
    let workspace_root = temp_dir.path().join("restartable-workspace");
    std::fs::create_dir_all(&workspace_root).expect("workspace root should be created");

    let (thread_id, workspace_mount_id) = {
        let mut runtime = RuntimeCoordinator::from_storage_paths(storage_paths.clone());
        let thread = runtime
            .start_thread(ThreadStartRequest {
                title: Some("Restartable mount".to_owned()),
                initial_goal: Some("Keep node-scoped workspace identity".to_owned()),
                workspace_ref: Some(workspace_root.display().to_string()),
                workspace_mount_id: None,
            })
            .expect("thread start should resolve workspace mount")
            .thread;
        (thread.id, thread.workspace_mount_id.expect("thread should reference workspace mount"))
    };

    let restarted_runtime = RuntimeCoordinator::from_storage_paths(storage_paths);
    let mount = restarted_runtime
        .read_workspace_mount(&workspace_mount_id)
        .expect("persisted workspace mount should be readable");
    let (node_id, scoped_mount_id) = restarted_runtime
        .thread_execution_scope(&thread_id)
        .expect("thread scope should survive restart");
    let listed_mounts = restarted_runtime
        .list_workspace_mounts(WorkspaceMountListRequest { node_id: None })
        .expect("workspace mounts should list")
        .mounts;

    assert_eq!(mount.node_id.as_str(), "local");
    assert_eq!(mount.root_path, workspace_root.display().to_string());
    assert_eq!(node_id.as_str(), "local");
    assert_eq!(scoped_mount_id, Some(workspace_mount_id.clone()));
    assert!(listed_mounts.iter().any(|mount| mount.workspace_id == workspace_mount_id));
}

#[test]
fn node_heartbeat_updates_liveness_without_turn_action() {
    let temp_dir = TempDir::new().expect("temp dir should be created");
    let storage_paths = StoragePaths::new(temp_dir.path().join(".liz"));
    let heartbeat_at = Timestamp::new("2026-05-01T13:00:00Z");

    {
        let mut runtime = RuntimeCoordinator::from_storage_paths(storage_paths.clone());
        let existing = runtime
            .read_node(liz_protocol::NodeReadRequest { node_id: NodeId::new("local") })
            .expect("local node should exist")
            .node;
        let response = runtime
            .heartbeat_node(NodeHeartbeatRequest {
                node_id: NodeId::new("local"),
                status: NodeStatus {
                    online: true,
                    last_seen_at: Some(heartbeat_at.clone()),
                    app_version: Some("test-version".to_owned()),
                    os: existing.status.os,
                    hostname: Some("test-host".to_owned()),
                },
            })
            .expect("heartbeat should update local node");

        assert_eq!(response.action, InboundEventAction::StoreOnly);
        assert_eq!(response.node.status.last_seen_at, Some(heartbeat_at.clone()));
        assert_eq!(response.node.status.hostname.as_deref(), Some("test-host"));
    }

    let restarted_runtime = RuntimeCoordinator::from_storage_paths(storage_paths);
    let node = restarted_runtime
        .read_node(liz_protocol::NodeReadRequest { node_id: NodeId::new("local") })
        .expect("local node should be readable")
        .node;

    assert_eq!(node.status.last_seen_at, Some(heartbeat_at));
}

#[test]
fn about_you_items_persist_as_identity_facts() {
    let temp_dir = TempDir::new().expect("temp dir should be created");
    let storage_paths = StoragePaths::new(temp_dir.path().join(".liz"));

    {
        let runtime = RuntimeCoordinator::from_storage_paths(storage_paths.clone());
        runtime
            .update_about_you_surface(MemorySurfaceAboutYouUpdateRequest {
                update: AboutYouUpdate {
                    identity_summary: Some(
                        "Owner prefers direct implementation updates".to_owned(),
                    ),
                    items: vec![AboutYouItem {
                        key: "language".to_owned(),
                        label: "Language preference".to_owned(),
                        value: "Chinese".to_owned(),
                        confirmed: true,
                        source_fact_id: None,
                    }],
                },
            })
            .expect("about you update should persist");
    }

    let restarted_runtime = RuntimeCoordinator::from_storage_paths(storage_paths);
    let surface = restarted_runtime
        .read_about_you_surface(MemorySurfaceAboutYouReadRequest {})
        .expect("about you surface should load")
        .surface;

    assert_eq!(
        surface.identity_summary.as_deref(),
        Some("Owner prefers direct implementation updates")
    );
    assert!(surface
        .items
        .iter()
        .any(|item| { item.key == "language" && item.value == "Chinese" && item.confirmed }));
}

#[test]
fn turn_log_sequence_continues_after_runtime_restart() {
    let temp_dir = TempDir::new().expect("temp dir should be created");
    let storage_paths = StoragePaths::new(temp_dir.path().join(".liz"));
    let turn_log = FsTurnLog::new(storage_paths.clone());

    let thread_id = {
        let mut runtime = RuntimeCoordinator::from_storage_paths(storage_paths.clone());
        let thread = runtime
            .start_thread(ThreadStartRequest {
                title: Some("Restartable thread".to_owned()),
                initial_goal: Some("Keep turn-log order stable".to_owned()),
                workspace_ref: None,
                workspace_mount_id: None,
            })
            .expect("thread start should succeed")
            .thread;
        let turn = runtime
            .start_turn(TurnStartRequest {
                thread_id: thread.id.clone(),
                input: "Prepare the first lifecycle step".to_owned(),
                input_kind: TurnInputKind::UserMessage,
                channel: None,
                participant: None,
                interaction_context: None,
            })
            .expect("turn start should succeed")
            .turn;
        runtime
            .cancel_turn(TurnCancelRequest { thread_id: thread.id.clone(), turn_id: turn.id })
            .expect("turn cancel should succeed");
        thread.id
    };

    let mut restarted_runtime = RuntimeCoordinator::from_storage_paths(storage_paths);
    let resumed_turn = restarted_runtime
        .start_turn(TurnStartRequest {
            thread_id: thread_id.clone(),
            input: "Resume after restart".to_owned(),
            input_kind: TurnInputKind::ResumeCommand,
            channel: None,
            participant: None,
            interaction_context: None,
        })
        .expect("turn should restart cleanly")
        .turn;
    restarted_runtime
        .cancel_turn(TurnCancelRequest { thread_id: thread_id.clone(), turn_id: resumed_turn.id })
        .expect("restart turn cancel should succeed");

    let entries = turn_log.read_entries(&thread_id).expect("turn log entries should be readable");
    let sequences: Vec<u64> = entries.iter().map(|entry| entry.sequence).collect();

    assert_eq!(sequences, vec![1, 2, 3, 4]);
}
