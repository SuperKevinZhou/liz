//! Filesystem storage coverage for the app-server storage layer.

use liz_app_server::storage::{
    ArtifactStore, CheckpointStore, FsArtifactStore, FsCheckpointStore, FsGlobalMemoryStore,
    FsThreadStore, FsTurnLog, GlobalMemorySnapshot, GlobalMemoryStore, StoragePaths,
    StoredArtifact, StoredMemoryFact, StoredTopicRecord, ThreadStore, TurnLog, TurnLogEntry,
};
use liz_protocol::{
    ArtifactId, ArtifactKind, ArtifactRef, Checkpoint, CheckpointId, CheckpointScope, InfoBoundary,
    MemoryCitationRef, MemoryFactId, MemoryFactKind, MemoryTopicStatus, Thread, ThreadId,
    ThreadStatus, Timestamp, TurnId,
};
use tempfile::TempDir;

/// Ensures the global memory snapshot can be persisted and loaded again.
#[test]
fn global_memory_store_round_trips_snapshot() {
    let temp_dir = TempDir::new().expect("temp dir should be created");
    let store = FsGlobalMemoryStore::new(StoragePaths::new(temp_dir.path().join(".liz")));
    let snapshot = GlobalMemorySnapshot {
        identity_summary: Some("User prefers concise implementation notes".to_owned()),
        active_state_summary: Some("Foreground memory keeps the active thread ready".to_owned()),
        active_thread_ids: vec![ThreadId::new("thread_01")],
        recent_topics: vec!["memory".to_owned(), "wakeup".to_owned()],
        recent_keywords: vec!["foreground".to_owned(), "recall".to_owned()],
        facts: vec![StoredMemoryFact {
            id: MemoryFactId::new("fact_01"),
            kind: MemoryFactKind::Identity,
            subject: "user.preference".to_owned(),
            value: "likes direct answers".to_owned(),
            keywords: vec!["preference".to_owned(), "direct".to_owned()],
            related_thread_ids: vec![ThreadId::new("thread_01")],
            citations: vec![MemoryCitationRef {
                thread_id: ThreadId::new("thread_01"),
                turn_id: Some(TurnId::new("turn_01")),
                artifact_id: None,
                note: "Preference captured during kickoff".to_owned(),
            }],
            updated_at: Timestamp::new("2026-04-13T21:00:00Z"),
            invalidated_at: None,
            invalidated_by: None,
        }],
        topic_index: vec![StoredTopicRecord {
            name: "memory".to_owned(),
            aliases: vec!["recall".to_owned()],
            summary: "Foreground memory work".to_owned(),
            status: MemoryTopicStatus::Active,
            last_active_at: Timestamp::new("2026-04-13T21:00:00Z"),
            related_thread_ids: vec![ThreadId::new("thread_01")],
            related_artifact_ids: vec![ArtifactId::new("artifact_01")],
            citation_fact_ids: vec![MemoryFactId::new("fact_01")],
            recent_keywords: vec!["foreground".to_owned(), "recall".to_owned()],
            citations: vec![MemoryCitationRef {
                thread_id: ThreadId::new("thread_01"),
                turn_id: Some(TurnId::new("turn_01")),
                artifact_id: Some(ArtifactId::new("artifact_01")),
                note: "Foreground memory evidence".to_owned(),
            }],
        }],
        relationships: Vec::new(),
        default_stranger_boundary: InfoBoundary::stranger_default(),
    };

    store.write_snapshot(&snapshot).expect("snapshot should be written");

    let round_trip = store.read_snapshot().expect("snapshot should be read");

    assert_eq!(round_trip, snapshot);
}

