//! Transport-facing server façade.

mod websocket;

use crate::config::LizConfigFile;
use crate::events::EventBus;
use crate::executor::ExecutorGateway;
use crate::handlers;
use crate::model::{
    ModelGateway, ModelTurnRequest, NormalizedTurnEvent, ProviderOverride, ProviderToolCall,
    ToolResultInjection, ToolSurfaceMode,
};
use crate::runtime::RuntimeCoordinator;
use crate::storage::StoragePaths;
use liz_protocol::{
    ApprovalDecision, ApprovalPolicy, ApprovalRequestedEvent, ArtifactCreatedEvent,
    AssistantChunkEvent, AssistantCompletedEvent, CheckpointCreatedEvent, ClientRequestEnvelope,
    DiffAvailableEvent, ExecutorOutputChunkEvent, ExecutorTaskId, MemoryCompilationAppliedEvent,
    MemoryDreamingCompletedEvent, MemoryInvalidationAppliedEvent, ModelStatusResponse,
    ParticipantRef, ProviderAuthProfile, ProviderCredential, ServerEvent, ServerEventPayload,
    ServerResponseEnvelope, ThreadId, ToolCallRequest, ToolCompletedEvent, TurnCancelRequest,
    TurnCompletedEvent, TurnFailedEvent, TurnId,
};
use std::sync::mpsc::Receiver;

pub use websocket::{
    spawn_loopback_websocket, spawn_websocket_server, LoopbackWebSocketClient,
    WebSocketServerHandle, WebSocketTransportError,
};

/// Minimal server configuration used by the app server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerConfig {
    /// The bind address reserved for the future websocket server.
    pub bind_address: &'static str,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self { bind_address: "127.0.0.1:7777" }
    }
}

/// High-level server façade used by tests and future transports.
#[derive(Debug)]
pub struct AppServer {
    runtime: RuntimeCoordinator,
    executor: ExecutorGateway,
    event_bus: EventBus,
    model_gateway: ModelGateway,
    approval_policy: ApprovalPolicy,
}

impl AppServer {
    /// Creates a new app server rooted at the provided storage paths.
    pub fn new(paths: StoragePaths) -> Self {
        let model_gateway = model_gateway_from_paths(&paths);
        Self::new_with_model_gateway(paths, model_gateway)
    }

    /// Creates a new app server rooted at the provided storage paths and explicit model gateway.
    pub fn new_with_model_gateway(paths: StoragePaths, model_gateway: ModelGateway) -> Self {
        Self {
            runtime: RuntimeCoordinator::new(crate::runtime::RuntimeStores::new(paths)),
            executor: ExecutorGateway::default(),
            event_bus: EventBus::new(),
            model_gateway,
            approval_policy: ApprovalPolicy::OnRequest,
        }
    }

    /// Creates a new app server with simulated provider streaming for isolated tests.
    pub fn new_simulated(paths: StoragePaths) -> Self {
        Self::new_with_model_gateway(paths, ModelGateway::simulated())
    }

    /// Creates an app server using the default `.liz` storage layout.
    pub fn from_default_layout() -> Self {
        Self::new(StoragePaths::from_default_layout())
    }

    /// Handles a single protocol request and returns the matching response envelope.
    pub fn handle_request(&mut self, envelope: ClientRequestEnvelope) -> ServerResponseEnvelope {
        if matches!(envelope.request, liz_protocol::ClientRequest::ModelStatus(_)) {
            return ServerResponseEnvelope::Success(Box::new(
                liz_protocol::SuccessResponseEnvelope {
                    ok: true,
                    request_id: envelope.request_id,
                    response: liz_protocol::ResponsePayload::ModelStatus(self.model_status()),
                },
            ));
        }
        if matches!(envelope.request, liz_protocol::ClientRequest::RuntimeConfigGet(_)) {
            return ServerResponseEnvelope::Success(Box::new(
                liz_protocol::SuccessResponseEnvelope {
                    ok: true,
                    request_id: envelope.request_id,
                    response: liz_protocol::ResponsePayload::RuntimeConfig(self.runtime_config()),
                },
            ));
        }
        if let liz_protocol::ClientRequest::RuntimeConfigUpdate(request) = &envelope.request {
            if let Some(sandbox) = request.sandbox.clone() {
                self.executor.set_default_shell_sandbox(sandbox);
            }
            if let Some(approval_policy) = request.approval_policy {
                self.approval_policy = approval_policy;
            }
            return ServerResponseEnvelope::Success(Box::new(
                liz_protocol::SuccessResponseEnvelope {
                    ok: true,
                    request_id: envelope.request_id,
                    response: liz_protocol::ResponsePayload::RuntimeConfig(self.runtime_config()),
                },
            ));
        }

        let request = envelope.request.clone();
        let handled = handlers::handle_request(&mut self.runtime, &self.executor, envelope);
        self.event_bus.publish_all(handled.events);
        match request {
            liz_protocol::ClientRequest::TurnStart(request) => {
                self.continue_turn_after_policy(
                    &handled.response,
                    request.input,
                    request.participant,
                );
            }
            liz_protocol::ClientRequest::TurnCancel(request) => {
                self.compile_memory_after_boundary(&handled.response, &request.thread_id, None);
            }
            liz_protocol::ClientRequest::ApprovalRespond(request) => {
                self.continue_after_approval(&handled.response, request.decision);
            }
            _ => {}
        }
        handled.response
    }

