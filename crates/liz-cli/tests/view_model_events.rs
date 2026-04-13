//! Event-driven CLI view-model coverage.

use liz_cli::view_model::ViewModel;
use liz_protocol::events::{
    ThreadStartedEvent, TurnCancelledEvent, TurnStartedEvent,
};
use liz_protocol::{
    EventId, ServerEvent, ServerEventPayload, Thread, ThreadId, ThreadStatus, Timestamp, Turn,
    TurnId, TurnKind, TurnStatus,
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

    assert_eq!(
        view_model.thread_statuses.get(&ThreadId::new("thread_01")),
        Some(&ThreadStatus::Active)
    );
    assert_eq!(view_model.transcript_lines.len(), 2);
    assert!(view_model.transcript_lines[0].contains("turn started"));
    assert!(view_model.transcript_lines[1].contains("turn interrupted"));
}