/// Ensures thread records can be written, read back, and listed.
#[test]
fn thread_store_round_trips_thread_records() {
    let temp_dir = TempDir::new().expect("temp dir should be created");
    let store = FsThreadStore::new(StoragePaths::new(temp_dir.path().join(".liz")));
    let thread = Thread {
        id: ThreadId::new("thread_01"),
        title: "Implement storage".to_owned(),
        status: ThreadStatus::Active,
        created_at: Timestamp::new("2026-04-13T21:01:00Z"),
        updated_at: Timestamp::new("2026-04-13T21:02:00Z"),
        active_goal: Some("Persist thread state".to_owned()),
        active_summary: Some("Storage interface committed".to_owned()),
        last_interruption: None,
        workspace_ref: Some("D:/zzh/Code/liz/liz".to_owned()),
        pending_commitments: vec!["Add filesystem tests".to_owned()],
        latest_turn_id: Some(TurnId::new("turn_01")),
        latest_checkpoint_id: Some(CheckpointId::new("checkpoint_01")),
        parent_thread_id: None,
    };

    store.put_thread(&thread).expect("thread should be written");

    let loaded = store
        .get_thread(&thread.id)
        .expect("thread read should succeed")
        .expect("thread should exist");
    let listed = store.list_threads().expect("thread list should succeed");

    assert_eq!(loaded, thread);
    assert_eq!(listed, vec![thread]);
}

/// Ensures append-only turn logs preserve append order on disk.
#[test]
fn turn_log_preserves_append_order() {
    let temp_dir = TempDir::new().expect("temp dir should be created");
    let store = FsTurnLog::new(StoragePaths::new(temp_dir.path().join(".liz")));
    let first = TurnLogEntry {
        thread_id: ThreadId::new("thread_01"),
        sequence: 1,
        turn_id: Some(TurnId::new("turn_01")),
        recorded_at: Timestamp::new("2026-04-13T21:03:00Z"),
        event: "turn_started".to_owned(),
        summary: "Started turn".to_owned(),
        artifact_ids: Vec::new(),
    };
    let second = TurnLogEntry {
        thread_id: ThreadId::new("thread_01"),
        sequence: 2,
        turn_id: Some(TurnId::new("turn_01")),
        recorded_at: Timestamp::new("2026-04-13T21:03:05Z"),
        event: "turn_completed".to_owned(),
        summary: "Completed turn".to_owned(),
        artifact_ids: Vec::new(),
    };

    store.append_entry(&first).expect("first log entry should append");
    store.append_entry(&second).expect("second log entry should append");

    let entries =
        store.read_entries(&ThreadId::new("thread_01")).expect("turn log should be readable");

    assert_eq!(entries, vec![first, second]);
}

/// Ensures artifact payloads can be stored and recovered by identifier.
#[test]
fn artifact_store_round_trips_payloads() {
    let temp_dir = TempDir::new().expect("temp dir should be created");
    let store = FsArtifactStore::new(StoragePaths::new(temp_dir.path().join(".liz")));
    let artifact = StoredArtifact {
        reference: ArtifactRef {
            id: ArtifactId::new("artifact_01"),
            thread_id: ThreadId::new("thread_01"),
            turn_id: TurnId::new("turn_01"),
            kind: ArtifactKind::Diff,
            summary: "Patch preview".to_owned(),
            locator: ".liz/artifacts/artifact_01.json".to_owned(),
            created_at: Timestamp::new("2026-04-13T21:04:00Z"),
        },
        body: "{\"diff\":\"+hello\"}".to_owned(),
    };

    store.put_artifact(&artifact).expect("artifact should be written");

    let loaded = store
        .get_artifact(&artifact.reference.id)
        .expect("artifact read should succeed")
        .expect("artifact should exist");

    assert_eq!(loaded, artifact);
}

/// Ensures checkpoints can be stored and recovered by identifier.
#[test]
fn checkpoint_store_round_trips_checkpoints() {
    let temp_dir = TempDir::new().expect("temp dir should be created");
    let store = FsCheckpointStore::new(StoragePaths::new(temp_dir.path().join(".liz")));
    let checkpoint = Checkpoint {
        id: CheckpointId::new("checkpoint_01"),
        thread_id: ThreadId::new("thread_01"),
        turn_id: TurnId::new("turn_01"),
        scope: CheckpointScope::ConversationAndWorkspace,
        reason: "Before write_text".to_owned(),
        created_at: Timestamp::new("2026-04-13T21:05:00Z"),
    };

    store.put_checkpoint(&checkpoint).expect("checkpoint should be written");

    let loaded = store
        .get_checkpoint(&checkpoint.id)
        .expect("checkpoint read should succeed")
        .expect("checkpoint should exist");

    assert_eq!(loaded, checkpoint);
}
