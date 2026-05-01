//! Client request envelopes and request payloads.

use crate::approval::{ApprovalDecision, ApprovalPolicy};
use crate::auth::ProviderAuthProfile;
use crate::checkpoint::CheckpointScope;
use crate::ids::{
    ApprovalId, ArtifactId, CheckpointId, MemoryFactId, NodeId, RequestId, ThreadId, TurnId,
    WorkspaceMountId,
};
use crate::interaction::InteractionContext;
use crate::memory::{ChannelRef, MemorySearchMode, MemoryTopicStatus, ParticipantRef};
use crate::memory_surface::{AboutYouUpdate, KnowledgeCorrection};
use crate::node::{NodePolicy, NodeStatus, WorkspaceMountPermissions};
use crate::sandbox::ShellSandboxRequest;
use crate::tool::ToolCallRequest;
use serde::{Deserialize, Serialize};

/// Describes what kind of user input started a turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnInputKind {
    /// The input came directly from the user.
    UserMessage,
    /// The input nudges an in-flight turn.
    SteeringNote,
    /// The input resumes a prior thread state.
    ResumeCommand,
}

/// The top-level client request envelope sent over the transport.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientRequestEnvelope {
    /// The request identifier used to correlate responses.
    pub request_id: RequestId,
    /// The typed request payload.
    #[serde(flatten)]
    pub request: ClientRequest,
}

