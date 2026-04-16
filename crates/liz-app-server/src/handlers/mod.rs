//! Request handling for app-server protocol messages.

use crate::events::PendingEvent;
use crate::executor::ExecutorGateway;
use crate::runtime::{RuntimeCoordinator, RuntimeError};
use liz_protocol::events::{
    ApprovalResolvedEvent, ArtifactCreatedEvent, DiffAvailableEvent, ExecutorOutputChunkEvent,
    ThreadForkedEvent, ThreadInterruptedEvent, ThreadResumedEvent, ThreadStartedEvent,
    ThreadUpdatedEvent, ToolCompletedEvent, TurnCancelledEvent, TurnStartedEvent,
};
use liz_protocol::requests::ClientRequest;
use liz_protocol::responses::{
    ErrorResponseEnvelope, ResponseError, ResponsePayload, ServerResponseEnvelope,
    SuccessResponseEnvelope,
};
use liz_protocol::{ClientRequestEnvelope, ExecutorTaskId, RequestId, ServerEventPayload};

/// The fully handled result of a request, including any events that should be published.
#[derive(Debug)]
pub struct HandledRequest {
    /// The response that should be returned to the caller.
    pub response: ServerResponseEnvelope,
    /// The events that should be emitted as a consequence of the request.
    pub events: Vec<PendingEvent>,
}

