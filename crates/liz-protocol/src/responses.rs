//! Server response envelopes and typed response payloads.

use crate::approval::ApprovalPolicy;
use crate::approval::ApprovalRequest;
use crate::auth::{
    GitHubCopilotDeviceCode, GitHubCopilotDevicePollStatus, GitLabOAuthStart,
    MiniMaxOAuthDeviceCode, MiniMaxOAuthPollStatus, OpenAiCodexOAuthStart, ProviderAuthProfile,
};
use crate::checkpoint::{Checkpoint, CheckpointScope};
use crate::ids::{RequestId, ThreadId, WorkspaceMountId};
use crate::interaction::InboundEventAction;
use crate::memory::{
    MemoryCompilationSummary, MemoryEvidenceView, MemorySearchHit, MemorySearchMode,
    MemorySessionView, MemoryTopicSummary, MemoryWakeup, RecentConversationWakeupView,
    ResumeSummary,
};
use crate::memory_surface::{AboutYouSurface, CarryingSurface, KnowledgeItem, KnowledgeSurface};
use crate::node::{NodeRecord, WorkspaceMount};
use crate::sandbox::ShellSandboxSummary;
use crate::thread::Thread;
use crate::tool::ToolCallResponse;
use crate::turn::Turn;
use serde::{Deserialize, Serialize};

/// The top-level server response envelope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ServerResponseEnvelope {
    /// A successful response.
    Success(Box<SuccessResponseEnvelope>),
    /// A failed response.
    Error(ErrorResponseEnvelope),
}

/// A successful response to a client request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SuccessResponseEnvelope {
    /// Always `true` for successful responses.
    pub ok: bool,
    /// The request identifier being acknowledged.
    pub request_id: RequestId,
    /// The typed response payload.
    #[serde(flatten)]
    pub response: ResponsePayload,
}

/// A failed response to a client request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorResponseEnvelope {
    /// Always `false` for failed responses.
    pub ok: bool,
    /// The request identifier being acknowledged.
    pub request_id: RequestId,
    /// The structured error payload.
    pub error: ResponseError,
}

/// A structured response error payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResponseError {
    /// The stable machine-readable error code.
    pub code: String,
    /// The human-readable error message.
    pub message: String,
    /// Whether retrying the request may succeed.
    pub retryable: bool,
}

