//! Runtime lifecycle coverage for Phase 3.

use liz_app_server::runtime::RuntimeCoordinator;
use liz_app_server::storage::FsTurnLog;
use liz_app_server::storage::{StoragePaths, TurnLog};
use liz_protocol::requests::{
    ThreadForkRequest, ThreadResumeRequest, ThreadStartRequest, TurnCancelRequest, TurnInputKind,
    TurnStartRequest,
};
use liz_protocol::{ThreadStatus, TurnStatus};
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
        })
        .expect("thread start should succeed")
        .thread;

    assert_eq!(thread.status, ThreadStatus::Active);
    assert_eq!(thread.active_goal.as_deref(), Some("Stand up runtime lifecycle"));

    let turn = runtime
        .start_turn(TurnStartRequest {
            thread_id: thread.id.clone(),
            input: "Implement thread and turn managers".to_owned(),
            input_kind: TurnInputKind::UserMessage,
            channel: None,
            participant: None,
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
            workspace_ref: None,
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

    assert_eq!(child.parent_thread_id, Some(parent.id));
    assert_eq!(child.active_goal.as_deref(), Some("Ship Phase 3"));
    assert!(child.latest_turn_id.is_none());
    assert!(child
        .pending_commitments
        .contains(&"Fork created for: Try a different lifecycle projection".to_owned()));
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
