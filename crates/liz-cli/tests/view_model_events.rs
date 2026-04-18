//! Event-driven CLI view-model coverage.

use liz_cli::view_model::{TranscriptEntryKind, ViewModel};
use liz_protocol::events::{
    MemoryCompilationAppliedEvent, MemoryDreamingCompletedEvent, MemoryWakeupLoadedEvent,
    ThreadStartedEvent, TurnCancelledEvent, TurnStartedEvent,
};
use liz_protocol::{
    EventId, MemoryCompilationSummary, MemoryOpenSessionResponse, MemorySessionEntry,
    MemorySessionView, MemoryWakeup, ModelStatusResponse, ResponsePayload, ServerEvent,
    ServerEventPayload, ServerResponseEnvelope, SuccessResponseEnvelope, Thread, ThreadId,
    ThreadStatus, Timestamp, Turn, TurnId, TurnKind, TurnStatus,
};

#[test]
fn view_model_projects_thread_and_turn_events() {
    let thread_id = ThreadId::new("thread_01");
    let mut view_model = ViewModel::default();
    let thread = Thread {
        id: thread_id.clone(),
        title: "Phase 4".to_owned(),
        status: ThreadStatus::Active,
        created_at: Timestamp::new("2026-04-13T20:00:00Z"),
        updated_at: Timestamp::new("2026-04-13T20:00:00Z"),
        active_goal: Some("Project websocket events".to_owned()),
        active_summary: None,
        last_interruption: None,
        pending_commitments: Vec::new(),
        latest_turn_id: None,
        latest_checkpoint_id: None,
        parent_thread_id: None,
    };
    let turn = Turn {
        id: TurnId::new("turn_01"),
        thread_id: thread_id.clone(),
        kind: TurnKind::User,
        status: TurnStatus::Running,
        started_at: Timestamp::new("2026-04-13T20:00:01Z"),
        ended_at: None,
        goal: Some("Start event stream".to_owned()),
        summary: None,
        checkpoint_before: None,
        checkpoint_after: None,
    };

    view_model.apply_event(&ServerEvent {
        event_id: EventId::new("event_01"),
        thread_id: thread_id.clone(),
        turn_id: None,
        created_at: Timestamp::new("2026-04-13T20:00:00Z"),
        payload: ServerEventPayload::ThreadStarted(ThreadStartedEvent { thread }),
    });
    view_model.apply_event(&ServerEvent {
        event_id: EventId::new("event_02"),
        thread_id: thread_id.clone(),
        turn_id: Some(turn.id.clone()),
        created_at: Timestamp::new("2026-04-13T20:00:01Z"),
        payload: ServerEventPayload::TurnStarted(TurnStartedEvent { turn: turn.clone() }),
    });
    view_model.apply_event(&ServerEvent {
        event_id: EventId::new("event_03"),
        thread_id,
        turn_id: Some(turn.id.clone()),
        created_at: Timestamp::new("2026-04-13T20:00:02Z"),
        payload: ServerEventPayload::TurnCancelled(TurnCancelledEvent { turn }),
    });
    view_model.apply_event(&ServerEvent {
        event_id: EventId::new("event_04"),
        thread_id: ThreadId::new("thread_01"),
        turn_id: None,
        created_at: Timestamp::new("2026-04-13T20:00:03Z"),
        payload: ServerEventPayload::MemoryWakeupLoaded(MemoryWakeupLoadedEvent {
            wakeup: MemoryWakeup {
                identity_summary: None,
                active_state: Some("Resume websocket event work".to_owned()),
                relevant_facts: vec![],
                open_commitments: vec!["Verify event stream".to_owned()],
                recent_topics: vec!["websocket".to_owned(), "events".to_owned()],
                recent_keywords: vec!["resume".to_owned()],
                citation_fact_ids: vec![],
                citations: vec![],
            },
        }),
    });
    view_model.apply_event(&ServerEvent {
        event_id: EventId::new("event_05"),
        thread_id: ThreadId::new("thread_01"),
        turn_id: None,
        created_at: Timestamp::new("2026-04-13T20:00:04Z"),
        payload: ServerEventPayload::MemoryCompilationApplied(MemoryCompilationAppliedEvent {
            compilation: MemoryCompilationSummary {
                delta_summary: "Compiled websocket wake-up facts".to_owned(),
                updated_fact_ids: vec![],
                invalidated_fact_ids: vec![],
                recent_topics: vec!["websocket".to_owned()],
                recent_keywords: vec!["events".to_owned()],
                candidate_procedures: vec![],
            },
        }),
    });
    view_model.apply_event(&ServerEvent {
        event_id: EventId::new("event_06"),
        thread_id: ThreadId::new("thread_01"),
        turn_id: None,
        created_at: Timestamp::new("2026-04-13T20:00:05Z"),
        payload: ServerEventPayload::MemoryDreamingCompleted(MemoryDreamingCompletedEvent {
            summary: "Reflection for Phase 4: active topics: websocket".to_owned(),
        }),
    });

    assert_eq!(
        view_model.thread_statuses.get(&ThreadId::new("thread_01")),
        Some(&ThreadStatus::Active)
    );
    assert_eq!(view_model.transcript_entries.len(), 3);
    assert_eq!(view_model.transcript_entries[0].kind, TranscriptEntryKind::User);
    assert!(view_model.transcript_entries[0].body.contains("Start event stream"));
    assert_eq!(view_model.transcript_entries[1].kind, TranscriptEntryKind::System);
    assert!(view_model.transcript_entries[1].body.contains("Turn interrupted"));
    assert_eq!(view_model.transcript_entries[2].kind, TranscriptEntryKind::System);
    assert!(view_model.transcript_entries[2].body.contains("Memory updated"));
    assert!(
        view_model.status_line.contains("Reflection") || view_model.status_line.contains("updated")
    );
    assert!(view_model.wakeup.is_some());
    assert_eq!(view_model.dreaming_summaries.len(), 1);
}