/// The typed successful response payloads supported by protocol v0.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "method", content = "data")]
pub enum ResponsePayload {
    /// Acknowledges `provider_auth/openai_codex_oauth_start`.
    #[serde(rename = "provider_auth/openai_codex_oauth_start")]
    OpenAiCodexOAuthStart(OpenAiCodexOAuthStartResponse),
    /// Acknowledges `provider_auth/openai_codex_oauth_complete`.
    #[serde(rename = "provider_auth/openai_codex_oauth_complete")]
    OpenAiCodexOAuthComplete(OpenAiCodexOAuthCompleteResponse),
    /// Acknowledges `provider_auth/gitlab_oauth_start`.
    #[serde(rename = "provider_auth/gitlab_oauth_start")]
    GitLabOAuthStart(GitLabOAuthStartResponse),
    /// Acknowledges `provider_auth/gitlab_oauth_complete`.
    #[serde(rename = "provider_auth/gitlab_oauth_complete")]
    GitLabOAuthComplete(GitLabOAuthCompleteResponse),
    /// Acknowledges `provider_auth/gitlab_pat_save`.
    #[serde(rename = "provider_auth/gitlab_pat_save")]
    GitLabPatSave(GitLabPatSaveResponse),
    /// Acknowledges `provider_auth/github_copilot_device_start`.
    #[serde(rename = "provider_auth/github_copilot_device_start")]
    GitHubCopilotDeviceStart(GitHubCopilotDeviceStartResponse),
    /// Acknowledges `provider_auth/github_copilot_device_poll`.
    #[serde(rename = "provider_auth/github_copilot_device_poll")]
    GitHubCopilotDevicePoll(GitHubCopilotDevicePollResponse),
    /// Acknowledges `provider_auth/minimax_oauth_start`.
    #[serde(rename = "provider_auth/minimax_oauth_start")]
    MiniMaxOAuthStart(MiniMaxOAuthStartResponse),
    /// Acknowledges `provider_auth/minimax_oauth_poll`.
    #[serde(rename = "provider_auth/minimax_oauth_poll")]
    MiniMaxOAuthPoll(MiniMaxOAuthPollResponse),
    /// Acknowledges `provider_auth/list`.
    #[serde(rename = "provider_auth/list")]
    ProviderAuthList(ProviderAuthListResponse),
    /// Acknowledges `model/status`.
    #[serde(rename = "model/status")]
    ModelStatus(ModelStatusResponse),
    /// Acknowledges `runtime/config_get` and `runtime/config_update`.
    #[serde(rename = "runtime/config")]
    RuntimeConfig(RuntimeConfigResponse),
    /// Acknowledges `provider_auth/upsert`.
    #[serde(rename = "provider_auth/upsert")]
    ProviderAuthUpsert(ProviderAuthUpsertResponse),
    /// Acknowledges `provider_auth/delete`.
    #[serde(rename = "provider_auth/delete")]
    ProviderAuthDelete(ProviderAuthDeleteResponse),
    /// Acknowledges `thread/start`.
    #[serde(rename = "thread/start")]
    ThreadStart(ThreadStartResponse),
    /// Acknowledges `thread/resume`.
    #[serde(rename = "thread/resume")]
    ThreadResume(ThreadResumeResponse),
    /// Acknowledges `thread/list`.
    #[serde(rename = "thread/list")]
    ThreadList(ThreadListResponse),
    /// Acknowledges `thread/fork`.
    #[serde(rename = "thread/fork")]
    ThreadFork(ThreadForkResponse),
    /// Acknowledges `turn/start`.
    #[serde(rename = "turn/start")]
    TurnStart(TurnStartResponse),
    /// Acknowledges `turn/cancel`.
    #[serde(rename = "turn/cancel")]
    TurnCancel(TurnCancelResponse),
    /// Acknowledges `approval/respond`.
    #[serde(rename = "approval/respond")]
    ApprovalRespond(ApprovalRespondResponse),
    /// Acknowledges `tool/call`.
    #[serde(rename = "tool/call")]
    ToolCall(ToolCallResponse),
    /// Acknowledges `thread/rollback`.
    #[serde(rename = "thread/rollback")]
    ThreadRollback(ThreadRollbackResponse),
    /// Acknowledges `memory/read_wakeup`.
    #[serde(rename = "memory/read_wakeup")]
    MemoryReadWakeup(MemoryReadWakeupResponse),
    /// Acknowledges `memory/compile_now`.
    #[serde(rename = "memory/compile_now")]
    MemoryCompileNow(MemoryCompileNowResponse),
    /// Acknowledges `memory/list_topics`.
    #[serde(rename = "memory/list_topics")]
    MemoryListTopics(MemoryListTopicsResponse),
    /// Acknowledges `memory/search`.
    #[serde(rename = "memory/search")]
    MemorySearch(MemorySearchResponse),
    /// Acknowledges `memory/open_session`.
    #[serde(rename = "memory/open_session")]
    MemoryOpenSession(MemoryOpenSessionResponse),
    /// Acknowledges `memory/open_evidence`.
    #[serde(rename = "memory/open_evidence")]
    MemoryOpenEvidence(MemoryOpenEvidenceResponse),
    /// Acknowledges `memory_surface/about_you/read`.
    #[serde(rename = "memory_surface/about_you/read")]
    MemorySurfaceAboutYouRead(MemorySurfaceAboutYouReadResponse),
    /// Acknowledges `memory_surface/about_you/update`.
    #[serde(rename = "memory_surface/about_you/update")]
    MemorySurfaceAboutYouUpdate(MemorySurfaceAboutYouUpdateResponse),
    /// Acknowledges `memory_surface/carrying/read`.
    #[serde(rename = "memory_surface/carrying/read")]
    MemorySurfaceCarryingRead(MemorySurfaceCarryingReadResponse),
    /// Acknowledges `memory_surface/knowledge/list`.
    #[serde(rename = "memory_surface/knowledge/list")]
    MemorySurfaceKnowledgeList(MemorySurfaceKnowledgeListResponse),
    /// Acknowledges `memory_surface/knowledge/correct`.
    #[serde(rename = "memory_surface/knowledge/correct")]
    MemorySurfaceKnowledgeCorrect(MemorySurfaceKnowledgeCorrectResponse),
    /// Acknowledges `node/list`.
    #[serde(rename = "node/list")]
    NodeList(NodeListResponse),
    /// Acknowledges `node/read`.
    #[serde(rename = "node/read")]
    NodeRead(NodeReadResponse),
    /// Acknowledges `node/update_policy`.
    #[serde(rename = "node/update_policy")]
    NodeUpdatePolicy(NodeUpdatePolicyResponse),
    /// Acknowledges `node/heartbeat`.
    #[serde(rename = "node/heartbeat")]
    NodeHeartbeat(NodeHeartbeatResponse),
    /// Acknowledges `workspace_mount/list`.
    #[serde(rename = "workspace_mount/list")]
    WorkspaceMountList(WorkspaceMountListResponse),
    /// Acknowledges `workspace_mount/attach`.
    #[serde(rename = "workspace_mount/attach")]
    WorkspaceMountAttach(WorkspaceMountAttachResponse),
    /// Acknowledges `workspace_mount/detach`.
    #[serde(rename = "workspace_mount/detach")]
    WorkspaceMountDetach(WorkspaceMountDetachResponse),
}