    /// Subscribes to the server event stream.
    pub fn subscribe_events(&self) -> Receiver<ServerEvent> {
        self.event_bus.subscribe()
    }

    /// Returns a shared reference to the runtime coordinator for direct inspection in tests.
    pub fn runtime(&self) -> &RuntimeCoordinator {
        &self.runtime
    }

    /// Returns the effective model/provider readiness status used by CLI startup diagnostics.
    pub fn model_status(&self) -> ModelStatusResponse {
        model_status_from_gateway(&self.gateway_with_provider_auth_profiles())
    }

    /// Returns the effective runtime execution configuration.
    pub fn runtime_config(&self) -> liz_protocol::RuntimeConfigResponse {
        liz_protocol::RuntimeConfigResponse {
            sandbox: self.executor.default_shell_sandbox(),
            approval_policy: self.approval_policy,
        }
    }

    fn continue_turn_after_policy(
        &mut self,
        response: &ServerResponseEnvelope,
        input: String,
        participant: Option<ParticipantRef>,
    ) {
        let (thread, turn) = match response {
            ServerResponseEnvelope::Success(success) => match &success.response {
                liz_protocol::ResponsePayload::TurnStart(turn_response) => {
                    let Some(thread) = self
                        .runtime
                        .read_thread(&turn_response.turn.thread_id)
                        .ok()
                        .and_then(|thread| thread)
                    else {
                        return;
                    };
                    (thread, turn_response.turn.clone())
                }
                _ => return,
            },
            ServerResponseEnvelope::Error(_) => return,
        };

        let Ok(context) =
            self.runtime.assemble_context_for_participant(&thread.id, &input, participant.as_ref())
        else {
            return;
        };
        let decision = self.runtime.evaluate_policy(&input, &context);

        if self.approval_policy == ApprovalPolicy::OnRequest && decision.requires_approval {
            if let Ok((checkpoint, approval)) =
                self.runtime.require_approval_for_turn(&thread.id, &turn.id, &decision)
            {
                if let Some(checkpoint) = checkpoint {
                    self.event_bus.publish(crate::events::PendingEvent::new(
                        thread.id.clone(),
                        Some(turn.id.clone()),
                        ServerEventPayload::CheckpointCreated(CheckpointCreatedEvent {
                            checkpoint,
                        }),
                    ));
                }
                self.event_bus.publish(crate::events::PendingEvent::new(
                    thread.id.clone(),
                    Some(turn.id.clone()),
                    ServerEventPayload::ApprovalRequested(ApprovalRequestedEvent { approval }),
                ));
            }
            return;
        }

        self.stream_model_turn(
            thread,
            turn,
            context.system_prompt,
            context.developer_prompt,
            context.user_prompt,
        );
    }

    fn continue_after_approval(
        &mut self,
        response: &ServerResponseEnvelope,
        decision: ApprovalDecision,
    ) {
        let approval = match response {
            ServerResponseEnvelope::Success(success) => match &success.response {
                liz_protocol::ResponsePayload::ApprovalRespond(response) => {
                    response.approval.clone()
                }
                _ => return,
            },
            ServerResponseEnvelope::Error(_) => return,
        };

        match decision {
            ApprovalDecision::ApproveOnce | ApprovalDecision::ApproveAndPersist => {
                let Ok(turn) =
                    self.runtime.resume_approved_turn(&approval.thread_id, &approval.turn_id)
                else {
                    return;
                };
                let Some(thread) =
                    self.runtime.read_thread(&approval.thread_id).ok().and_then(|thread| thread)
                else {
                    return;
                };
                let input = turn.goal.clone().unwrap_or_default();
                let Ok(context) = self.runtime.assemble_context(&thread.id, &input) else {
                    return;
                };
                self.stream_model_turn(
                    thread,
                    turn,
                    context.system_prompt,
                    context.developer_prompt,
                    context.user_prompt,
                );
            }
            ApprovalDecision::Deny => {
                if let Ok(response) = self.runtime.cancel_turn(TurnCancelRequest {
                    thread_id: approval.thread_id.clone(),
                    turn_id: approval.turn_id.clone(),
                }) {
                    let turn = response.turn;
                    self.event_bus.publish(crate::events::PendingEvent::new(
                        approval.thread_id.clone(),
                        Some(turn.id.clone()),
                        ServerEventPayload::TurnCancelled(liz_protocol::TurnCancelledEvent {
                            turn: turn.clone(),
                        }),
                    ));
                    if let Ok(Some(thread)) = self.runtime.read_thread(&approval.thread_id) {
                        self.event_bus.publish(crate::events::PendingEvent::new(
                            thread.id.clone(),
                            Some(turn.id.clone()),
                            ServerEventPayload::ThreadInterrupted(
                                liz_protocol::ThreadInterruptedEvent { thread },
                            ),
                        ));
                    }
                    self.compile_memory_for_thread(&approval.thread_id, Some(&turn.id));
                }
            }
        }
    }