/// Dispatches a typed request to the runtime coordinator.
pub fn handle_request(
    runtime: &mut RuntimeCoordinator,
    executor: &ExecutorGateway,
    envelope: ClientRequestEnvelope,
) -> HandledRequest {
    let ClientRequestEnvelope { request_id, request } = envelope;

    let response = match request {
        ClientRequest::ProviderAuthList(request) => runtime
            .list_provider_auth_profiles(request)
            .map(|response| (ResponsePayload::ProviderAuthList(response), Vec::new())),
        ClientRequest::ProviderAuthUpsert(request) => runtime
            .upsert_provider_auth_profile(request)
            .map(|response| (ResponsePayload::ProviderAuthUpsert(response), Vec::new())),
        ClientRequest::ProviderAuthDelete(request) => runtime
            .delete_provider_auth_profile(request)
            .map(|response| (ResponsePayload::ProviderAuthDelete(response), Vec::new())),
        ClientRequest::ThreadStart(request) => runtime.start_thread(request).map(|response| {
            let thread = response.thread.clone();
            (
                ResponsePayload::ThreadStart(response),
                vec![PendingEvent::new(
                    thread.id.clone(),
                    None,
                    ServerEventPayload::ThreadStarted(ThreadStartedEvent { thread }),
                )],
            )
        }),
        ClientRequest::ThreadResume(request) => runtime.resume_thread(request).map(|response| {
            let thread = response.thread.clone();
            (
                ResponsePayload::ThreadResume(response),
                vec![PendingEvent::new(
                    thread.id.clone(),
                    None,
                    ServerEventPayload::ThreadResumed(ThreadResumedEvent { thread }),
                )],
            )
        }),
        ClientRequest::ThreadFork(request) => runtime.fork_thread(request).map(|response| {
            let thread = response.thread.clone();
            (
                ResponsePayload::ThreadFork(response),
                vec![PendingEvent::new(
                    thread.id.clone(),
                    None,
                    ServerEventPayload::ThreadForked(ThreadForkedEvent { thread }),
                )],
            )
        }),
        ClientRequest::TurnStart(request) => runtime.start_turn(request).map(|response| {
            let turn = response.turn.clone();
            let mut events = vec![PendingEvent::new(
                turn.thread_id.clone(),
                Some(turn.id.clone()),
                ServerEventPayload::TurnStarted(TurnStartedEvent { turn }),
            )];
            if let Ok(Some(thread)) = runtime.read_thread(&response.turn.thread_id) {
                events.push(PendingEvent::new(
                    thread.id.clone(),
                    response.turn.id.clone().into(),
                    ServerEventPayload::ThreadUpdated(ThreadUpdatedEvent { thread }),
                ));
            }
            (ResponsePayload::TurnStart(response), events)
        }),
        ClientRequest::TurnCancel(request) => runtime.cancel_turn(request).map(|response| {
            let turn = response.turn.clone();
            let mut events = vec![PendingEvent::new(
                turn.thread_id.clone(),
                Some(turn.id.clone()),
                ServerEventPayload::TurnCancelled(TurnCancelledEvent { turn }),
            )];
            if let Ok(Some(thread)) = runtime.read_thread(&response.turn.thread_id) {
                events.push(PendingEvent::new(
                    thread.id.clone(),
                    Some(response.turn.id.clone()),
                    ServerEventPayload::ThreadInterrupted(ThreadInterruptedEvent { thread }),
                ));
            }
            (ResponsePayload::TurnCancel(response), events)
        }),
        ClientRequest::ApprovalRespond(request) => {
            let decision = request.decision;
            runtime.respond_approval(request).map(|response| {
                let approval = response.approval.clone();
                (
                    ResponsePayload::ApprovalRespond(response),
                    vec![PendingEvent::new(
                        approval.thread_id.clone(),
                        Some(approval.turn_id.clone()),
                        ServerEventPayload::ApprovalResolved(ApprovalResolvedEvent {
                            approval,
                            decision,
                        }),
                    )],
                )
            })
        }
        ClientRequest::ToolCall(request) => executor.execute_tool(&request).and_then(|executed| {
            let executor_task_id = executor_task_id_for_tool(&request.thread_id, &executed.result);
            let output_chunks = executed.output_chunks.clone();
            let artifacts = executed
                .artifacts
                .into_iter()
                .map(|artifact| (artifact.kind, artifact.summary, artifact.body))
                .collect::<Vec<_>>();
            let (execution_turn_id, artifact_refs) = runtime.record_tool_execution(
                &request.thread_id,
                request.turn_id.as_ref(),
                executed.tool_name.as_str(),
                &executed.summary,
                artifacts,
            )?;
            let mut events = artifact_refs
                .iter()
                .cloned()
                .flat_map(|artifact| {
                    let mut pending = vec![PendingEvent::new(
                        artifact.thread_id.clone(),
                        Some(artifact.turn_id.clone()),
                        ServerEventPayload::ArtifactCreated(ArtifactCreatedEvent {
                            artifact: artifact.clone(),
                        }),
                    )];
                    if matches!(artifact.kind, liz_protocol::ArtifactKind::Diff) {
                        pending.push(PendingEvent::new(
                            artifact.thread_id.clone(),
                            Some(artifact.turn_id.clone()),
                            ServerEventPayload::DiffAvailable(DiffAvailableEvent { artifact }),
                        ));
                    }
                    pending
                })
                .collect::<Vec<_>>();
            events.extend(output_chunks.into_iter().map(|chunk| {
                PendingEvent::new(
                    request.thread_id.clone(),
                    Some(execution_turn_id.clone()),
                    ServerEventPayload::ExecutorOutputChunk(ExecutorOutputChunkEvent {
                        executor_task_id: executor_task_id.clone(),
                        stream: chunk.stream,
                        chunk: chunk.chunk,
                    }),
                )
            }));
            events.push(PendingEvent::new(
                request.thread_id.clone(),
                Some(execution_turn_id.clone()),
                ServerEventPayload::ToolCompleted(ToolCompletedEvent {
                    tool_name: executed.tool_name.as_str().to_owned(),
                    summary: executed.summary.clone(),
                    artifact_ids: artifact_refs
                        .iter()
                        .map(|artifact| artifact.id.clone())
                        .collect(),
                }),
            ));
            Ok((
                ResponsePayload::ToolCall(liz_protocol::ToolCallResponse {
                    execution_turn_id,
                    summary: executed.summary,
                    result: executed.result,
                    artifact_refs,
                }),
                events,
            ))
        }),
        ClientRequest::ThreadRollback(_) => Err(RuntimeError::unsupported(
            "rollback_not_ready",
            "rollback handling is implemented in a later phase",
        )),
    };

    match response {
        Ok((response, events)) => HandledRequest {
            response: ServerResponseEnvelope::Success(Box::new(SuccessResponseEnvelope {
                ok: true,
                request_id,
                response,
            })),
            events,
        },
        Err(error) => HandledRequest {
            response: ServerResponseEnvelope::Error(error_envelope(request_id, error)),
            events: Vec::new(),
        },
    }
}

fn error_envelope(request_id: RequestId, error: RuntimeError) -> ErrorResponseEnvelope {
    ErrorResponseEnvelope {
        ok: false,
        request_id,
        error: ResponseError {
            code: error.code().to_owned(),
            message: error.to_string(),
            retryable: error.retryable(),
        },
    }
}

fn executor_task_id_for_tool(
    thread_id: &liz_protocol::ThreadId,
    result: &liz_protocol::ToolResult,
) -> ExecutorTaskId {
    match result {
        liz_protocol::ToolResult::ShellSpawn(result) => result.task_id.clone(),
        liz_protocol::ToolResult::ShellWait(result) => result.task_id.clone(),
        liz_protocol::ToolResult::ShellReadOutput(result) => result.task_id.clone(),
        liz_protocol::ToolResult::ShellTerminate(result) => result.task_id.clone(),
        _ => ExecutorTaskId::new(format!(
            "executor_{}_{}",
            thread_id,
            result.tool_name().as_str().replace('.', "_")
        )),
    }
}