/// The response payload for `provider_auth/openai_codex_oauth_start`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenAiCodexOAuthStartResponse {
    /// The OAuth bootstrap data that the client should open and preserve.
    pub oauth: OpenAiCodexOAuthStart,
}

/// The response payload for `provider_auth/openai_codex_oauth_complete`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenAiCodexOAuthCompleteResponse {
    /// The persisted provider auth profile.
    pub profile: ProviderAuthProfile,
}

/// The response payload for `provider_auth/gitlab_oauth_start`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitLabOAuthStartResponse {
    /// The OAuth bootstrap data that the client should open and preserve.
    pub oauth: GitLabOAuthStart,
}

/// The response payload for `provider_auth/gitlab_oauth_complete`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitLabOAuthCompleteResponse {
    /// The persisted provider auth profile.
    pub profile: ProviderAuthProfile,
}

/// The response payload for `provider_auth/gitlab_pat_save`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitLabPatSaveResponse {
    /// The persisted provider auth profile.
    pub profile: ProviderAuthProfile,
}

/// The response payload for `provider_auth/github_copilot_device_start`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitHubCopilotDeviceStartResponse {
    /// The device-code bootstrap information.
    pub device: GitHubCopilotDeviceCode,
}

/// The response payload for `provider_auth/github_copilot_device_poll`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitHubCopilotDevicePollResponse {
    /// The current polling status.
    pub status: GitHubCopilotDevicePollStatus,
    /// Suggested retry delay in seconds when polling should continue.
    pub retry_after_seconds: Option<u32>,
    /// The persisted profile when authorization completed.
    pub profile: Option<ProviderAuthProfile>,
}

/// The response payload for `provider_auth/minimax_oauth_start`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MiniMaxOAuthStartResponse {
    /// The device-code bootstrap information.
    pub device: MiniMaxOAuthDeviceCode,
}

/// The response payload for `provider_auth/minimax_oauth_poll`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MiniMaxOAuthPollResponse {
    /// The current polling status.
    pub status: MiniMaxOAuthPollStatus,
    /// Suggested retry delay in milliseconds when polling should continue.
    pub retry_after_ms: Option<u32>,
    /// The persisted profile when authorization completed.
    pub profile: Option<ProviderAuthProfile>,
}

/// The response payload for `provider_auth/list`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderAuthListResponse {
    /// The matching persisted profiles.
    pub profiles: Vec<ProviderAuthProfile>,
}

/// The response payload for `model/status`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelStatusResponse {
    /// The configured primary provider identifier.
    pub provider_id: String,
    /// The provider display name, when the provider is known.
    pub display_name: Option<String>,
    /// The resolved model identifier, when the provider is known.
    pub model_id: Option<String>,
    /// The provider auth mode label.
    pub auth_kind: Option<String>,
    /// Whether the provider can be used for a live model turn now.
    pub ready: bool,
    /// Whether a usable credential is present or not required.
    pub credential_configured: bool,
    /// Safe environment/config hints that can satisfy the provider.
    pub credential_hints: Vec<String>,
    /// User-visible readiness notes.
    pub notes: Vec<String>,
}

/// The response payload for runtime execution configuration requests.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeConfigResponse {
    /// The default shell sandbox used when a tool call does not provide an override.
    pub sandbox: ShellSandboxSummary,
    /// The current approval policy for high-risk actions.
    pub approval_policy: ApprovalPolicy,
}

/// The response payload for `provider_auth/upsert`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderAuthUpsertResponse {
    /// The persisted auth profile after write.
    pub profile: ProviderAuthProfile,
}

/// The response payload for `provider_auth/delete`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderAuthDeleteResponse {
    /// The deleted profile identifier.
    pub profile_id: String,
}

/// The response payload for `thread/start`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreadStartResponse {
    /// The created thread.
    pub thread: Thread,
}

