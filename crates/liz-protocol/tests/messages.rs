//! Serialization coverage for request, response, and event envelopes.

use liz_protocol::{
    ApprovalDecision, ApprovalId, ApprovalRequest, ApprovalRespondRequest, ApprovalRespondResponse,
    ApprovalStatus, ClientRequest, ClientRequestEnvelope, ClientTransportMessage, EventId,
    MemoryCompilationAppliedEvent, MemoryCompilationSummary, RequestId, ResponsePayload, RiskLevel,
    ServerEvent, ServerEventPayload, ServerResponseEnvelope, ServerTransportMessage,
    SuccessResponseEnvelope, Thread, ThreadId, ThreadStartRequest, ThreadStartResponse,
    ThreadStartedEvent, ThreadStatus, Timestamp, ToolCallRequest, ToolCallResponse, ToolInvocation,
    ToolResult, TurnId, WorkspaceReadRequest, WorkspaceReadResult,
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
        workspace_ref: Some("D:/zzh/Code/liz/liz".to_owned()),
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

/// Ensures model status messages keep a stable wire shape for CLI startup diagnostics.
#[test]
fn model_status_messages_round_trip() {
    let request = ClientRequestEnvelope {
        request_id: RequestId::new("req_model_status"),
        request: ClientRequest::ModelStatus(liz_protocol::ModelStatusRequest {}),
    };
    let response = ServerResponseEnvelope::Success(Box::new(SuccessResponseEnvelope {
        ok: true,
        request_id: RequestId::new("req_model_status"),
        response: ResponsePayload::ModelStatus(liz_protocol::ModelStatusResponse {
            provider_id: "openai".to_owned(),
            display_name: Some("OpenAI".to_owned()),
            model_id: Some("gpt-5.4".to_owned()),
            auth_kind: Some("api-key".to_owned()),
            ready: false,
            credential_configured: false,
            credential_hints: vec!["OPENAI_API_KEY".to_owned()],
            notes: vec!["Configure credentials with OPENAI_API_KEY".to_owned()],
        }),
    }));

    let request_value = serde_json::to_value(&request).expect("request should serialize");
    let response_value = serde_json::to_value(&response).expect("response should serialize");

    assert_eq!(request_value["method"], "model/status");
    assert_eq!(response_value["method"], "model/status");
    assert_eq!(response_value["data"]["provider_id"], "openai");

    let request_round_trip: ClientRequestEnvelope =
        serde_json::from_value(request_value).expect("request should deserialize");
    let response_round_trip: ServerResponseEnvelope =
        serde_json::from_value(response_value).expect("response should deserialize");

    assert_eq!(request_round_trip, request);
    assert_eq!(response_round_trip, response);
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
                workspace_ref: Some("D:/zzh/Code/liz/liz".to_owned()),
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
                recent_topics: vec!["runtime".to_owned()],
                recent_keywords: vec!["summary".to_owned()],
                candidate_procedures: vec![],
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

/// Ensures websocket transport frames distinguish requests, responses, and events.
#[test]
fn transport_messages_round_trip_through_json() {
    let request = ClientTransportMessage::request(ClientRequestEnvelope {
        request_id: RequestId::new("req_transport"),
        request: ClientRequest::ThreadStart(ThreadStartRequest {
            title: Some("Transport bootstrap".to_owned()),
            initial_goal: Some("Exercise websocket frames".to_owned()),
            workspace_ref: None,
        }),
    });
    let response = ServerTransportMessage::response(ServerResponseEnvelope::Success(Box::new(
        SuccessResponseEnvelope {
            ok: true,
            request_id: RequestId::new("req_transport"),
            response: ResponsePayload::ThreadStart(ThreadStartResponse {
                thread: Thread {
                    id: ThreadId::new("thread_transport"),
                    title: "Transport bootstrap".to_owned(),
                    status: ThreadStatus::Active,
                    created_at: Timestamp::new("2026-04-16T18:00:00Z"),
                    updated_at: Timestamp::new("2026-04-16T18:00:00Z"),
                    active_goal: Some("Exercise websocket frames".to_owned()),
                    active_summary: Some("Transport ready".to_owned()),
                    last_interruption: None,
                    workspace_ref: None,
                    pending_commitments: Vec::new(),
                    latest_turn_id: None,
                    latest_checkpoint_id: None,
                    parent_thread_id: None,
                },
            }),
        },
    )));
    let event = ServerTransportMessage::event(ServerEvent {
        event_id: EventId::new("event_transport"),
        thread_id: ThreadId::new("thread_transport"),
        turn_id: None,
        created_at: Timestamp::new("2026-04-16T18:00:01Z"),
        payload: ServerEventPayload::ThreadStarted(ThreadStartedEvent {
            thread: Thread {
                id: ThreadId::new("thread_transport"),
                title: "Transport bootstrap".to_owned(),
                status: ThreadStatus::Active,
                created_at: Timestamp::new("2026-04-16T18:00:00Z"),
                updated_at: Timestamp::new("2026-04-16T18:00:01Z"),
                active_goal: Some("Exercise websocket frames".to_owned()),
                active_summary: Some("Transport ready".to_owned()),
                last_interruption: None,
                workspace_ref: None,
                pending_commitments: Vec::new(),
                latest_turn_id: None,
                latest_checkpoint_id: None,
                parent_thread_id: None,
            },
        }),
    });

    let request_value = serde_json::to_value(&request).expect("request frame should serialize");
    let response_value = serde_json::to_value(&response).expect("response frame should serialize");
    let event_value = serde_json::to_value(&event).expect("event frame should serialize");

    assert_eq!(request_value["kind"], "request");
    assert_eq!(response_value["kind"], "response");
    assert_eq!(event_value["kind"], "event");

    let request_round_trip: ClientTransportMessage =
        serde_json::from_value(request_value).expect("request frame should deserialize");
    let response_round_trip: ServerTransportMessage =
        serde_json::from_value(response_value).expect("response frame should deserialize");
    let event_round_trip: ServerTransportMessage =
        serde_json::from_value(event_value).expect("event frame should deserialize");

    assert_eq!(request_round_trip, request);
    assert_eq!(response_round_trip, response);
    assert_eq!(event_round_trip, event);
}

/// Ensures tool-call requests and responses keep their stable wire shape.
#[test]
fn tool_call_messages_round_trip() {
    let request = ClientRequestEnvelope {
        request_id: RequestId::new("req_tool"),
        request: ClientRequest::ToolCall(ToolCallRequest {
            thread_id: ThreadId::new("thread_01"),
            turn_id: Some(TurnId::new("turn_04")),
            invocation: ToolInvocation::WorkspaceRead(WorkspaceReadRequest {
                path: "crates/liz-protocol/src/lib.rs".to_owned(),
                start_line: Some(1),
                end_line: Some(5),
            }),
        }),
    };
    let response = ServerResponseEnvelope::Success(Box::new(SuccessResponseEnvelope {
        ok: true,
        request_id: RequestId::new("req_tool"),
        response: ResponsePayload::ToolCall(ToolCallResponse {
            execution_turn_id: TurnId::new("turn_04"),
            summary: "Read 5 lines from crates/liz-protocol/src/lib.rs".to_owned(),
            result: ToolResult::WorkspaceRead(WorkspaceReadResult {
                path: "crates/liz-protocol/src/lib.rs".to_owned(),
                content: "//! Shared request surface".to_owned(),
                start_line: 1,
                end_line: 5,
                total_lines: 42,
            }),
            artifact_refs: Vec::new(),
        }),
    }));

    let request_value = serde_json::to_value(&request).expect("tool request should serialize");
    let response_value = serde_json::to_value(&response).expect("tool response should serialize");

    assert_eq!(request_value["method"], "tool/call");
    assert_eq!(request_value["params"]["tool_name"], "workspace.read");
    assert_eq!(response_value["method"], "tool/call");
    assert_eq!(response_value["data"]["tool_name"], "workspace.read");

    let request_round_trip: ClientRequestEnvelope =
        serde_json::from_value(request_value).expect("tool request should deserialize");
    let response_round_trip: ServerResponseEnvelope =
        serde_json::from_value(response_value).expect("tool response should deserialize");

    assert_eq!(request_round_trip, request);
    assert_eq!(response_round_trip, response);
}
