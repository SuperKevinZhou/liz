//! Request handling for app-server protocol messages.

use crate::events::PendingEvent;
use crate::executor::NodeExecutorRouter;
use crate::runtime::{RuntimeCoordinator, RuntimeError};
use liz_protocol::events::{
    ApprovalResolvedEvent, ArtifactCreatedEvent, DiffAvailableEvent, ExecutorOutputChunkEvent,
    MemoryCompilationAppliedEvent, MemoryInvalidationAppliedEvent, MemoryWakeupLoadedEvent,
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
    executor: &NodeExecutorRouter,
    envelope: ClientRequestEnvelope,
) -> HandledRequest {
    let ClientRequestEnvelope { request_id, request } = envelope;

    let response = match request {
        ClientRequest::OpenAiCodexOAuthStart(request) => runtime
            .start_openai_codex_oauth_login(request)
            .map(|response| (ResponsePayload::OpenAiCodexOAuthStart(response), Vec::new())),
        ClientRequest::OpenAiCodexOAuthComplete(request) => runtime
            .complete_openai_codex_oauth_login(request)
            .map(|response| (ResponsePayload::OpenAiCodexOAuthComplete(response), Vec::new())),
        ClientRequest::GitLabOAuthStart(request) => runtime
            .start_gitlab_oauth_login(request)
            .map(|response| (ResponsePayload::GitLabOAuthStart(response), Vec::new())),
        ClientRequest::GitLabOAuthComplete(request) => runtime
            .complete_gitlab_oauth_login(request)
            .map(|response| (ResponsePayload::GitLabOAuthComplete(response), Vec::new())),
        ClientRequest::GitLabPatSave(request) => runtime
            .save_gitlab_pat(request)
            .map(|response| (ResponsePayload::GitLabPatSave(response), Vec::new())),
        ClientRequest::GitHubCopilotDeviceStart(request) => runtime
            .start_github_copilot_device_login(request)
            .map(|response| (ResponsePayload::GitHubCopilotDeviceStart(response), Vec::new())),
        ClientRequest::GitHubCopilotDevicePoll(request) => runtime
            .poll_github_copilot_device_login(request)
            .map(|response| (ResponsePayload::GitHubCopilotDevicePoll(response), Vec::new())),
        ClientRequest::MiniMaxOAuthStart(request) => runtime
            .start_minimax_oauth_login(request)
            .map(|response| (ResponsePayload::MiniMaxOAuthStart(response), Vec::new())),
        ClientRequest::MiniMaxOAuthPoll(request) => runtime
            .poll_minimax_oauth_login(request)
            .map(|response| (ResponsePayload::MiniMaxOAuthPoll(response), Vec::new())),
        ClientRequest::ProviderAuthList(request) => runtime
            .list_provider_auth_profiles(request)
            .map(|response| (ResponsePayload::ProviderAuthList(response), Vec::new())),
        ClientRequest::ModelStatus(_) => Err(RuntimeError::unsupported(
            "model_status_requires_server",
            "model/status must be handled by the app server",
        )),
        ClientRequest::RuntimeConfigGet(_) | ClientRequest::RuntimeConfigUpdate(_) => {
            Err(RuntimeError::unsupported(
                "runtime_config_requires_server",
                "runtime/config must be handled by the app server",
            ))
        }
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
            let mut events = vec![PendingEvent::new(
                thread.id.clone(),
                None,
                ServerEventPayload::ThreadResumed(ThreadResumedEvent { thread: thread.clone() }),
            )];
            if let Ok(memory) = runtime.read_memory_wakeup(liz_protocol::MemoryReadWakeupRequest {
                thread_id: thread.id.clone(),
            }) {
                events.push(PendingEvent::new(
                    thread.id.clone(),
                    None,
                    ServerEventPayload::MemoryWakeupLoaded(MemoryWakeupLoadedEvent {
                        wakeup: memory.wakeup,
                    }),
                ));
            }
            (ResponsePayload::ThreadResume(response), events)
        }),
        ClientRequest::ThreadList(request) => runtime
            .list_threads(request)
            .map(|response| (ResponsePayload::ThreadList(response), Vec::new())),
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
        ClientRequest::ToolCall(request) => {
            executor.execute_tool(&request).and_then(|node_execution| {
                let node_id = node_execution.node_id.clone();
                let executed = node_execution.executed;
                let executor_task_id =
                    executor_task_id_for_tool(&request.thread_id, &executed.result);
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
                            node_id: Some(node_id.clone()),
                            workspace_mount_id: request.workspace_mount_id.clone(),
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
                        node_id: Some(node_id),
                        workspace_mount_id: request.workspace_mount_id.clone(),
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
            })
        }
        ClientRequest::ThreadRollback(_) => Err(RuntimeError::unsupported(
            "rollback_not_ready",
            "rollback handling is implemented in a later phase",
        )),
        ClientRequest::MemoryReadWakeup(request) => {
            runtime.read_memory_wakeup(request).map(|response| {
                let thread_id = response.thread_id.clone();
                let wakeup = response.wakeup.clone();
                (
                    ResponsePayload::MemoryReadWakeup(response),
                    vec![PendingEvent::new(
                        thread_id,
                        None,
                        ServerEventPayload::MemoryWakeupLoaded(MemoryWakeupLoadedEvent { wakeup }),
                    )],
                )
            })
        }
        ClientRequest::MemoryCompileNow(request) => {
            runtime.compile_memory_now(request).map(|response| {
                let thread_id = response.thread_id.clone();
                let mut events = vec![PendingEvent::new(
                    thread_id.clone(),
                    None,
                    ServerEventPayload::MemoryCompilationApplied(MemoryCompilationAppliedEvent {
                        compilation: response.compilation.clone(),
                    }),
                )];
                if !response.compilation.invalidated_fact_ids.is_empty() {
                    events.push(PendingEvent::new(
                        thread_id,
                        None,
                        ServerEventPayload::MemoryInvalidationApplied(
                            MemoryInvalidationAppliedEvent {
                                compilation: response.compilation.clone(),
                            },
                        ),
                    ));
                }
                (ResponsePayload::MemoryCompileNow(response), events)
            })
        }
        ClientRequest::MemoryListTopics(request) => runtime
            .list_memory_topics(request)
            .map(|response| (ResponsePayload::MemoryListTopics(response), Vec::new())),
        ClientRequest::MemorySearch(request) => runtime
            .search_memory(request)
            .map(|response| (ResponsePayload::MemorySearch(response), Vec::new())),
        ClientRequest::MemoryOpenSession(request) => runtime
            .open_memory_session(request)
            .map(|response| (ResponsePayload::MemoryOpenSession(response), Vec::new())),
        ClientRequest::MemoryOpenEvidence(request) => runtime
            .open_memory_evidence(request)
            .map(|response| (ResponsePayload::MemoryOpenEvidence(response), Vec::new())),
        ClientRequest::MemorySurfaceAboutYouRead(request) => runtime
            .read_about_you_surface(request)
            .map(|response| (ResponsePayload::MemorySurfaceAboutYouRead(response), Vec::new())),
        ClientRequest::MemorySurfaceAboutYouUpdate(request) => runtime
            .update_about_you_surface(request)
            .map(|response| (ResponsePayload::MemorySurfaceAboutYouUpdate(response), Vec::new())),
        ClientRequest::MemorySurfaceCarryingRead(request) => runtime
            .read_carrying_surface(request)
            .map(|response| (ResponsePayload::MemorySurfaceCarryingRead(response), Vec::new())),
        ClientRequest::MemorySurfaceKnowledgeList(request) => runtime
            .list_knowledge_surface(request)
            .map(|response| (ResponsePayload::MemorySurfaceKnowledgeList(response), Vec::new())),
        ClientRequest::MemorySurfaceKnowledgeCorrect(request) => runtime
            .correct_knowledge_surface(request)
            .map(|response| (ResponsePayload::MemorySurfaceKnowledgeCorrect(response), Vec::new())),
        ClientRequest::NodeList(request) => runtime
            .list_nodes(request)
            .map(|response| (ResponsePayload::NodeList(response), Vec::new())),
        ClientRequest::NodeRead(request) => runtime
            .read_node(request)
            .map(|response| (ResponsePayload::NodeRead(response), Vec::new())),
        ClientRequest::NodeUpdatePolicy(request) => runtime
            .update_node_policy(request)
            .map(|response| (ResponsePayload::NodeUpdatePolicy(response), Vec::new())),
        ClientRequest::WorkspaceMountList(request) => runtime
            .list_workspace_mounts(request)
            .map(|response| (ResponsePayload::WorkspaceMountList(response), Vec::new())),
        ClientRequest::WorkspaceMountAttach(request) => runtime
            .attach_workspace_mount(request)
            .map(|response| (ResponsePayload::WorkspaceMountAttach(response), Vec::new())),
        ClientRequest::WorkspaceMountDetach(request) => runtime
            .detach_workspace_mount(request)
            .map(|response| (ResponsePayload::WorkspaceMountDetach(response), Vec::new())),
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
