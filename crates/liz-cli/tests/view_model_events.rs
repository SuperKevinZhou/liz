//! Event-driven CLI view-model coverage.

use liz_cli::view_model::ViewModel;
use liz_protocol::events::{
    MemoryCompilationAppliedEvent, MemoryWakeupLoadedEvent, ThreadStartedEvent, TurnCancelledEvent,
    TurnStartedEvent,
};
use liz_protocol::{
    EventId, MemoryCompilationSummary, MemoryWakeup, ServerEvent, ServerEventPayload, Thread,
    ThreadId, ThreadStatus, Timestamp, Turn, TurnId, TurnKind, TurnStatus,
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

    assert_eq!(
        view_model.thread_statuses.get(&ThreadId::new("thread_01")),
        Some(&ThreadStatus::Active)
    );
    assert_eq!(view_model.transcript_lines.len(), 4);
    assert!(view_model.transcript_lines[0].contains("turn started"));
    assert!(view_model.transcript_lines[1].contains("turn interrupted"));
    assert!(view_model.transcript_lines[2].contains("wake-up loaded"));
    assert!(view_model.transcript_lines[3].contains("memory compiled"));
}
