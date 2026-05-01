//! High-level runtime coordination for thread and turn lifecycle work.

use crate::model::{
    build_openai_codex_authorize_url, exchange_gitlab_oauth_code,
    exchange_openai_codex_authorization_code, poll_github_copilot_device_authorization,
    poll_minimax_oauth_authorization, start_github_copilot_device_authorization,
    start_gitlab_oauth_authorization, start_minimax_oauth_authorization,
    GitHubCopilotDevicePollOutcome, MiniMaxOAuthPollOutcome,
};
use crate::runtime::context_assembler::{AssembledContext, ContextAssembler};
use crate::runtime::error::{RuntimeError, RuntimeResult};
use crate::runtime::ids::IdGenerator;
use crate::runtime::memory::ForegroundMemoryEngine;
use crate::runtime::policy_engine::{PolicyDecision, PolicyEngine};
use crate::runtime::stores::RuntimeStores;
use crate::runtime::thread_manager::ThreadManager;
use crate::runtime::turn_manager::TurnManager;
use crate::storage::StoredArtifact;
use liz_protocol::memory::ResumeSummary;
use liz_protocol::requests::{
    ApprovalRespondRequest, GitHubCopilotDevicePollRequest, GitHubCopilotDeviceStartRequest,
    GitLabOAuthCompleteRequest, GitLabOAuthStartRequest, GitLabPatSaveRequest,
    MemoryCompileNowRequest, MemoryListTopicsRequest, MemoryOpenEvidenceRequest,
    MemoryOpenSessionRequest, MemoryReadWakeupRequest, MemorySearchRequest,
    MiniMaxOAuthPollRequest, MiniMaxOAuthStartRequest, OpenAiCodexOAuthCompleteRequest,
    OpenAiCodexOAuthStartRequest, ProviderAuthDeleteRequest, ProviderAuthListRequest,
    ProviderAuthUpsertRequest, ThreadForkRequest, ThreadListRequest, ThreadResumeRequest,
    ThreadStartRequest, TurnCancelRequest, TurnStartRequest,
};
use liz_protocol::responses::{
    ApprovalRespondResponse, GitHubCopilotDevicePollResponse, GitHubCopilotDeviceStartResponse,
    GitLabOAuthCompleteResponse, GitLabOAuthStartResponse, GitLabPatSaveResponse,
    MemoryCompileNowResponse, MemoryListTopicsResponse, MemoryOpenEvidenceResponse,
    MemoryOpenSessionResponse, MemoryReadWakeupResponse, MemorySearchResponse,
    MiniMaxOAuthPollResponse, MiniMaxOAuthStartResponse, OpenAiCodexOAuthCompleteResponse,
    OpenAiCodexOAuthStartResponse, ProviderAuthDeleteResponse, ProviderAuthListResponse,
    ProviderAuthUpsertResponse, ThreadForkResponse, ThreadListResponse, ThreadResumeResponse,
    ThreadStartResponse, TurnCancelResponse, TurnStartResponse,
};
use liz_protocol::{
    ApprovalDecision, ApprovalRequest, ApprovalStatus, ArtifactKind, ArtifactRef, Checkpoint,
    CheckpointScope, GitHubCopilotDeviceCode, GitHubCopilotDevicePollStatus, GitLabOAuthStart,
    MiniMaxOAuthDeviceCode, MiniMaxOAuthPollStatus, OpenAiCodexOAuthStart, ParticipantRef,
    ProviderAuthProfile, ProviderCredential, Thread, ThreadId, Turn, TurnId,
};
use std::collections::HashMap;

/// Coordinates the persisted runtime state for thread and turn lifecycle actions.
#[derive(Debug)]
pub struct RuntimeCoordinator {
    stores: RuntimeStores,
    ids: IdGenerator,
    thread_manager: ThreadManager,
    turn_manager: TurnManager,
    context_assembler: ContextAssembler,
    memory_engine: ForegroundMemoryEngine,
    policy_engine: PolicyEngine,
    approvals: HashMap<liz_protocol::ApprovalId, ApprovalRequest>,
}

impl RuntimeCoordinator {
    /// Creates a runtime coordinator backed by the provided stores.
    pub fn new(stores: RuntimeStores) -> Self {
        Self {
            stores,
            ids: IdGenerator::default(),
            thread_manager: ThreadManager::default(),
            turn_manager: TurnManager::default(),
            context_assembler: ContextAssembler::default(),
            memory_engine: ForegroundMemoryEngine::default(),
            policy_engine: PolicyEngine::default(),
            approvals: HashMap::new(),
        }
    }