    fn stream_model_turn(
        &mut self,
        thread: liz_protocol::Thread,
        turn: liz_protocol::Turn,
        system_prompt: String,
        developer_prompt: String,
        user_prompt: String,
    ) {
        let thread_id = thread.id.clone();
        let turn_id = turn.id.clone();
        let model_gateway = self.gateway_with_provider_auth_profiles();
        let base_request = ModelTurnRequest::from_prompt_parts(
            thread.clone(),
            turn,
            system_prompt,
            developer_prompt,
            user_prompt,
        )
        .with_tool_surface_mode(if thread.workspace_ref.is_some() {
            ToolSurfaceMode::Standard
        } else {
            ToolSurfaceMode::ConversationOnly
        });
        let mut continuation_results = Vec::<ToolResultInjection>::new();
        let mut last_tool_fingerprint = String::new();
        let mut repeated_tool_rounds = 0_u32;

        loop {
            let request =
                base_request.clone().with_tool_result_injections(continuation_results.clone());
            let run_result = model_gateway
                .run_turn(request, |event| self.handle_model_event(&thread_id, &turn_id, event));
            let summary = match run_result {
                Ok(summary) => summary,
                Err(error) => {
                    if let Ok(turn) =
                        self.runtime.fail_turn(&thread_id, &turn_id, error.to_string())
                    {
                        self.event_bus.publish(crate::events::PendingEvent::new(
                            thread_id.clone(),
                            Some(turn.id.clone()),
                            ServerEventPayload::TurnFailed(TurnFailedEvent {
                                turn,
                                message: error.to_string(),
                            }),
                        ));
                        self.compile_memory_for_thread(&thread_id, Some(&turn_id));
                    }
                    return;
                }
            };

            if summary.tool_calls.is_empty() {
                let final_message =
                    summary.assistant_message.unwrap_or_else(|| "Completed turn".to_owned());
                if let Ok(turn) = self.runtime.complete_turn(&thread_id, &turn_id, final_message) {
                    self.event_bus.publish(crate::events::PendingEvent::new(
                        thread_id.clone(),
                        Some(turn.id.clone()),
                        ServerEventPayload::TurnCompleted(TurnCompletedEvent { turn }),
                    ));
                    self.compile_memory_for_thread(&thread_id, Some(&turn_id));
                }
                return;
            }

            let fingerprint = tool_call_fingerprint(&summary.tool_calls);
            if fingerprint == last_tool_fingerprint {
                repeated_tool_rounds = repeated_tool_rounds.saturating_add(1);
            } else {
                repeated_tool_rounds = 0;
                last_tool_fingerprint = fingerprint;
            }

            let mut round_results = summary
                .tool_calls
                .iter()
                .map(|call| self.execute_model_tool_call(&thread_id, &turn_id, call))
                .collect::<Vec<_>>();

            if repeated_tool_rounds >= 2 {
                round_results.push(runtime_diagnostic_injection(
                    "runtime.loop_diagnostic",
                    "Detected repeated tool-call pattern. Explain the blocker or choose a different action.",
                ));
            }

            continuation_results.extend(round_results);
        }
    }

    fn handle_model_event(
        &mut self,
        thread_id: &ThreadId,
        turn_id: &TurnId,
        event: NormalizedTurnEvent,
    ) {
        match event {
            NormalizedTurnEvent::AssistantDelta { chunk } => {
                self.event_bus.publish(crate::events::PendingEvent::new(
                    thread_id.clone(),
                    Some(turn_id.clone()),
                    ServerEventPayload::AssistantChunk(AssistantChunkEvent {
                        chunk,
                        stream_id: Some("primary".to_owned()),
                        is_final: false,
                    }),
                ));
            }
            NormalizedTurnEvent::AssistantMessage { message } => {
                self.event_bus.publish(crate::events::PendingEvent::new(
                    thread_id.clone(),
                    Some(turn_id.clone()),
                    ServerEventPayload::AssistantCompleted(AssistantCompletedEvent { message }),
                ));
            }
            NormalizedTurnEvent::ToolCallStarted { call_id, tool_name, summary } => {
                self.event_bus.publish(crate::events::PendingEvent::new(
                    thread_id.clone(),
                    Some(turn_id.clone()),
                    ServerEventPayload::ToolCallStarted(liz_protocol::ToolCallStartedEvent {
                        call_id,
                        tool_name,
                        summary,
                    }),
                ));
            }
            NormalizedTurnEvent::ToolCallDelta { call_id, tool_name, delta_summary, preview } => {
                self.event_bus.publish(crate::events::PendingEvent::new(
                    thread_id.clone(),
                    Some(turn_id.clone()),
                    ServerEventPayload::ToolCallUpdated(liz_protocol::ToolCallUpdatedEvent {
                        call_id,
                        tool_name,
                        delta_summary,
                        preview,
                    }),
                ));
            }
            NormalizedTurnEvent::ToolCallCommitted { call_id, tool_name, arguments } => {
                self.event_bus.publish(crate::events::PendingEvent::new(
                    thread_id.clone(),
                    Some(turn_id.clone()),
                    ServerEventPayload::ToolCallCommitted(liz_protocol::ToolCallCommittedEvent {
                        call_id,
                        tool_name: tool_name.clone(),
                        arguments_summary: arguments.clone(),
                        risk_hint: None,
                    }),
                ));
            }
            NormalizedTurnEvent::UsageDelta(_) | NormalizedTurnEvent::ProviderRawEvent { .. } => {}
        }
    }