/// The response payload for `thread/resume`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreadResumeResponse {
    /// The resumed thread.
    pub thread: Thread,
    /// The concise resume summary for the thread.
    pub resume_summary: Option<ResumeSummary>,
}

/// The response payload for `thread/list`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreadListResponse {
    /// Threads ordered for client-side picking surfaces.
    pub threads: Vec<Thread>,
}

/// The response payload for `thread/fork`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreadForkResponse {
    /// The forked thread.
    pub thread: Thread,
}

/// The response payload for `turn/start`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurnStartResponse {
    /// The started turn.
    pub turn: Turn,
}

/// The response payload for `turn/cancel`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurnCancelResponse {
    /// The cancelled turn after state projection.
    pub turn: Turn,
}

/// The response payload for `approval/respond`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalRespondResponse {
    /// The approval after applying the user decision.
    pub approval: ApprovalRequest,
}

/// The response payload for `thread/rollback`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreadRollbackResponse {
    /// The updated thread after rollback.
    pub thread: Thread,
    /// The checkpoint that was restored, if one was resolved.
    pub restored_checkpoint: Option<Checkpoint>,
    /// The scope that was restored by the rollback.
    pub rollback_scope: CheckpointScope,
}

/// The response payload for `memory/read_wakeup`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryReadWakeupResponse {
    /// The thread whose wake-up payload was read.
    pub thread_id: ThreadId,
    /// The resident wake-up slice for that thread.
    pub wakeup: MemoryWakeup,
    /// The recent-conversation wake-up view for that thread.
    pub recent_conversation: RecentConversationWakeupView,
}

/// The response payload for `memory/compile_now`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryCompileNowResponse {
    /// The thread whose memory was compiled.
    pub thread_id: ThreadId,
    /// The summary of the compilation result.
    pub compilation: MemoryCompilationSummary,
}

/// The response payload for `memory/list_topics`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryListTopicsResponse {
    /// Topics returned from the topic index.
    pub topics: Vec<MemoryTopicSummary>,
}

/// The response payload for `memory/search`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemorySearchResponse {
    /// The query that was executed.
    pub query: String,
    /// The recall mode used for the search.
    pub mode: MemorySearchMode,
    /// The ordered hits returned by the search.
    pub hits: Vec<MemorySearchHit>,
}

/// The response payload for `memory/open_session`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryOpenSessionResponse {
    /// The expanded session view.
    pub session: MemorySessionView,
}

/// The response payload for `memory/open_evidence`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryOpenEvidenceResponse {
    /// The expanded evidence view.
    pub evidence: MemoryEvidenceView,
}

/// The response payload for `memory_surface/about_you/read`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemorySurfaceAboutYouReadResponse {
    /// The owner-facing About You surface.
    pub surface: AboutYouSurface,
}

/// The response payload for `memory_surface/about_you/update`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemorySurfaceAboutYouUpdateResponse {
    /// The updated About You surface.
    pub surface: AboutYouSurface,
}

/// The response payload for `memory_surface/carrying/read`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemorySurfaceCarryingReadResponse {
    /// The owner-facing active work surface.
    pub surface: CarryingSurface,
}

/// The response payload for `memory_surface/knowledge/list`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemorySurfaceKnowledgeListResponse {
    /// The owner-facing knowledge surface.
    pub surface: KnowledgeSurface,
}

/// The response payload for `memory_surface/knowledge/correct`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemorySurfaceKnowledgeCorrectResponse {
    /// The corrected knowledge item.
    pub item: KnowledgeItem,
}

/// The response payload for `node/list`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeListResponse {
    /// Registered nodes.
    pub nodes: Vec<NodeRecord>,
}

/// The response payload for `node/read`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeReadResponse {
    /// The requested node.
    pub node: NodeRecord,
}

/// The response payload for `node/update_policy`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeUpdatePolicyResponse {
    /// The updated node.
    pub node: NodeRecord,
}

/// The response payload for `node/heartbeat`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeHeartbeatResponse {
    /// The node after applying heartbeat state.
    pub node: NodeRecord,
    /// The inbound classification action used for the heartbeat.
    pub action: InboundEventAction,
}

/// The response payload for `workspace_mount/list`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceMountListResponse {
    /// Workspace mounts.
    pub mounts: Vec<WorkspaceMount>,
}

/// The response payload for `workspace_mount/attach`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceMountAttachResponse {
    /// The attached workspace mount.
    pub mount: WorkspaceMount,
}

/// The response payload for `workspace_mount/detach`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceMountDetachResponse {
    /// The detached workspace identifier.
    pub workspace_id: WorkspaceMountId,
}