/// All request types supported by protocol v0.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "method", content = "params")]
pub enum ClientRequest {
    /// Starts an OpenAI Codex OAuth login flow.
    #[serde(rename = "provider_auth/openai_codex_oauth_start")]
    OpenAiCodexOAuthStart(OpenAiCodexOAuthStartRequest),
    /// Completes an OpenAI Codex OAuth login flow and persists a profile.
    #[serde(rename = "provider_auth/openai_codex_oauth_complete")]
    OpenAiCodexOAuthComplete(OpenAiCodexOAuthCompleteRequest),
    /// Starts a GitLab OAuth login flow.
    #[serde(rename = "provider_auth/gitlab_oauth_start")]
    GitLabOAuthStart(GitLabOAuthStartRequest),
    /// Completes a GitLab OAuth login flow and persists a profile.
    #[serde(rename = "provider_auth/gitlab_oauth_complete")]
    GitLabOAuthComplete(GitLabOAuthCompleteRequest),
    /// Saves a GitLab personal access token as a provider auth profile.
    #[serde(rename = "provider_auth/gitlab_pat_save")]
    GitLabPatSave(GitLabPatSaveRequest),
    /// Starts a GitHub Copilot device-code login flow.
    #[serde(rename = "provider_auth/github_copilot_device_start")]
    GitHubCopilotDeviceStart(GitHubCopilotDeviceStartRequest),
    /// Polls a GitHub Copilot device-code login flow to completion.
    #[serde(rename = "provider_auth/github_copilot_device_poll")]
    GitHubCopilotDevicePoll(GitHubCopilotDevicePollRequest),
    /// Starts a MiniMax Portal OAuth login flow.
    #[serde(rename = "provider_auth/minimax_oauth_start")]
    MiniMaxOAuthStart(MiniMaxOAuthStartRequest),
    /// Polls a MiniMax Portal OAuth login flow to completion.
    #[serde(rename = "provider_auth/minimax_oauth_poll")]
    MiniMaxOAuthPoll(MiniMaxOAuthPollRequest),
    /// Lists persisted provider auth profiles.
    #[serde(rename = "provider_auth/list")]
    ProviderAuthList(ProviderAuthListRequest),
    /// Reads the effective model/provider readiness status.
    #[serde(rename = "model/status")]
    ModelStatus(ModelStatusRequest),
    /// Reads runtime execution settings.
    #[serde(rename = "runtime/config_get")]
    RuntimeConfigGet(RuntimeConfigGetRequest),
    /// Updates runtime execution settings for the current app-server process.
    #[serde(rename = "runtime/config_update")]
    RuntimeConfigUpdate(RuntimeConfigUpdateRequest),
    /// Creates or replaces a provider auth profile.
    #[serde(rename = "provider_auth/upsert")]
    ProviderAuthUpsert(ProviderAuthUpsertRequest),
    /// Deletes a provider auth profile.
    #[serde(rename = "provider_auth/delete")]
    ProviderAuthDelete(ProviderAuthDeleteRequest),
    /// Starts a new thread.
    #[serde(rename = "thread/start")]
    ThreadStart(ThreadStartRequest),
    /// Resumes an existing thread.
    #[serde(rename = "thread/resume")]
    ThreadResume(ThreadResumeRequest),
    /// Lists persisted threads for thread-picker style clients.
    #[serde(rename = "thread/list")]
    ThreadList(ThreadListRequest),
    /// Forks an existing thread into a new line of work.
    #[serde(rename = "thread/fork")]
    ThreadFork(ThreadForkRequest),
    /// Starts a new turn on a thread.
    #[serde(rename = "turn/start")]
    TurnStart(TurnStartRequest),
    /// Cancels a running turn.
    #[serde(rename = "turn/cancel")]
    TurnCancel(TurnCancelRequest),
    /// Responds to a pending approval.
    #[serde(rename = "approval/respond")]
    ApprovalRespond(ApprovalRespondRequest),
    /// Executes a runtime tool call.
    #[serde(rename = "tool/call")]
    ToolCall(ToolCallRequest),
    /// Rolls back a thread to a prior checkpoint.
    #[serde(rename = "thread/rollback")]
    ThreadRollback(ThreadRollbackRequest),
    /// Reads the current foreground memory wake-up for a thread.
    #[serde(rename = "memory/read_wakeup")]
    MemoryReadWakeup(MemoryReadWakeupRequest),
    /// Forces a foreground compilation pass for a thread.
    #[serde(rename = "memory/compile_now")]
    MemoryCompileNow(MemoryCompileNowRequest),
    /// Lists topics from the foreground memory topic index.
    #[serde(rename = "memory/list_topics")]
    MemoryListTopics(MemoryListTopicsRequest),
    /// Searches foreground and recent memory using a recall mode.
    #[serde(rename = "memory/search")]
    MemorySearch(MemorySearchRequest),
    /// Expands a thread session into recent evidence and artifacts.
    #[serde(rename = "memory/open_session")]
    MemoryOpenSession(MemoryOpenSessionRequest),
    /// Expands one fact or artifact citation into raw evidence.
    #[serde(rename = "memory/open_evidence")]
    MemoryOpenEvidence(MemoryOpenEvidenceRequest),
    /// Reads the owner-facing About You memory surface.
    #[serde(rename = "memory_surface/about_you/read")]
    MemorySurfaceAboutYouRead(MemorySurfaceAboutYouReadRequest),
    /// Updates the owner-facing About You memory surface.
    #[serde(rename = "memory_surface/about_you/update")]
    MemorySurfaceAboutYouUpdate(MemorySurfaceAboutYouUpdateRequest),
    /// Reads active work and commitments liz is carrying.
    #[serde(rename = "memory_surface/carrying/read")]
    MemorySurfaceCarryingRead(MemorySurfaceCarryingReadRequest),
    /// Lists owner-facing knowledge and decisions.
    #[serde(rename = "memory_surface/knowledge/list")]
    MemorySurfaceKnowledgeList(MemorySurfaceKnowledgeListRequest),
    /// Corrects a knowledge item.
    #[serde(rename = "memory_surface/knowledge/correct")]
    MemorySurfaceKnowledgeCorrect(MemorySurfaceKnowledgeCorrectRequest),
    /// Lists registered runtime nodes.
    #[serde(rename = "node/list")]
    NodeList(NodeListRequest),
    /// Reads one runtime node.
    #[serde(rename = "node/read")]
    NodeRead(NodeReadRequest),
    /// Updates node policy.
    #[serde(rename = "node/update_policy")]
    NodeUpdatePolicy(NodeUpdatePolicyRequest),
    /// Records a node heartbeat without creating a model turn.
    #[serde(rename = "node/heartbeat")]
    NodeHeartbeat(NodeHeartbeatRequest),
    /// Lists workspace mounts.
    #[serde(rename = "workspace_mount/list")]
    WorkspaceMountList(WorkspaceMountListRequest),
    /// Attaches a workspace mount.
    #[serde(rename = "workspace_mount/attach")]
    WorkspaceMountAttach(WorkspaceMountAttachRequest),
    /// Detaches a workspace mount.
    #[serde(rename = "workspace_mount/detach")]
    WorkspaceMountDetach(WorkspaceMountDetachRequest),
}

/// Starts an OpenAI Codex OAuth login flow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenAiCodexOAuthStartRequest {
    /// Redirect URI registered for the OAuth flow.
    pub redirect_uri: String,
    /// Optional OpenAI originator label to include in the authorize URL.
    pub originator: Option<String>,
}

/// Completes an OpenAI Codex OAuth login flow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenAiCodexOAuthCompleteRequest {
    /// Redirect URI registered for the OAuth flow.
    pub redirect_uri: String,
    /// The authorization code or the full redirect URL returned by OpenAI.
    pub code_or_redirect_url: String,
    /// PKCE verifier returned by the start request.
    pub code_verifier: String,
    /// Optional state generated by the start request and expected from the callback.
    pub expected_state: Option<String>,
    /// Optional profile id to persist on success.
    pub profile_id: Option<String>,
}