    fn execute_model_tool_call(
        &mut self,
        thread_id: &ThreadId,
        turn_id: &TurnId,
        tool_call: &ProviderToolCall,
    ) -> ToolResultInjection {
        let invocation = match parse_tool_invocation(&tool_call.tool_name, &tool_call.arguments) {
            Ok(invocation) => invocation,
            Err(message) => {
                self.event_bus.publish(crate::events::PendingEvent::new(
                    thread_id.clone(),
                    Some(turn_id.clone()),
                    ServerEventPayload::ToolFailed(liz_protocol::ToolFailedEvent {
                        tool_name: tool_call.tool_name.clone(),
                        summary: message.clone(),
                    }),
                ));
                return ToolResultInjection {
                    call_id: tool_call.call_id.clone(),
                    tool_name: tool_call.tool_name.clone(),
                    provider_tool_name: tool_call.provider_tool_name.clone(),
                    result: serde_json::json!({ "error": message }),
                    is_error: true,
                    summary: "Tool invocation parse failed".to_owned(),
                };
            }
        };
        let request = ToolCallRequest {
            thread_id: thread_id.clone(),
            turn_id: Some(turn_id.clone()),
            invocation,
        };
        let executed = match self.executor.execute_tool(&request) {
            Ok(executed) => executed,
            Err(error) => {
                let summary = error.to_string();
                self.event_bus.publish(crate::events::PendingEvent::new(
                    thread_id.clone(),
                    Some(turn_id.clone()),
                    ServerEventPayload::ToolFailed(liz_protocol::ToolFailedEvent {
                        tool_name: tool_call.tool_name.clone(),
                        summary: summary.clone(),
                    }),
                ));
                return ToolResultInjection {
                    call_id: tool_call.call_id.clone(),
                    tool_name: tool_call.tool_name.clone(),
                    provider_tool_name: tool_call.provider_tool_name.clone(),
                    result: serde_json::json!({ "error": summary }),
                    is_error: true,
                    summary: "Tool execution failed".to_owned(),
                };
            }
        };
        let executor_task_id = executor_task_id_for_result(thread_id, &executed.result);
        let output_chunks = executed.output_chunks.clone();
        let artifacts = executed
            .artifacts
            .into_iter()
            .map(|artifact| (artifact.kind, artifact.summary, artifact.body))
            .collect::<Vec<_>>();
        let (execution_turn_id, artifact_refs) = match self.runtime.record_tool_execution(
            thread_id,
            Some(turn_id),
            executed.tool_name.as_str(),
            &executed.summary,
            artifacts,
        ) {
            Ok(values) => values,
            Err(error) => {
                let summary = error.to_string();
                self.event_bus.publish(crate::events::PendingEvent::new(
                    thread_id.clone(),
                    Some(turn_id.clone()),
                    ServerEventPayload::ToolFailed(liz_protocol::ToolFailedEvent {
                        tool_name: executed.tool_name.as_str().to_owned(),
                        summary: summary.clone(),
                    }),
                ));
                return ToolResultInjection {
                    call_id: tool_call.call_id.clone(),
                    tool_name: tool_call.tool_name.clone(),
                    provider_tool_name: tool_call.provider_tool_name.clone(),
                    result: serde_json::json!({ "error": summary }),
                    is_error: true,
                    summary: "Tool result recording failed".to_owned(),
                };
            }
        };

        for artifact in artifact_refs.iter().cloned() {
            self.event_bus.publish(crate::events::PendingEvent::new(
                artifact.thread_id.clone(),
                Some(artifact.turn_id.clone()),
                ServerEventPayload::ArtifactCreated(ArtifactCreatedEvent {
                    artifact: artifact.clone(),
                }),
            ));
            if matches!(artifact.kind, liz_protocol::ArtifactKind::Diff) {
                self.event_bus.publish(crate::events::PendingEvent::new(
                    artifact.thread_id.clone(),
                    Some(artifact.turn_id.clone()),
                    ServerEventPayload::DiffAvailable(DiffAvailableEvent { artifact }),
                ));
            }
        }

        for chunk in output_chunks {
            self.event_bus.publish(crate::events::PendingEvent::new(
                thread_id.clone(),
                Some(execution_turn_id.clone()),
                ServerEventPayload::ExecutorOutputChunk(ExecutorOutputChunkEvent {
                    executor_task_id: executor_task_id.clone(),
                    stream: chunk.stream,
                    chunk: chunk.chunk,
                }),
            ));
        }

        self.event_bus.publish(crate::events::PendingEvent::new(
            thread_id.clone(),
            Some(execution_turn_id),
            ServerEventPayload::ToolCompleted(ToolCompletedEvent {
                tool_name: executed.tool_name.as_str().to_owned(),
                summary: executed.summary,
                artifact_ids: artifact_refs.iter().map(|artifact| artifact.id.clone()).collect(),
            }),
        ));

        ToolResultInjection {
            call_id: tool_call.call_id.clone(),
            tool_name: tool_call.tool_name.clone(),
            provider_tool_name: tool_call.provider_tool_name.clone(),
            result: serde_json::to_value(&executed.result).unwrap_or_else(|_| {
                serde_json::json!({
                    "tool_name": executed.tool_name.as_str(),
                    "summary": "tool result serialization failed"
                })
            }),
            is_error: false,
            summary: format!("{} succeeded", executed.tool_name.as_str()),
        }
    }