#[test]
fn view_model_projects_session_history_into_primary_transcript() {
    let thread_id = ThreadId::new("thread_history");
    let mut view_model = ViewModel::default();
    view_model.apply_event(&ServerEvent {
        event_id: EventId::new("event_history_thread"),
        thread_id: thread_id.clone(),
        turn_id: None,
        created_at: Timestamp::new("2026-04-18T00:00:00Z"),
        payload: ServerEventPayload::ThreadStarted(ThreadStartedEvent {
            thread: thread("thread_history", "History"),
        }),
    });

    view_model.apply_response(&ServerResponseEnvelope::Success(Box::new(
        SuccessResponseEnvelope {
            ok: true,
            request_id: liz_protocol::RequestId::new("response_history"),
            response: ResponsePayload::MemoryOpenSession(MemoryOpenSessionResponse {
                session: MemorySessionView {
                    thread_id,
                    title: "History".to_owned(),
                    status: ThreadStatus::Active,
                    active_summary: Some("Continue history".to_owned()),
                    pending_commitments: Vec::new(),
                    recent_entries: vec![
                        MemorySessionEntry {
                            recorded_at: Timestamp::new("2026-04-18T00:00:01Z"),
                            event: "turn_started".to_owned(),
                            summary: "Started turn for: explain the CLI".to_owned(),
                            turn_id: Some(TurnId::new("turn_01")),
                            artifact_ids: Vec::new(),
                        },
                        MemorySessionEntry {
                            recorded_at: Timestamp::new("2026-04-18T00:00:02Z"),
                            event: "turn_completed".to_owned(),
                            summary: "The CLI keeps a continuous transcript.".to_owned(),
                            turn_id: Some(TurnId::new("turn_01")),
                            artifact_ids: Vec::new(),
                        },
                    ],
                    artifacts: Vec::new(),
                },
            }),
        },
    )));

    assert_eq!(view_model.transcript_entries.len(), 2);
    assert_eq!(view_model.transcript_entries[0].kind, TranscriptEntryKind::User);
    assert_eq!(view_model.transcript_entries[0].body, "explain the CLI");
    assert_eq!(view_model.transcript_entries[1].kind, TranscriptEntryKind::Assistant);
}

#[test]
fn view_model_surfaces_missing_provider_configuration() {
    let mut view_model = ViewModel::default();
    view_model.apply_response(&ServerResponseEnvelope::Success(Box::new(
        SuccessResponseEnvelope {
            ok: true,
            request_id: liz_protocol::RequestId::new("response_model_status"),
            response: ResponsePayload::ModelStatus(ModelStatusResponse {
                provider_id: "openai".to_owned(),
                display_name: Some("OpenAI".to_owned()),
                model_id: Some("gpt-5.4".to_owned()),
                auth_kind: Some("api-key".to_owned()),
                ready: false,
                credential_configured: false,
                credential_hints: vec!["OPENAI_API_KEY".to_owned()],
                notes: vec!["Configure credentials with OPENAI_API_KEY".to_owned()],
            }),
        },
    )));

    assert_eq!(view_model.transcript_entries.len(), 1);
    assert_eq!(view_model.transcript_entries[0].kind, TranscriptEntryKind::System);
    assert!(view_model.transcript_entries[0].body.contains("Model provider is not ready"));
    assert!(view_model.transcript_entries[0].body.contains("OPENAI_API_KEY"));
}

fn thread(id: &str, title: &str) -> Thread {
    Thread {
        id: ThreadId::new(id),
        title: title.to_owned(),
        status: ThreadStatus::Active,
        created_at: Timestamp::new("2026-04-18T00:00:00Z"),
        updated_at: Timestamp::new("2026-04-18T00:00:00Z"),
        active_goal: Some(title.to_owned()),
        active_summary: None,
        last_interruption: None,
        pending_commitments: Vec::new(),
        latest_turn_id: None,
        latest_checkpoint_id: None,
        parent_thread_id: None,
    }
}
