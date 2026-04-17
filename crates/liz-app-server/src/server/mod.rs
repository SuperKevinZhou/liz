//! Transport-facing server façade.

mod websocket;

use crate::events::EventBus;
use crate::executor::ExecutorGateway;
use crate::handlers;
use crate::model::{ModelGateway, ModelTurnRequest, NormalizedTurnEvent, ProviderOverride};
use crate::runtime::RuntimeCoordinator;
use crate::storage::StoragePaths;
use liz_protocol::{
    ApprovalDecision, ApprovalRequestedEvent, ArtifactCreatedEvent, AssistantChunkEvent,
    AssistantCompletedEvent, CheckpointCreatedEvent, ClientRequestEnvelope, DiffAvailableEvent,
    ExecutorOutputChunkEvent, ExecutorTaskId, MemoryCompilationAppliedEvent,
    MemoryInvalidationAppliedEvent, ProviderAuthProfile, ProviderCredential, ServerEvent,
    ServerEventPayload, ServerResponseEnvelope, ThreadId, ToolCallRequest, ToolCompletedEvent,
    TurnCancelRequest, TurnCompletedEvent, TurnFailedEvent, TurnId,
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
}

impl AppServer {
    /// Creates a new app server rooted at the provided storage paths.
    pub fn new(paths: StoragePaths) -> Self {
        Self::new_with_model_gateway(paths, ModelGateway::default())
    }

    /// Creates a new app server rooted at the provided storage paths and explicit model gateway.
    pub fn new_with_model_gateway(paths: StoragePaths, model_gateway: ModelGateway) -> Self {
        Self {
            runtime: RuntimeCoordinator::new(crate::runtime::RuntimeStores::new(paths)),
            executor: ExecutorGateway::default(),
            event_bus: EventBus::new(),
            model_gateway,
        }
    }

    /// Creates a new app server with simulated provider streaming for isolated tests.
    pub fn new_simulated(paths: StoragePaths) -> Self {
        Self::new_with_model_gateway(paths, ModelGateway::simulated())
    }

    /// Creates an app server using the default `.liz` storage layout.
    pub fn from_default_layout() -> Self {
        Self {
            runtime: RuntimeCoordinator::default(),
            executor: ExecutorGateway::default(),
            event_bus: EventBus::new(),
            model_gateway: ModelGateway::default(),
        }
    }

    /// Handles a single protocol request and returns the matching response envelope.
    pub fn handle_request(&mut self, envelope: ClientRequestEnvelope) -> ServerResponseEnvelope {
        let request = envelope.request.clone();
        let handled = handlers::handle_request(&mut self.runtime, &self.executor, envelope);
        self.event_bus.publish_all(handled.events);
        match request {
            liz_protocol::ClientRequest::TurnStart(request) => {
                self.continue_turn_after_policy(&handled.response, request.input);
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

    fn continue_turn_after_policy(&mut self, response: &ServerResponseEnvelope, input: String) {
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

        let Ok(context) = self.runtime.assemble_context(&thread.id, &input) else {
            return;
        };
        let decision = self.runtime.evaluate_policy(&input, &context);

        if decision.requires_approval {
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

        self.stream_model_turn(thread, turn, context.prompt);
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
                self.stream_model_turn(thread, turn, context.prompt);
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
        prompt: String,
    ) {
        let thread_id = thread.id.clone();
        let turn_id = turn.id.clone();
        let model_gateway = self.gateway_with_provider_auth_profiles();

        let run_result = model_gateway
            .run_turn(ModelTurnRequest { thread, turn, prompt }, |event| {
                self.handle_model_event(&thread_id, &turn_id, event)
            });

        match run_result {
            Ok(summary) => {
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
            }
            Err(error) => {
                if let Ok(turn) = self.runtime.fail_turn(&thread_id, &turn_id, error.to_string()) {
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
            }
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
                if let Some(invocation) = parse_tool_invocation(&tool_name, &arguments) {
                    self.execute_model_tool(thread_id, turn_id, invocation);
                }
            }
            NormalizedTurnEvent::UsageDelta(_) | NormalizedTurnEvent::ProviderRawEvent { .. } => {}
        }
    }

    fn execute_model_tool(
        &mut self,
        thread_id: &ThreadId,
        turn_id: &TurnId,
        invocation: liz_protocol::ToolInvocation,
    ) {
        let request = ToolCallRequest {
            thread_id: thread_id.clone(),
            turn_id: Some(turn_id.clone()),
            invocation,
        };
        let Ok(executed) = self.executor.execute_tool(&request) else {
            return;
        };
        let executor_task_id = executor_task_id_for_result(thread_id, &executed.result);
        let output_chunks = executed.output_chunks.clone();
        let artifacts = executed
            .artifacts
            .into_iter()
            .map(|artifact| (artifact.kind, artifact.summary, artifact.body))
            .collect::<Vec<_>>();
        let Ok((execution_turn_id, artifact_refs)) = self.runtime.record_tool_execution(
            thread_id,
            Some(turn_id),
            executed.tool_name.as_str(),
            &executed.summary,
            artifacts,
        ) else {
            return;
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
    use super::{provider_override_from_auth_profile, select_default_auth_profiles};
    use liz_protocol::{ProviderAuthProfile, ProviderCredential};
    use std::collections::BTreeMap;

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
}

fn parse_tool_invocation(tool_name: &str, arguments: &str) -> Option<liz_protocol::ToolInvocation> {
    let value: serde_json::Value = serde_json::from_str(arguments).ok()?;
    match tool_name {
        "shell.exec" => {
            Some(liz_protocol::ToolInvocation::ShellExec(liz_protocol::ShellExecRequest {
                command: value.get("command")?.as_str()?.to_owned(),
                working_dir: value
                    .get("working_dir")
                    .and_then(|value| value.as_str())
                    .map(str::to_owned),
                sandbox: parse_shell_sandbox_request(value.get("sandbox")),
            }))
        }
        _ => None,
    }
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
            r#"{
                "command":"echo hello",
                "working_dir":"/tmp/workspace",
                "sandbox":{
                    "mode":"danger-full-access",
                    "network_access":"enabled"
                }
            }"#,
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