    fn compile_memory_after_boundary(
        &mut self,
        response: &ServerResponseEnvelope,
        thread_id: &ThreadId,
        turn_id: Option<&TurnId>,
    ) {
        if matches!(response, ServerResponseEnvelope::Success(_)) {
            self.compile_memory_for_thread(thread_id, turn_id);
        }
    }

    fn compile_memory_for_thread(&mut self, thread_id: &ThreadId, turn_id: Option<&TurnId>) {
        let Ok(compilation) = self.runtime.compile_thread_memory(thread_id) else {
            return;
        };

        self.event_bus.publish(crate::events::PendingEvent::new(
            thread_id.clone(),
            turn_id.cloned(),
            ServerEventPayload::MemoryCompilationApplied(MemoryCompilationAppliedEvent {
                compilation: compilation.clone(),
            }),
        ));
        if !compilation.invalidated_fact_ids.is_empty() {
            self.event_bus.publish(crate::events::PendingEvent::new(
                thread_id.clone(),
                turn_id.cloned(),
                ServerEventPayload::MemoryInvalidationApplied(MemoryInvalidationAppliedEvent {
                    compilation,
                }),
            ));
        }
        if let Ok(Some(summary)) = self.runtime.summarize_thread_dreaming(thread_id) {
            self.event_bus.publish(crate::events::PendingEvent::new(
                thread_id.clone(),
                turn_id.cloned(),
                ServerEventPayload::MemoryDreamingCompleted(MemoryDreamingCompletedEvent {
                    summary,
                }),
            ));
        }
    }

    fn gateway_with_provider_auth_profiles(&self) -> ModelGateway {
        let mut gateway = self.model_gateway.clone();
        let Ok(profiles) = self.runtime.read_provider_auth_profiles() else {
            return gateway;
        };

        for profile in select_default_auth_profiles(profiles) {
            gateway = gateway.with_provider_override(
                profile.provider_id.clone(),
                provider_override_from_auth_profile(&profile),
            );
        }

        gateway
    }
}

fn tool_call_fingerprint(tool_calls: &[ProviderToolCall]) -> String {
    tool_calls
        .iter()
        .map(|call| format!("{}:{}", call.tool_name, call.arguments))
        .collect::<Vec<_>>()
        .join("|")
}

fn runtime_diagnostic_injection(tool_name: &str, message: &str) -> ToolResultInjection {
    let diagnostic_id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    ToolResultInjection {
        call_id: format!("diagnostic_{diagnostic_id}"),
        tool_name: tool_name.to_owned(),
        provider_tool_name: tool_name.to_owned(),
        result: serde_json::json!({ "diagnostic": message }),
        is_error: true,
        summary: "Runtime diagnostic".to_owned(),
    }
}

fn model_gateway_from_paths(paths: &StoragePaths) -> ModelGateway {
    let env_config = crate::model::ModelGatewayConfig::from_env();
    let file_config = LizConfigFile::load(paths);
    ModelGateway::from_config(file_config.into_gateway_config(env_config))
}

fn model_status_from_gateway(gateway: &ModelGateway) -> ModelStatusResponse {
    let provider_id = gateway.primary_provider_id().to_owned();
    match gateway.resolved_primary_provider() {
        Ok(provider) => {
            let credential_hints = credential_hints_for_spec(&provider.spec);
            let credential_configured = provider.api_key.is_some()
                || matches!(provider.spec.auth_kind, crate::model::ProviderAuthKind::Local)
                || provider.metadata.values().any(|value| !value.trim().is_empty());
            let mut notes =
                provider.spec.notes.iter().map(|note| (*note).to_owned()).collect::<Vec<_>>();
            if !credential_configured {
                notes.push(format!("Configure credentials with {}", credential_hints.join(" or ")));
            }
            ModelStatusResponse {
                provider_id,
                display_name: Some(provider.spec.display_name.to_owned()),
                model_id: Some(provider.model_id),
                auth_kind: Some(provider.spec.auth_kind.label().to_owned()),
                ready: credential_configured,
                credential_configured,
                credential_hints,
                notes,
            }
        }
        Err(error) => {
            let Some(spec) = gateway.provider_spec(&provider_id) else {
                return ModelStatusResponse {
                    provider_id,
                    display_name: None,
                    model_id: None,
                    auth_kind: None,
                    ready: false,
                    credential_configured: false,
                    credential_hints: Vec::new(),
                    notes: vec![error.to_string()],
                };
            };
            ModelStatusResponse {
                provider_id,
                display_name: Some(spec.display_name.to_owned()),
                model_id: Some(spec.default_model.to_owned()),
                auth_kind: Some(spec.auth_kind.label().to_owned()),
                ready: false,
                credential_configured: false,
                credential_hints: credential_hints_for_spec(spec),
                notes: vec![error.to_string()],
            }
        }
    }
}

