//! High-level runtime coordination for thread and turn lifecycle work.

use crate::runtime::context_assembler::{AssembledContext, ContextAssembler};
use crate::runtime::error::{RuntimeError, RuntimeResult};
use crate::runtime::ids::IdGenerator;
use crate::runtime::policy_engine::{PolicyDecision, PolicyEngine};
use crate::runtime::stores::RuntimeStores;
use crate::runtime::thread_manager::ThreadManager;
use crate::runtime::turn_manager::TurnManager;
use crate::storage::StoredArtifact;
use liz_protocol::memory::ResumeSummary;
use liz_protocol::requests::{
    ApprovalRespondRequest, ProviderAuthDeleteRequest, ProviderAuthListRequest,
    ProviderAuthUpsertRequest, ThreadForkRequest, ThreadResumeRequest, ThreadStartRequest,
    TurnCancelRequest, TurnStartRequest,
};
use liz_protocol::responses::{
    ApprovalRespondResponse, ProviderAuthDeleteResponse, ProviderAuthListResponse,
    ProviderAuthUpsertResponse, ThreadForkResponse, ThreadResumeResponse, ThreadStartResponse,
    TurnCancelResponse, TurnStartResponse,
};
use liz_protocol::{
    ApprovalDecision, ApprovalRequest, ApprovalStatus, ArtifactKind, ArtifactRef, Checkpoint,
    CheckpointScope, ProviderAuthProfile, Thread, ThreadId, Turn, TurnId,
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

    /// Resumes a thread and returns the current wake-up projection.
    pub fn resume_thread(
        &mut self,
        request: ThreadResumeRequest,
    ) -> RuntimeResult<ThreadResumeResponse> {
        let thread = self.thread_manager.resume_thread(&self.stores, request)?;
        let resume_summary = Some(self.build_resume_summary(&thread));
        Ok(ThreadResumeResponse { thread, resume_summary })
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
        let thread = self
            .stores
            .get_thread(thread_id)?
            .ok_or_else(|| RuntimeError::not_found("thread_not_found", "thread does not exist"))?;
        let snapshot = self.stores.read_global_memory()?;
        Ok(self.context_assembler.assemble(&snapshot, &thread, input))
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
                decision.sandbox_context.filesystem_mode,
                decision.sandbox_context.writable_roots.join(","),
                decision.sandbox_context.network_access
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