/// Starts a GitLab OAuth login flow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitLabOAuthStartRequest {
    /// GitLab instance URL, for example `https://gitlab.com`.
    pub instance_url: String,
    /// OAuth application client id.
    pub client_id: String,
    /// Redirect URI registered with the OAuth application.
    pub redirect_uri: String,
    /// Requested OAuth scopes.
    pub scopes: Vec<String>,
}

/// Completes a GitLab OAuth login flow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitLabOAuthCompleteRequest {
    /// GitLab instance URL, for example `https://gitlab.com`.
    pub instance_url: String,
    /// OAuth application client id.
    pub client_id: String,
    /// Optional OAuth application client secret.
    pub client_secret: Option<String>,
    /// Redirect URI registered with the OAuth application.
    pub redirect_uri: String,
    /// Authorization code returned from GitLab.
    pub code: String,
    /// Optional PKCE verifier used during authorize URL generation.
    pub code_verifier: Option<String>,
    /// Optional profile id to persist on success.
    pub profile_id: Option<String>,
}

/// Saves a GitLab personal access token as a provider auth profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitLabPatSaveRequest {
    /// Optional GitLab instance URL, for example `https://gitlab.com`.
    pub instance_url: Option<String>,
    /// The personal access token to persist.
    pub token: String,
    /// Optional profile id to persist.
    pub profile_id: Option<String>,
    /// Optional display label for the stored profile.
    pub display_name: Option<String>,
}

/// Starts a GitHub Copilot device-code login flow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitHubCopilotDeviceStartRequest {
    /// Optional GitHub Enterprise URL or domain.
    pub enterprise_url: Option<String>,
}

/// Polls a GitHub Copilot device-code login flow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitHubCopilotDevicePollRequest {
    /// The device code obtained from `provider_auth/github_copilot_device_start`.
    pub device_code: String,
    /// Optional GitHub Enterprise URL or domain.
    pub enterprise_url: Option<String>,
    /// Optional polling interval hint returned from the device-code start call.
    pub interval_seconds: Option<u32>,
    /// Optional profile id to persist when authorization completes.
    pub profile_id: Option<String>,
}

/// Starts a MiniMax Portal OAuth login flow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MiniMaxOAuthStartRequest {
    /// Selected MiniMax region, either `global` or `cn`.
    pub region: String,
}

/// Polls a MiniMax Portal OAuth login flow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MiniMaxOAuthPollRequest {
    /// One-time user code returned from the start request.
    pub user_code: String,
    /// PKCE verifier returned from the start request.
    pub code_verifier: String,
    /// Selected MiniMax region, either `global` or `cn`.
    pub region: String,
    /// Optional polling interval hint returned from the start request.
    pub interval_ms: Option<u32>,
    /// Optional profile id to persist when authorization completes.
    pub profile_id: Option<String>,
}

/// Lists provider auth profiles, optionally scoped to one provider.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderAuthListRequest {
    /// Optional provider identifier filter.
    pub provider_id: Option<String>,
}

/// Reads effective model/provider readiness without exposing credentials.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelStatusRequest {}

/// Reads the current runtime execution configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeConfigGetRequest {}

/// Updates runtime execution configuration for new tool calls.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeConfigUpdateRequest {
    /// The default shell sandbox to apply when tool calls do not provide an override.
    pub sandbox: Option<ShellSandboxRequest>,
    /// The approval policy to apply to new high-risk actions.
    pub approval_policy: Option<ApprovalPolicy>,
}

/// Creates or replaces a provider auth profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderAuthUpsertRequest {
    /// The full profile payload to persist.
    pub profile: ProviderAuthProfile,
}

/// Deletes a provider auth profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderAuthDeleteRequest {
    /// The profile identifier to delete.
    pub profile_id: String,
}

/// Starts a new thread.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreadStartRequest {
    /// An optional user-visible thread title.
    pub title: Option<String>,
    /// The initial goal for the thread.
    pub initial_goal: Option<String>,
    /// An optional workspace locator associated with the thread.
    pub workspace_ref: Option<String>,
    /// An optional workspace mount associated with the thread.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_mount_id: Option<WorkspaceMountId>,
}

/// Resumes an existing thread.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreadResumeRequest {
    /// The identifier of the thread to resume.
    pub thread_id: ThreadId,
}

/// Lists persisted threads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreadListRequest {
    /// Optional status filter for the returned threads.
    pub status: Option<crate::thread::ThreadStatus>,
    /// Optional result limit.
    pub limit: Option<usize>,
}

/// Forks an existing thread.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreadForkRequest {
    /// The thread to fork from.
    pub thread_id: ThreadId,
    /// An optional title for the forked thread.
    pub title: Option<String>,
    /// The reason the fork is being created.
    pub fork_reason: Option<String>,
}