fn credential_hints_for_spec(spec: &crate::model::ProviderSpec) -> Vec<String> {
    let mut hints = spec.credential_env_keys().into_iter().map(str::to_owned).collect::<Vec<_>>();
    for strategy in &spec.auth_strategies {
        for key in strategy.config_keys {
            let key = (*key).to_owned();
            if !hints.contains(&key) {
                hints.push(key);
            }
        }
    }
    hints
}

fn select_default_auth_profiles(profiles: Vec<ProviderAuthProfile>) -> Vec<ProviderAuthProfile> {
    use std::collections::BTreeMap;

    let mut grouped = BTreeMap::<String, Vec<ProviderAuthProfile>>::new();
    for profile in profiles {
        grouped.entry(profile.provider_id.clone()).or_default().push(profile);
    }

    grouped
        .into_iter()
        .filter_map(|(provider_id, mut profiles)| {
            profiles.sort_by(|left, right| left.profile_id.cmp(&right.profile_id));
            profiles
                .iter()
                .find(|profile| profile.profile_id == format!("{provider_id}:default"))
                .cloned()
                .or_else(|| profiles.into_iter().next())
        })
        .collect()
}

fn provider_override_from_auth_profile(profile: &ProviderAuthProfile) -> ProviderOverride {
    let mut override_config = ProviderOverride::default();
    match &profile.credential {
        ProviderCredential::ApiKey { api_key } => {
            override_config.api_key = Some(api_key.clone());
        }
        ProviderCredential::OAuth {
            access_token,
            refresh_token,
            expires_at_ms,
            account_id,
            email,
        } => {
            override_config.api_key = Some(access_token.clone());
            if let Some(refresh_token) = refresh_token {
                match profile.provider_id.as_str() {
                    "openai-codex" => {
                        override_config
                            .metadata
                            .insert("openai_codex.refresh_token".to_owned(), refresh_token.clone());
                        if let Some(expires_at_ms) = expires_at_ms {
                            override_config.metadata.insert(
                                "openai_codex.expires_at_ms".to_owned(),
                                expires_at_ms.to_string(),
                            );
                        }
                        if let Some(account_id) = account_id {
                            override_config
                                .metadata
                                .insert("openai_codex.account_id".to_owned(), account_id.clone());
                        }
                        if let Some(email) = email {
                            override_config
                                .metadata
                                .insert("openai_codex.email".to_owned(), email.clone());
                        }
                    }
                    "gitlab" => {
                        override_config
                            .metadata
                            .insert("gitlab.auth_mode".to_owned(), "oauth".to_owned());
                        override_config
                            .metadata
                            .insert("gitlab.oauth.refresh_token".to_owned(), refresh_token.clone());
                        if let Some(expires_at_ms) = expires_at_ms {
                            override_config.metadata.insert(
                                "gitlab.oauth.expires_at_ms".to_owned(),
                                expires_at_ms.to_string(),
                            );
                        }
                    }
                    _ => {
                        override_config
                            .metadata
                            .insert("oauth.refresh_token".to_owned(), refresh_token.clone());
                    }
                }
            }
        }
        ProviderCredential::Token { token, expires_at_ms, metadata } => {
            override_config.api_key = Some(token.clone());
            override_config.metadata.extend(metadata.clone());
            if let Some(expires_at_ms) = expires_at_ms {
                if profile.provider_id == "minimax-portal" {
                    override_config.metadata.insert(
                        "minimax.oauth.expires_at_ms".to_owned(),
                        expires_at_ms.to_string(),
                    );
                }
                override_config
                    .metadata
                    .insert("auth.expires_at_ms".to_owned(), expires_at_ms.to_string());
            }
        }
    }
    override_config
}

#[cfg(test)]
mod tests {
    use super::{provider_override_from_auth_profile, select_default_auth_profiles, AppServer};
    use crate::storage::StoragePaths;
    use liz_protocol::{
        ApprovalPolicy, ClientRequest, ClientRequestEnvelope, ProviderAuthProfile,
        ProviderCredential, RequestId, ResponsePayload, RuntimeConfigGetRequest,
        RuntimeConfigUpdateRequest, ServerEventPayload, ServerResponseEnvelope, ThreadStartRequest,
        TurnInputKind, TurnStartRequest,
    };
    use std::collections::BTreeMap;
    use std::time::Duration;
    use tempfile::TempDir;

