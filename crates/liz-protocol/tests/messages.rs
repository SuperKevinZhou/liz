//! Serialization coverage for request, response, and event envelopes.

use liz_protocol::{
    ApprovalDecision, ApprovalId, ApprovalRequest, ApprovalRespondRequest, ApprovalRespondResponse,
    ApprovalStatus, ClientRequest, ClientRequestEnvelope, EventId, MemoryCompilationAppliedEvent,
    MemoryCompilationSummary, RequestId, ResponsePayload, RiskLevel, ServerEvent,
    ServerEventPayload, ServerResponseEnvelope, SuccessResponseEnvelope, Thread, ThreadId,
    ThreadStartRequest, ThreadStartResponse, ThreadStartedEvent, ThreadStatus, Timestamp, TurnId,
};

/// Ensures request envelopes serialize with the expected method names.
#[test]
fn client_request_envelope_serializes_method_names() {
    let request = ClientRequestEnvelope {
        request_id: RequestId::new("req_01"),
        request: ClientRequest::ThreadStart(ThreadStartRequest {
            title: Some("Bootstrap".to_owned()),
            initial_goal: Some("Start liz".to_owned()),
            workspace_ref: Some("D:/zzh/Code/liz/liz".to_owned()),
        }),
    };

    let value = serde_json::to_value(&request).expect("request should serialize");

    assert_eq!(value["request_id"], "req_01");
    assert_eq!(value["method"], "thread/start");
    assert_eq!(value["params"]["title"], "Bootstrap");
}

/// Ensures success responses serialize with a method-specific payload.
#[test]
fn success_response_envelope_round_trips() {
    let thread = Thread {
        id: ThreadId::new("thread_01"),
        title: "Bootstrap".to_owned(),
        status: ThreadStatus::Active,
        created_at: Timestamp::new("2026-04-13T20:00:00Z"),
        updated_at: Timestamp::new("2026-04-13T20:01:00Z"),
        active_goal: Some("Start liz".to_owned()),
        active_summary: Some("Ready for the first turn".to_owned()),
        last_interruption: None,
        pending_commitments: vec![],
        latest_turn_id: None,
        latest_checkpoint_id: None,
        parent_thread_id: None,
    };
    let response = ServerResponseEnvelope::Success(Box::new(SuccessResponseEnvelope {
        ok: true,
        request_id: RequestId::new("req_01"),
        response: ResponsePayload::ThreadStart(ThreadStartResponse { thread }),
    }));

    let value = serde_json::to_value(&response).expect("response should serialize");

    assert_eq!(value["ok"], true);
    assert_eq!(value["method"], "thread/start");

    let round_trip: ServerResponseEnvelope =
        serde_json::from_value(value).expect("response should deserialize");

    assert_eq!(round_trip, response);
}

/// Ensures error responses remain machine-readable.
#[test]
fn error_response_envelope_serializes_error_payload() {
    let response = ServerResponseEnvelope::Error(liz_protocol::ErrorResponseEnvelope {
        ok: false,
        request_id: RequestId::new("req_02"),
        error: liz_protocol::ResponseError {
            code: "thread_not_found".to_owned(),
            message: "Thread thread_missing does not exist".to_owned(),
            retryable: false,
        },
    });

    let value = serde_json::to_value(&response).expect("error response should serialize");

    assert_eq!(value["ok"], false);
    assert_eq!(value["error"]["code"], "thread_not_found");
}

/// Ensures event envelopes serialize with the expected event type names.
#[test]
fn server_event_envelope_round_trips() {
    let event = ServerEvent {
        event_id: EventId::new("event_01"),
        thread_id: ThreadId::new("thread_01"),
        turn_id: Some(TurnId::new("turn_01")),
        created_at: Timestamp::new("2026-04-13T20:02:00Z"),
        payload: ServerEventPayload::ThreadStarted(ThreadStartedEvent {
            thread: Thread {
                id: ThreadId::new("thread_01"),
                title: "Bootstrap".to_owned(),
                status: ThreadStatus::Active,
                created_at: Timestamp::new("2026-04-13T20:00:00Z"),
                updated_at: Timestamp::new("2026-04-13T20:02:00Z"),
                active_goal: Some("Start liz".to_owned()),
                active_summary: Some("First turn started".to_owned()),
                last_interruption: None,
                pending_commitments: vec!["Add protocol events".to_owned()],
                latest_turn_id: Some(TurnId::new("turn_01")),
                latest_checkpoint_id: None,
                parent_thread_id: None,
            },
        }),
    };

    let value = serde_json::to_value(&event).expect("event should serialize");

    assert_eq!(value["event_type"], "thread_started");

    let round_trip: ServerEvent = serde_json::from_value(value).expect("event should deserialize");

    assert_eq!(round_trip, event);
}

/// Ensures approval requests and memory events serialize with their typed payloads.
#[test]
fn protocol_messages_cover_approval_and_memory_shapes() {
    let approval_response = ServerResponseEnvelope::Success(Box::new(SuccessResponseEnvelope {
        ok: true,
        request_id: RequestId::new("req_approval"),
        response: ResponsePayload::ApprovalRespond(ApprovalRespondResponse {
            approval: ApprovalRequest {
                id: ApprovalId::new("approval_01"),
                thread_id: ThreadId::new("thread_01"),
                turn_id: TurnId::new("turn_01"),
                action_type: "shell.exec".to_owned(),
                risk_level: RiskLevel::High,
                reason: "High-risk shell command".to_owned(),
                sandbox_context: Some("workspace-write".to_owned()),
                status: ApprovalStatus::Approved,
            },
        }),
    }));
    let approval_request = ClientRequestEnvelope {
        request_id: RequestId::new("req_approval"),
        request: ClientRequest::ApprovalRespond(ApprovalRespondRequest {
            approval_id: ApprovalId::new("approval_01"),
            decision: ApprovalDecision::ApproveOnce,
        }),
    };
    let memory_event = ServerEvent {
        event_id: EventId::new("event_memory"),
        thread_id: ThreadId::new("thread_01"),
        turn_id: Some(TurnId::new("turn_01")),
        created_at: Timestamp::new("2026-04-13T20:03:00Z"),
        payload: ServerEventPayload::MemoryCompilationApplied(MemoryCompilationAppliedEvent {
            compilation: MemoryCompilationSummary {
                delta_summary: "Updated active summary".to_owned(),
                updated_fact_ids: vec![],
                invalidated_fact_ids: vec![],
            },
        }),
    };

    let approval_request_value =
        serde_json::to_value(&approval_request).expect("approval request should serialize");
    let approval_response_value =
        serde_json::to_value(&approval_response).expect("approval response should serialize");
    let memory_event_value =
        serde_json::to_value(&memory_event).expect("memory event should serialize");

    assert_eq!(approval_request_value["method"], "approval/respond");
    assert_eq!(approval_response_value["method"], "approval/respond");
    assert_eq!(memory_event_value["event_type"], "memory_compilation_applied");
}