/// Starts a new turn.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurnStartRequest {
    /// The parent thread for the new turn.
    pub thread_id: ThreadId,
    /// The raw turn input text.
    pub input: String,
    /// The kind of input that triggered the turn.
    pub input_kind: TurnInputKind,
    /// The channel that produced this turn, if the client knows it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel: Option<ChannelRef>,
    /// The participant currently talking to liz, if the client knows it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub participant: Option<ParticipantRef>,
    /// The resolved interaction context, when the client can provide it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interaction_context: Option<InteractionContext>,
}

/// Cancels an existing turn.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurnCancelRequest {
    /// The thread that owns the turn.
    pub thread_id: ThreadId,
    /// The turn to cancel.
    pub turn_id: TurnId,
}

/// Responds to an approval request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalRespondRequest {
    /// The approval request to resolve.
    pub approval_id: ApprovalId,
    /// The decision that resolves the approval.
    pub decision: ApprovalDecision,
}

/// Rolls back a thread.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreadRollbackRequest {
    /// The thread to roll back.
    pub thread_id: ThreadId,
    /// The specific checkpoint to restore, if not restoring the latest one.
    pub target_checkpoint_id: Option<CheckpointId>,
    /// The restore scope for the rollback.
    pub rollback_scope: CheckpointScope,
}

/// Reads the current wake-up payload for a thread.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryReadWakeupRequest {
    /// The thread whose wake-up payload should be read.
    pub thread_id: ThreadId,
}

/// Forces a foreground compilation pass for a thread.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryCompileNowRequest {
    /// The thread whose memory should be compiled.
    pub thread_id: ThreadId,
}

/// Lists topics from the topic index.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryListTopicsRequest {
    /// Optional topic status filter.
    pub status: Option<MemoryTopicStatus>,
    /// Optional result limit.
    pub limit: Option<usize>,
}

/// Searches memory using a specific recall mode.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemorySearchRequest {
    /// The query to search for.
    pub query: String,
    /// The recall mode used for ranking.
    pub mode: MemorySearchMode,
    /// Optional result limit.
    pub limit: Option<usize>,
}

/// Expands one session into recent log entries and artifacts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryOpenSessionRequest {
    /// The thread to expand.
    pub thread_id: ThreadId,
}

/// Expands one fact or artifact citation into raw evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryOpenEvidenceRequest {
    /// The thread that owns the evidence.
    pub thread_id: ThreadId,
    /// The turn associated with the evidence, if any.
    pub turn_id: Option<TurnId>,
    /// The artifact to expand, if any.
    pub artifact_id: Option<ArtifactId>,
    /// The compiled fact to expand, if any.
    pub fact_id: Option<MemoryFactId>,
}

/// Reads the About You surface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemorySurfaceAboutYouReadRequest {}

/// Updates the About You surface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemorySurfaceAboutYouUpdateRequest {
    /// Replacement surface values.
    pub update: AboutYouUpdate,
}

/// Reads the What We're Carrying surface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemorySurfaceCarryingReadRequest {
    /// Optional result limit.
    pub limit: Option<usize>,
}

/// Lists user-facing knowledge and decisions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemorySurfaceKnowledgeListRequest {
    /// Optional result limit.
    pub limit: Option<usize>,
}

/// Corrects one knowledge item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemorySurfaceKnowledgeCorrectRequest {
    /// Correction payload.
    pub correction: KnowledgeCorrection,
}

/// Lists runtime nodes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeListRequest {}

/// Reads one runtime node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeReadRequest {
    /// The node to read.
    pub node_id: NodeId,
}

/// Updates node policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeUpdatePolicyRequest {
    /// The node to update.
    pub node_id: NodeId,
    /// Replacement policy.
    pub policy: NodePolicy,
}

/// Records node liveness and runtime version state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeHeartbeatRequest {
    /// The node that sent the heartbeat.
    pub node_id: NodeId,
    /// Replacement liveness status.
    pub status: NodeStatus,
}

/// Lists workspace mounts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceMountListRequest {
    /// Optional node filter.
    pub node_id: Option<NodeId>,
}

/// Attaches a workspace mount.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceMountAttachRequest {
    /// The node that owns the path.
    pub node_id: NodeId,
    /// Root path on that node.
    pub root_path: String,
    /// Owner-facing label.
    pub label: Option<String>,
    /// Mount permissions.
    pub permissions: WorkspaceMountPermissions,
}

/// Detaches a workspace mount.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceMountDetachRequest {
    /// The workspace mount to detach.
    pub workspace_id: WorkspaceMountId,
}