    #[test]
    fn select_default_auth_profiles_prefers_provider_default_ids() {
        let profiles = vec![
            ProviderAuthProfile {
                profile_id: "github-copilot:work".to_owned(),
                provider_id: "github-copilot".to_owned(),
                display_name: Some("Work".to_owned()),
                credential: ProviderCredential::Token {
                    token: "token-work".to_owned(),
                    expires_at_ms: None,
                    metadata: BTreeMap::new(),
                },
            },
            ProviderAuthProfile {
                profile_id: "github-copilot:default".to_owned(),
                provider_id: "github-copilot".to_owned(),
                display_name: Some("Default".to_owned()),
                credential: ProviderCredential::Token {
                    token: "token-default".to_owned(),
                    expires_at_ms: None,
                    metadata: BTreeMap::new(),
                },
            },
        ];

        let selected = select_default_auth_profiles(profiles);
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].profile_id, "github-copilot:default");
    }

    #[test]
    fn provider_override_from_oauth_profile_preserves_codex_refresh_metadata() {
        let profile = ProviderAuthProfile {
            profile_id: "openai-codex:default".to_owned(),
            provider_id: "openai-codex".to_owned(),
            display_name: Some("Codex".to_owned()),
            credential: ProviderCredential::OAuth {
                access_token: "access".to_owned(),
                refresh_token: Some("refresh".to_owned()),
                expires_at_ms: Some(42),
                account_id: Some("acct".to_owned()),
                email: Some("user@example.com".to_owned()),
            },
        };

        let override_config = provider_override_from_auth_profile(&profile);
        assert_eq!(override_config.api_key.as_deref(), Some("access"));
        assert_eq!(
            override_config.metadata.get("openai_codex.refresh_token").map(String::as_str),
            Some("refresh")
        );
        assert_eq!(
            override_config.metadata.get("openai_codex.account_id").map(String::as_str),
            Some("acct")
        );
    }

    #[test]
    fn runtime_config_update_changes_approval_policy() {
        let temp_dir = TempDir::new().expect("temp dir should be created");
        let mut server = AppServer::new_simulated(StoragePaths::new(temp_dir.path().join(".liz")));

        let response = server.handle_request(envelope(
            "set_permissions",
            ClientRequest::RuntimeConfigUpdate(RuntimeConfigUpdateRequest {
                sandbox: None,
                approval_policy: Some(ApprovalPolicy::DangerFullAccess),
            }),
        ));

        match response {
            ServerResponseEnvelope::Success(success) => match success.response {
                ResponsePayload::RuntimeConfig(config) => {
                    assert_eq!(config.approval_policy, ApprovalPolicy::DangerFullAccess);
                }
                other => panic!("unexpected response payload: {other:?}"),
            },
            other => panic!("unexpected response envelope: {other:?}"),
        }

        let response = server.handle_request(envelope(
            "get_permissions",
            ClientRequest::RuntimeConfigGet(RuntimeConfigGetRequest {}),
        ));
        match response {
            ServerResponseEnvelope::Success(success) => match success.response {
                ResponsePayload::RuntimeConfig(config) => {
                    assert_eq!(config.approval_policy, ApprovalPolicy::DangerFullAccess);
                }
                other => panic!("unexpected response payload: {other:?}"),
            },
            other => panic!("unexpected response envelope: {other:?}"),
        }
    }

    #[test]
    fn danger_full_access_policy_skips_high_risk_approval_prompt() {
        let temp_dir = TempDir::new().expect("temp dir should be created");
        let mut server = AppServer::new_simulated(StoragePaths::new(temp_dir.path().join(".liz")));
        let events = server.subscribe_events();

        server.handle_request(envelope(
            "set_permissions",
            ClientRequest::RuntimeConfigUpdate(RuntimeConfigUpdateRequest {
                sandbox: None,
                approval_policy: Some(ApprovalPolicy::DangerFullAccess),
            }),
        ));
        let response = server.handle_request(envelope(
            "start_high_risk_turn",
            ClientRequest::ThreadStart(ThreadStartRequest {
                title: Some("High risk".to_owned()),
                initial_goal: None,
                workspace_ref: None,
            }),
        ));
        let thread = match response {
            ServerResponseEnvelope::Success(success) => match success.response {
                ResponsePayload::ThreadStart(response) => response.thread,
                other => panic!("unexpected response payload: {other:?}"),
            },
            other => panic!("unexpected response envelope: {other:?}"),
        };
        server.handle_request(envelope(
            "run_high_risk_turn",
            ClientRequest::TurnStart(TurnStartRequest {
                thread_id: thread.id,
                input: "delete .env".to_owned(),
                input_kind: TurnInputKind::UserMessage,
                channel: None,
                participant: None,
            }),
        ));

        let mut saw_approval = false;
        while let Ok(event) = events.recv_timeout(Duration::from_millis(25)) {
            if matches!(event.payload, ServerEventPayload::ApprovalRequested(_)) {
                saw_approval = true;
                break;
            }
        }

        assert!(!saw_approval);
    }

    fn envelope(request_id: &str, request: ClientRequest) -> ClientRequestEnvelope {
        ClientRequestEnvelope { request_id: RequestId::new(request_id), request }
    }
}