    /// Returns the short runtime mode label used by the binary banner.
    pub fn default_mode() -> &'static str {
        "thread-turn-runtime"
    }

    /// Creates a runtime coordinator rooted at explicit storage paths.
    pub fn from_storage_paths(paths: crate::storage::StoragePaths) -> Self {
        Self::new(RuntimeStores::new(paths))
    }

    /// Starts a new thread and persists the initial thread projection.
    pub fn start_thread(
        &mut self,
        request: ThreadStartRequest,
    ) -> RuntimeResult<ThreadStartResponse> {
        let thread = self.thread_manager.start_thread(&self.stores, &mut self.ids, request)?;
        Ok(ThreadStartResponse { thread })
    }

    /// Lists persisted provider auth profiles.
    pub fn list_provider_auth_profiles(
        &self,
        request: ProviderAuthListRequest,
    ) -> RuntimeResult<ProviderAuthListResponse> {
        let mut snapshot = self.stores.read_auth_profiles()?;
        if let Some(provider_id) = request.provider_id.as_deref() {
            snapshot.profiles.retain(|profile| profile.provider_id == provider_id);
        }
        Ok(ProviderAuthListResponse { profiles: snapshot.profiles })
    }

    /// Creates or replaces a provider auth profile.
    pub fn upsert_provider_auth_profile(
        &mut self,
        request: ProviderAuthUpsertRequest,
    ) -> RuntimeResult<ProviderAuthUpsertResponse> {
        validate_provider_auth_profile(&request.profile)?;

        let mut snapshot = self.stores.read_auth_profiles()?;
        if let Some(existing) = snapshot
            .profiles
            .iter_mut()
            .find(|profile| profile.profile_id == request.profile.profile_id)
        {
            *existing = request.profile.clone();
        } else {
            snapshot.profiles.push(request.profile.clone());
        }
        snapshot.profiles.sort_by(|left, right| left.profile_id.cmp(&right.profile_id));
        self.stores.write_auth_profiles(&snapshot)?;

        Ok(ProviderAuthUpsertResponse { profile: request.profile })
    }

    /// Deletes a provider auth profile.
    pub fn delete_provider_auth_profile(
        &mut self,
        request: ProviderAuthDeleteRequest,
    ) -> RuntimeResult<ProviderAuthDeleteResponse> {
        let mut snapshot = self.stores.read_auth_profiles()?;
        let original_len = snapshot.profiles.len();
        snapshot.profiles.retain(|profile| profile.profile_id != request.profile_id);
        if snapshot.profiles.len() == original_len {
            return Err(RuntimeError::not_found(
                "provider_auth_profile_not_found",
                format!("provider auth profile {} does not exist", request.profile_id),
            ));
        }
        self.stores.write_auth_profiles(&snapshot)?;
        Ok(ProviderAuthDeleteResponse { profile_id: request.profile_id })
    }

    /// Starts an OpenAI Codex OAuth login flow.
    pub fn start_openai_codex_oauth_login(
        &self,
        request: OpenAiCodexOAuthStartRequest,
    ) -> RuntimeResult<OpenAiCodexOAuthStartResponse> {
        let code_verifier = generate_codex_code_verifier();
        let code_challenge = pkce_sha256_challenge(&code_verifier);
        let state = generate_codex_state();
        let authorize_url = build_openai_codex_authorize_url(
            &request.redirect_uri,
            &code_challenge,
            &state,
            request.originator.as_deref().unwrap_or("liz"),
        )
        .map_err(|error| {
            RuntimeError::invalid_state("openai_codex_oauth_start_failed", error.to_string())
        })?;

        Ok(OpenAiCodexOAuthStartResponse {
            oauth: OpenAiCodexOAuthStart { authorize_url, state, code_verifier },
        })
    }

    /// Completes an OpenAI Codex OAuth login flow and persists the resulting profile.
    pub fn complete_openai_codex_oauth_login(
        &mut self,
        request: OpenAiCodexOAuthCompleteRequest,
    ) -> RuntimeResult<OpenAiCodexOAuthCompleteResponse> {
        let callback = parse_codex_callback(&request.code_or_redirect_url).map_err(|error| {
            RuntimeError::invalid_state("openai_codex_oauth_complete_failed", error)
        })?;
        if let Some(expected_state) = request.expected_state.as_deref() {
            if callback.state.as_deref() != Some(expected_state) {
                return Err(RuntimeError::invalid_state(
                    "openai_codex_oauth_state_mismatch",
                    "OpenAI Codex OAuth callback state did not match the expected value",
                ));
            }
        }

        let token_url_override = std::env::var("OPENAI_CODEX_TOKEN_URL")
            .ok()
            .or_else(|| std::env::var("LIZ_OPENAI_CODEX_TOKEN_URL").ok());
        let oauth = exchange_openai_codex_authorization_code(
            &callback.code,
            &request.redirect_uri,
            &request.code_verifier,
            token_url_override.as_deref(),
        )
        .map_err(|error| {
            RuntimeError::invalid_state("openai_codex_oauth_complete_failed", error.to_string())
        })?;

        let profile = ProviderAuthProfile {
            profile_id: request.profile_id.unwrap_or_else(|| "openai-codex:default".to_owned()),
            provider_id: "openai-codex".to_owned(),
            display_name: oauth.email.clone().or(Some("OpenAI Codex".to_owned())),
            credential: ProviderCredential::OAuth {
                access_token: oauth.access_token,
                refresh_token: Some(oauth.refresh_token),
                expires_at_ms: Some(oauth.expires_at_ms),
                account_id: oauth.account_id,
                email: oauth.email,
            },
        };
        let response = self.upsert_provider_auth_profile(ProviderAuthUpsertRequest { profile })?;
        Ok(OpenAiCodexOAuthCompleteResponse { profile: response.profile })
    }

    /// Starts a GitHub Copilot device-code login flow.
    pub fn start_github_copilot_device_login(
        &self,
        request: GitHubCopilotDeviceStartRequest,
    ) -> RuntimeResult<GitHubCopilotDeviceStartResponse> {
        let device =
            start_github_copilot_device_authorization(request.enterprise_url.as_deref(), None)
                .map_err(|error| {
                    RuntimeError::invalid_state(
                        "github_copilot_device_start_failed",
                        error.to_string(),
                    )
                })?;

        Ok(GitHubCopilotDeviceStartResponse {
            device: GitHubCopilotDeviceCode {
                verification_uri: device.verification_uri,
                user_code: device.user_code,
                device_code: device.device_code,
                interval_seconds: device.interval_seconds,
                api_base_url: device.api_base_url,
            },
        })
    }

    /// Polls a GitHub Copilot device-code login flow and persists the resulting auth profile.
    pub fn poll_github_copilot_device_login(
        &mut self,
        request: GitHubCopilotDevicePollRequest,
    ) -> RuntimeResult<GitHubCopilotDevicePollResponse> {
        let poll = poll_github_copilot_device_authorization(
            &request.device_code,
            request.enterprise_url.as_deref(),
            request.interval_seconds,
            None,
        )
        .map_err(|error| {
            RuntimeError::invalid_state("github_copilot_device_poll_failed", error.to_string())
        })?;

        match poll {
            GitHubCopilotDevicePollOutcome::Pending { retry_after_seconds } => {
                Ok(GitHubCopilotDevicePollResponse {
                    status: GitHubCopilotDevicePollStatus::Pending,
                    retry_after_seconds: Some(retry_after_seconds),
                    profile: None,
                })
            }
            GitHubCopilotDevicePollOutcome::SlowDown { retry_after_seconds } => {
                Ok(GitHubCopilotDevicePollResponse {
                    status: GitHubCopilotDevicePollStatus::SlowDown,
                    retry_after_seconds: Some(retry_after_seconds),
                    profile: None,
                })
            }
            GitHubCopilotDevicePollOutcome::Complete { github_token, api_base_url } => {
                let profile = ProviderAuthProfile {
                    profile_id: request
                        .profile_id
                        .unwrap_or_else(|| "github-copilot:default".to_owned()),
                    provider_id: "github-copilot".to_owned(),
                    display_name: Some("GitHub Copilot".to_owned()),
                    credential: ProviderCredential::Token {
                        token: github_token,
                        expires_at_ms: None,
                        metadata: std::collections::BTreeMap::from([(
                            "copilot.api_base_url".to_owned(),
                            api_base_url,
                        )]),
                    },
                };
                let response = self.upsert_provider_auth_profile(ProviderAuthUpsertRequest {
                    profile: profile.clone(),
                })?;
                Ok(GitHubCopilotDevicePollResponse {
                    status: GitHubCopilotDevicePollStatus::Complete,
                    retry_after_seconds: None,
                    profile: Some(response.profile),
                })
            }
        }
    }

    /// Starts a GitLab OAuth login flow.
    pub fn start_gitlab_oauth_login(
        &self,
        request: GitLabOAuthStartRequest,
    ) -> RuntimeResult<GitLabOAuthStartResponse> {
        let oauth = start_gitlab_oauth_authorization(
            &request.instance_url,
            &request.client_id,
            &request.redirect_uri,
            &request.scopes,
        )
        .map_err(|error| {
            RuntimeError::invalid_state("gitlab_oauth_start_failed", error.to_string())
        })?;

        Ok(GitLabOAuthStartResponse {
            oauth: GitLabOAuthStart {
                authorize_url: oauth.authorize_url,
                state: oauth.state,
                code_verifier: oauth.code_verifier,
            },
        })
    }

    /// Completes a GitLab OAuth login flow and persists the resulting profile.
    pub fn complete_gitlab_oauth_login(
        &mut self,
        request: GitLabOAuthCompleteRequest,
    ) -> RuntimeResult<GitLabOAuthCompleteResponse> {
        let token_url_override = std::env::var("LIZ_GITLAB_OAUTH_TOKEN_URL").ok();
        let oauth = exchange_gitlab_oauth_code(
            &request.instance_url,
            &request.client_id,
            request.client_secret.as_deref(),
            &request.redirect_uri,
            &request.code,
            request.code_verifier.as_deref(),
            token_url_override.as_deref(),
        )
        .map_err(|error| {
            RuntimeError::invalid_state("gitlab_oauth_complete_failed", error.to_string())
        })?;

        let profile = ProviderAuthProfile {
            profile_id: request.profile_id.unwrap_or_else(|| "gitlab:default".to_owned()),
            provider_id: "gitlab".to_owned(),
            display_name: Some("GitLab OAuth".to_owned()),
            credential: ProviderCredential::Token {
                token: oauth.access_token,
                expires_at_ms: oauth.expires_at_ms,
                metadata: std::collections::BTreeMap::from_iter(
                    [
                        Some(("gitlab.auth_mode".to_owned(), "oauth".to_owned())),
                        Some(("gitlab.instance_url".to_owned(), request.instance_url.clone())),
                        Some(("gitlab.oauth_client_id".to_owned(), request.client_id.clone())),
                        request
                            .client_secret
                            .clone()
                            .map(|value| ("gitlab.oauth_client_secret".to_owned(), value)),
                        oauth
                            .refresh_token
                            .clone()
                            .map(|value| ("gitlab.oauth.refresh_token".to_owned(), value)),
                        oauth.expires_at_ms.map(|value| {
                            ("gitlab.oauth.expires_at_ms".to_owned(), value.to_string())
                        }),
                    ]
                    .into_iter()
                    .flatten(),
                ),
            },
        };
        let response = self.upsert_provider_auth_profile(ProviderAuthUpsertRequest { profile })?;
        Ok(GitLabOAuthCompleteResponse { profile: response.profile })
    }

    /// Saves a GitLab personal access token as a provider auth profile.
    pub fn save_gitlab_pat(
        &mut self,
        request: GitLabPatSaveRequest,
    ) -> RuntimeResult<GitLabPatSaveResponse> {
        let mut metadata = std::collections::BTreeMap::new();
        metadata.insert("gitlab.auth_mode".to_owned(), "pat".to_owned());
        if let Some(instance_url) = request.instance_url.clone() {
            metadata.insert("gitlab.instance_url".to_owned(), instance_url);
        }
        let profile = ProviderAuthProfile {
            profile_id: request.profile_id.unwrap_or_else(|| "gitlab:default".to_owned()),
            provider_id: "gitlab".to_owned(),
            display_name: request.display_name.or(Some("GitLab PAT".to_owned())),
            credential: ProviderCredential::Token {
                token: request.token,
                expires_at_ms: None,
                metadata,
            },
        };
        let response = self.upsert_provider_auth_profile(ProviderAuthUpsertRequest { profile })?;
        Ok(GitLabPatSaveResponse { profile: response.profile })
    }

    /// Starts a MiniMax Portal OAuth login flow.
    pub fn start_minimax_oauth_login(
        &self,
        request: MiniMaxOAuthStartRequest,
    ) -> RuntimeResult<MiniMaxOAuthStartResponse> {
        let device = start_minimax_oauth_authorization(&request.region).map_err(|error| {
            RuntimeError::invalid_state("minimax_oauth_start_failed", error.to_string())
        })?;

        Ok(MiniMaxOAuthStartResponse {
            device: MiniMaxOAuthDeviceCode {
                verification_uri: device.verification_uri,
                user_code: device.user_code,
                code_verifier: device.code_verifier,
                interval_ms: device.interval_ms,
                expires_at_ms: device.expires_at_ms,
                region: device.region,
            },
        })
    }

    /// Polls a MiniMax Portal OAuth login flow until completion.
    pub fn poll_minimax_oauth_login(
        &mut self,
        request: MiniMaxOAuthPollRequest,
    ) -> RuntimeResult<MiniMaxOAuthPollResponse> {
        let poll = poll_minimax_oauth_authorization(
            &request.region,
            &request.user_code,
            &request.code_verifier,
            request.interval_ms,
        )
        .map_err(|error| {
            RuntimeError::invalid_state("minimax_oauth_poll_failed", error.to_string())
        })?;

        match poll {
            MiniMaxOAuthPollOutcome::Pending { retry_after_ms } => Ok(MiniMaxOAuthPollResponse {
                status: MiniMaxOAuthPollStatus::Pending,
                retry_after_ms: Some(retry_after_ms),
                profile: None,
            }),
            MiniMaxOAuthPollOutcome::Complete { auth } => {
                let profile = ProviderAuthProfile {
                    profile_id: request
                        .profile_id
                        .unwrap_or_else(|| "minimax-portal:default".to_owned()),
                    provider_id: "minimax-portal".to_owned(),
                    display_name: Some(format!("MiniMax OAuth ({})", request.region)),
                    credential: ProviderCredential::Token {
                        token: auth.access_token,
                        expires_at_ms: Some(auth.expires_at_ms),
                        metadata: std::collections::BTreeMap::from([
                            ("minimax.auth_mode".to_owned(), "oauth".to_owned()),
                            ("minimax.region".to_owned(), request.region.clone()),
                            ("minimax.oauth.refresh_token".to_owned(), auth.refresh_token),
                            ("minimax.resource_url".to_owned(), auth.resource_url),
                        ]),
                    },
                };
                let response =
                    self.upsert_provider_auth_profile(ProviderAuthUpsertRequest { profile })?;
                Ok(MiniMaxOAuthPollResponse {
                    status: MiniMaxOAuthPollStatus::Complete,
                    retry_after_ms: None,
                    profile: Some(response.profile),
                })
            }
        }
    }

    /// Resumes a thread and returns the current wake-up projection.
    pub fn resume_thread(
        &mut self,
        request: ThreadResumeRequest,
    ) -> RuntimeResult<ThreadResumeResponse> {
        let thread = self.thread_manager.resume_thread(&self.stores, request)?;
        let resume_summary = Some(self.build_resume_summary(&thread));
        Ok(ThreadResumeResponse { thread, resume_summary })
    }

    /// Lists persisted threads for picker-style client surfaces.
    pub fn list_threads(&self, request: ThreadListRequest) -> RuntimeResult<ThreadListResponse> {
        let mut threads = self.stores.list_threads()?;
        if let Some(status) = request.status {
            threads.retain(|thread| thread.status == status);
        }
        threads.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
        if let Some(limit) = request.limit {
            threads.truncate(limit);
        }
        Ok(ThreadListResponse { threads })
    }

    /// Reads the current foreground memory wake-up for a thread.
    pub fn read_memory_wakeup(
        &self,
        request: MemoryReadWakeupRequest,
    ) -> RuntimeResult<MemoryReadWakeupResponse> {
        let (wakeup, recent_conversation) =
            self.memory_engine.read_wakeup(&self.stores, &request.thread_id)?;
        Ok(MemoryReadWakeupResponse { thread_id: request.thread_id, wakeup, recent_conversation })
    }

    /// Forces a foreground memory compilation pass for a thread.
    pub fn compile_memory_now(
        &self,
        request: MemoryCompileNowRequest,
    ) -> RuntimeResult<MemoryCompileNowResponse> {
        let compilation =
            self.memory_engine.compile_thread(&self.stores, &self.ids, &request.thread_id)?;
        Ok(MemoryCompileNowResponse { thread_id: request.thread_id, compilation })
    }

    /// Lists topic summaries from the memory topic index.
    pub fn list_memory_topics(
        &self,
        request: MemoryListTopicsRequest,
    ) -> RuntimeResult<MemoryListTopicsResponse> {
        let topics = self.memory_engine.list_topics(&self.stores, request.status, request.limit)?;
        Ok(MemoryListTopicsResponse { topics })
    }

    /// Searches memory using the requested recall mode.
    pub fn search_memory(
        &self,
        request: MemorySearchRequest,
    ) -> RuntimeResult<MemorySearchResponse> {
        let hits =
            self.memory_engine.search(&self.stores, &request.query, request.mode, request.limit)?;
        Ok(MemorySearchResponse { query: request.query, mode: request.mode, hits })
    }

    /// Expands a session into recent evidence and artifacts.
    pub fn open_memory_session(
        &self,
        request: MemoryOpenSessionRequest,
    ) -> RuntimeResult<MemoryOpenSessionResponse> {
        let session = self.memory_engine.open_session(&self.stores, &request.thread_id)?;
        Ok(MemoryOpenSessionResponse { session })
    }

    /// Expands a fact or artifact citation into raw evidence.
    pub fn open_memory_evidence(
        &self,
        request: MemoryOpenEvidenceRequest,
    ) -> RuntimeResult<MemoryOpenEvidenceResponse> {
        let evidence = self.memory_engine.open_evidence(
            &self.stores,
            &request.thread_id,
            request.turn_id.as_ref(),
            request.artifact_id.as_ref(),
            request.fact_id.as_ref(),
        )?;
        Ok(MemoryOpenEvidenceResponse { evidence })
    }

    /// Forks a thread into a new line of work.
    pub fn fork_thread(&mut self, request: ThreadForkRequest) -> RuntimeResult<ThreadForkResponse> {
        let thread = self.thread_manager.fork_thread(&self.stores, &mut self.ids, request)?;
        Ok(ThreadForkResponse { thread })
    }

    /// Starts a turn on an existing thread.
    pub fn start_turn(&mut self, request: TurnStartRequest) -> RuntimeResult<TurnStartResponse> {
        let thread = self
            .stores
            .get_thread(&request.thread_id)?
            .ok_or_else(|| RuntimeError::not_found("thread_not_found", "thread does not exist"))?;
        let turn = self.turn_manager.start_turn(
            &self.stores,
            &mut self.ids,
            &self.thread_manager,
            thread,
            request,
        )?;

        Ok(TurnStartResponse { turn })
    }

    /// Cancels a running turn and projects the interruption back onto the thread.
    pub fn cancel_turn(&mut self, request: TurnCancelRequest) -> RuntimeResult<TurnCancelResponse> {
        let turn = self.turn_manager.cancel_turn(&self.stores, &mut self.ids, request)?;
        Ok(TurnCancelResponse { turn })
    }

    /// Responds to a previously generated approval request.
    pub fn respond_approval(
        &mut self,
        request: ApprovalRespondRequest,
    ) -> RuntimeResult<ApprovalRespondResponse> {
        let approval = self.approvals.get_mut(&request.approval_id).ok_or_else(|| {
            RuntimeError::not_found("approval_not_found", "approval does not exist")
        })?;

        approval.status = match request.decision {
            ApprovalDecision::Deny => ApprovalStatus::Denied,
            ApprovalDecision::ApproveOnce | ApprovalDecision::ApproveAndPersist => {
                ApprovalStatus::Approved
            }
        };

        Ok(ApprovalRespondResponse { approval: approval.clone() })
    }

    /// Marks a running turn as completed and projects the result back onto the thread.
    pub fn complete_turn(
        &mut self,
        thread_id: &ThreadId,
        turn_id: &liz_protocol::TurnId,
        final_message: String,
    ) -> RuntimeResult<Turn> {
        self.turn_manager.complete_turn(
            &self.stores,
            &mut self.ids,
            thread_id,
            turn_id,
            final_message,
        )
    }

    /// Marks a running turn as failed and projects the failure back onto the thread.
    pub fn fail_turn(
        &mut self,
        thread_id: &ThreadId,
        turn_id: &liz_protocol::TurnId,
        message: String,
    ) -> RuntimeResult<Turn> {
        self.turn_manager.fail_turn(&self.stores, &mut self.ids, thread_id, turn_id, message)
    }

    /// Returns a persisted thread when it exists.
    pub fn read_thread(&self, thread_id: &ThreadId) -> RuntimeResult<Option<Thread>> {
        Ok(self.stores.get_thread(thread_id)?)
    }

    /// Returns all persisted provider auth profiles.
    pub fn read_provider_auth_profiles(&self) -> RuntimeResult<Vec<ProviderAuthProfile>> {
        Ok(self.stores.read_auth_profiles()?.profiles)
    }

    /// Returns the active in-memory turn projection when it exists.
    pub fn read_turn(&self, turn_id: &liz_protocol::TurnId) -> Option<Turn> {
        self.turn_manager.read_turn(turn_id)
    }

    /// Persists tool artifacts and records a minimal tool-completion trace.
    pub fn record_tool_execution(
        &mut self,
        thread_id: &ThreadId,
        turn_id: Option<&TurnId>,
        tool_name: &str,
        summary: &str,
        artifacts: Vec<(ArtifactKind, String, String)>,
    ) -> RuntimeResult<(TurnId, Vec<ArtifactRef>)> {
        let execution_turn_id = turn_id.cloned().unwrap_or_else(|| self.ids.next_turn_id());
        let created_at = self.ids.now_timestamp();
        let mut references = Vec::with_capacity(artifacts.len());

        for (kind, artifact_summary, body) in artifacts {
            let artifact_id = self.ids.next_artifact_id();
            let locator = self
                .stores
                .paths()
                .artifact_file(&artifact_id)
                .to_string_lossy()
                .replace('\\', "/");
            let reference = ArtifactRef {
                id: artifact_id.clone(),
                thread_id: thread_id.clone(),
                turn_id: execution_turn_id.clone(),
                kind,
                summary: artifact_summary,
                locator,
                created_at: created_at.clone(),
            };
            self.stores.put_artifact(&StoredArtifact { reference: reference.clone(), body })?;
            references.push(reference);
        }

        self.stores.append_turn_log(&crate::storage::TurnLogEntry {
            thread_id: thread_id.clone(),
            sequence: self.next_tool_sequence(thread_id)?,
            turn_id: Some(execution_turn_id.clone()),
            recorded_at: created_at,
            event: "tool_completed".to_owned(),
            summary: format!("{tool_name}: {summary}"),
            artifact_ids: references.iter().map(|artifact| artifact.id.clone()).collect(),
        })?;

        Ok((execution_turn_id, references))
    }

    /// Assembles the current context envelope for a thread and input.
    pub fn assemble_context(
        &self,
        thread_id: &ThreadId,
        input: &str,
    ) -> RuntimeResult<AssembledContext> {
        self.assemble_context_for_participant(thread_id, input, None)
    }

    /// Assembles context for a participant-aware turn.
    pub fn assemble_context_for_participant(
        &self,
        thread_id: &ThreadId,
        input: &str,
        participant: Option<&ParticipantRef>,
    ) -> RuntimeResult<AssembledContext> {
        let thread = self
            .stores
            .get_thread(thread_id)?
            .ok_or_else(|| RuntimeError::not_found("thread_not_found", "thread does not exist"))?;
        let snapshot = self.stores.read_global_memory()?;
        let recent_entries = self.stores.read_turn_log(thread_id)?;
        Ok(self.context_assembler.assemble(&snapshot, &thread, &recent_entries, input, participant))
    }

    /// Runs the same foreground compilation helper used by runtime lifecycle boundaries.
    pub fn compile_thread_memory(
        &self,
        thread_id: &ThreadId,
    ) -> RuntimeResult<liz_protocol::MemoryCompilationSummary> {
        self.memory_engine.compile_thread(&self.stores, &self.ids, thread_id)
    }

    /// Produces a lightweight reflection summary from the currently compiled memory state.
    pub fn summarize_thread_dreaming(&self, thread_id: &ThreadId) -> RuntimeResult<Option<String>> {
        self.memory_engine.summarize_dreaming(&self.stores, thread_id)
    }

    /// Evaluates policy for a turn input and assembled context.
    pub fn evaluate_policy(&self, input: &str, context: &AssembledContext) -> PolicyDecision {
        self.policy_engine.evaluate(input, context)
    }

    /// Creates a checkpoint and approval request for a risky turn and marks it waiting.
    pub fn require_approval_for_turn(
        &mut self,
        thread_id: &ThreadId,
        turn_id: &liz_protocol::TurnId,
        decision: &PolicyDecision,
    ) -> RuntimeResult<(Option<Checkpoint>, ApprovalRequest)> {
        let checkpoint = if decision.requires_checkpoint {
            Some(self.create_checkpoint(
                thread_id,
                turn_id,
                CheckpointScope::ConversationOnly,
                format!("Before risky turn: {}", decision.reason),
            )?)
        } else {
            None
        };
        let approval = ApprovalRequest {
            id: self.ids.next_approval_id(),
            thread_id: thread_id.clone(),
            turn_id: turn_id.clone(),
            action_type: "turn/start".to_owned(),
            risk_level: decision.risk_level,
            reason: decision.reason.clone(),
            sandbox_context: Some(format!(
                "mode={} writable_roots={} network={}",
                decision.sandbox_context.filesystem_mode.as_str(),
                decision.sandbox_context.writable_roots.join(","),
                decision.sandbox_context.network_access.as_str()
            )),
            status: ApprovalStatus::Pending,
        };

        self.turn_manager.mark_waiting_approval(
            &self.stores,
            &mut self.ids,
            thread_id,
            turn_id,
            &decision.reason,
        )?;
        self.approvals.insert(approval.id.clone(), approval.clone());
        Ok((checkpoint, approval))
    }

    /// Marks a previously approved turn as runnable again.
    pub fn resume_approved_turn(
        &mut self,
        thread_id: &ThreadId,
        turn_id: &liz_protocol::TurnId,
    ) -> RuntimeResult<Turn> {
        self.turn_manager.mark_running(&self.stores, &mut self.ids, thread_id, turn_id)
    }

    fn build_resume_summary(&self, thread: &Thread) -> ResumeSummary {
        let headline = match thread.latest_turn_id.as_ref() {
            Some(turn_id) => format!("Resume thread {} from {turn_id}", thread.title),
            None => format!("Resume thread {}", thread.title),
        };

        ResumeSummary {
            headline,
            active_summary: thread.active_summary.clone(),
            pending_commitments: thread.pending_commitments.clone(),
            last_interruption: thread.last_interruption.clone(),
        }
    }

    fn create_checkpoint(
        &mut self,
        thread_id: &ThreadId,
        turn_id: &liz_protocol::TurnId,
        scope: CheckpointScope,
        reason: String,
    ) -> RuntimeResult<Checkpoint> {
        let checkpoint = Checkpoint {
            id: self.ids.next_checkpoint_id(),
            thread_id: thread_id.clone(),
            turn_id: turn_id.clone(),
            scope,
            reason,
            created_at: self.ids.now_timestamp(),
        };
        self.stores.put_checkpoint(&checkpoint)?;

        let mut thread = self
            .stores
            .get_thread(thread_id)?
            .ok_or_else(|| RuntimeError::not_found("thread_not_found", "thread does not exist"))?;
        thread.latest_checkpoint_id = Some(checkpoint.id.clone());
        thread.updated_at = checkpoint.created_at.clone();
        self.stores.put_thread(&thread)?;
        Ok(checkpoint)
    }

    fn next_tool_sequence(&self, thread_id: &ThreadId) -> RuntimeResult<u64> {
        Ok(self.stores.read_turn_log(thread_id)?.len() as u64 + 1)
    }
}