fn parse_tool_invocation(
    tool_name: &str,
    arguments: &serde_json::Value,
) -> Result<liz_protocol::ToolInvocation, String> {
    let value = arguments;
    match tool_name {
        "workspace.list" => {
            Ok(liz_protocol::ToolInvocation::WorkspaceList(liz_protocol::WorkspaceListRequest {
                root: required_string_field(value, "root")?,
                recursive: optional_bool_field(value, "recursive").unwrap_or(false),
                include_hidden: optional_bool_field(value, "include_hidden").unwrap_or(false),
                max_entries: optional_usize_field(value, "max_entries"),
            }))
        }
        "workspace.search" => Ok(liz_protocol::ToolInvocation::WorkspaceSearch(
            liz_protocol::WorkspaceSearchRequest {
                root: required_string_field(value, "root")?,
                pattern: required_string_field(value, "pattern")?,
                case_sensitive: optional_bool_field(value, "case_sensitive").unwrap_or(false),
                include_hidden: optional_bool_field(value, "include_hidden").unwrap_or(false),
                max_results: optional_usize_field(value, "max_results"),
            },
        )),
        "workspace.read" => {
            Ok(liz_protocol::ToolInvocation::WorkspaceRead(liz_protocol::WorkspaceReadRequest {
                path: required_string_field(value, "path")?,
                start_line: optional_usize_field(value, "start_line"),
                end_line: optional_usize_field(value, "end_line"),
            }))
        }
        "workspace.write_text" => Ok(liz_protocol::ToolInvocation::WorkspaceWriteText(
            liz_protocol::WorkspaceWriteTextRequest {
                path: required_string_field(value, "path")?,
                content: required_string_field(value, "content")?,
            },
        )),
        "workspace.apply_patch" => Ok(liz_protocol::ToolInvocation::WorkspaceApplyPatch(
            liz_protocol::WorkspaceApplyPatchRequest {
                path: required_string_field(value, "path")?,
                search: required_string_field(value, "search")?,
                replace: required_string_field(value, "replace")?,
                replace_all: optional_bool_field(value, "replace_all").unwrap_or(false),
            },
        )),
        "shell.exec" => {
            Ok(liz_protocol::ToolInvocation::ShellExec(liz_protocol::ShellExecRequest {
                command: required_string_field(value, "command")?,
                working_dir: optional_string_field(value, "working_dir"),
                sandbox: parse_shell_sandbox_request(value.get("sandbox")),
            }))
        }
        "shell.spawn" => {
            Ok(liz_protocol::ToolInvocation::ShellSpawn(liz_protocol::ShellSpawnRequest {
                command: required_string_field(value, "command")?,
                working_dir: optional_string_field(value, "working_dir"),
                sandbox: parse_shell_sandbox_request(value.get("sandbox")),
            }))
        }
        "shell.wait" => {
            Ok(liz_protocol::ToolInvocation::ShellWait(liz_protocol::ShellWaitRequest {
                task_id: ExecutorTaskId::new(required_string_field(value, "task_id")?),
            }))
        }
        "shell.read_output" => Ok(liz_protocol::ToolInvocation::ShellReadOutput(
            liz_protocol::ShellReadOutputRequest {
                task_id: ExecutorTaskId::new(required_string_field(value, "task_id")?),
            },
        )),
        "shell.terminate" => {
            Ok(liz_protocol::ToolInvocation::ShellTerminate(liz_protocol::ShellTerminateRequest {
                task_id: ExecutorTaskId::new(required_string_field(value, "task_id")?),
            }))
        }
        _ => Err(format!("unknown tool name: {tool_name}")),
    }
}

fn required_string_field(value: &serde_json::Value, field: &str) -> Result<String, String> {
    value
        .get(field)
        .and_then(|field_value| field_value.as_str())
        .map(str::to_owned)
        .ok_or_else(|| format!("missing required string field `{field}`"))
}

fn optional_string_field(value: &serde_json::Value, field: &str) -> Option<String> {
    value.get(field).and_then(|field_value| field_value.as_str()).map(str::to_owned)
}

fn optional_bool_field(value: &serde_json::Value, field: &str) -> Option<bool> {
    value.get(field).and_then(|field_value| field_value.as_bool())
}

fn optional_usize_field(value: &serde_json::Value, field: &str) -> Option<usize> {
    value
        .get(field)
        .and_then(|field_value| field_value.as_u64())
        .and_then(|raw| usize::try_from(raw).ok())
}

fn parse_shell_sandbox_request(
    value: Option<&serde_json::Value>,
) -> Option<liz_protocol::ShellSandboxRequest> {
    let value = value?;
    let mode =
        serde_json::from_value::<liz_protocol::SandboxMode>(value.get("mode")?.clone()).ok()?;
    let network_access = serde_json::from_value::<liz_protocol::SandboxNetworkAccess>(
        value.get("network_access")?.clone(),
    )
    .ok()?;
    Some(liz_protocol::ShellSandboxRequest { mode, network_access })
}

fn executor_task_id_for_result(
    thread_id: &ThreadId,
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

#[cfg(test)]
mod tool_invocation_tests {
    use super::parse_tool_invocation;
    use liz_protocol::{SandboxMode, SandboxNetworkAccess, ToolInvocation};

    #[test]
    fn parses_shell_exec_with_sandbox_override() {
        let invocation = parse_tool_invocation(
            "shell.exec",
            &serde_json::json!({
                "command":"echo hello",
                "working_dir":"/tmp/workspace",
                "sandbox":{
                    "mode":"danger-full-access",
                    "network_access":"enabled"
                }
            }),
        )
        .expect("shell.exec invocation should parse");

        match invocation {
            ToolInvocation::ShellExec(request) => {
                let sandbox = request.sandbox.expect("sandbox override should be present");
                assert_eq!(sandbox.mode, SandboxMode::DangerFullAccess);
                assert_eq!(sandbox.network_access, SandboxNetworkAccess::Enabled);
                assert_eq!(request.command, "echo hello");
                assert_eq!(request.working_dir.as_deref(), Some("/tmp/workspace"));
            }
            other => panic!("unexpected invocation: {other:?}"),
        }
    }
}