impl Default for RuntimeCoordinator {
    fn default() -> Self {
        Self::new(RuntimeStores::from_default_layout())
    }
}

fn validate_provider_auth_profile(profile: &ProviderAuthProfile) -> RuntimeResult<()> {
    if profile.profile_id.trim().is_empty() {
        return Err(RuntimeError::invalid_state(
            "provider_auth_profile_id_required",
            "provider auth profile id must not be empty",
        ));
    }
    if profile.provider_id.trim().is_empty() {
        return Err(RuntimeError::invalid_state(
            "provider_auth_provider_id_required",
            "provider auth provider id must not be empty",
        ));
    }
    Ok(())
}

#[derive(Debug)]
struct ParsedCodexCallback {
    code: String,
    state: Option<String>,
}

fn parse_codex_callback(input: &str) -> Result<ParsedCodexCallback, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("OpenAI Codex OAuth callback must not be empty".to_owned());
    }

    if let Ok(url) = reqwest::Url::parse(trimmed) {
        let code = url
            .query_pairs()
            .find(|(key, _)| key == "code")
            .map(|(_, value)| value.into_owned())
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| {
                "OpenAI Codex OAuth redirect URL did not include a code parameter".to_owned()
            })?;
        let state = url
            .query_pairs()
            .find(|(key, _)| key == "state")
            .map(|(_, value)| value.into_owned())
            .filter(|value| !value.trim().is_empty());
        return Ok(ParsedCodexCallback { code, state });
    }

    Ok(ParsedCodexCallback { code: trimmed.to_owned(), state: None })
}

fn generate_codex_state() -> String {
    generate_oauth_random_string(32)
}

fn generate_codex_code_verifier() -> String {
    generate_oauth_random_string(64)
}

fn pkce_sha256_challenge(verifier: &str) -> String {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;
    use sha2::{Digest, Sha256};

    let digest = Sha256::digest(verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(digest)
}

fn generate_oauth_random_string(length: usize) -> String {
    use rand::RngCore;

    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~";
    let mut bytes = vec![0_u8; length];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    bytes.into_iter().map(|value| ALPHABET[usize::from(value) % ALPHABET.len()] as char).collect()
}
