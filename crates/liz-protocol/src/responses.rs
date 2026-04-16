//! Server response envelopes and typed response payloads.

use crate::approval::ApprovalRequest;
use crate::auth::{
    GitHubCopilotDeviceCode, GitHubCopilotDevicePollStatus, ProviderAuthProfile,
};
use crate::checkpoint::{Checkpoint, CheckpointScope};
use crate::ids::{RequestId, ThreadId};
use crate::memory::{MemoryCompilationSummary, ResumeSummary};
use crate::thread::Thread;
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
    /// Acknowledges `provider_auth/github_copilot_device_start`.
    #[serde(rename = "provider_auth/github_copilot_device_start")]
    GitHubCopilotDeviceStart(GitHubCopilotDeviceStartResponse),
    /// Acknowledges `provider_auth/github_copilot_device_poll`.
    #[serde(rename = "provider_auth/github_copilot_device_poll")]
    GitHubCopilotDevicePoll(GitHubCopilotDevicePollResponse),
    /// Acknowledges `provider_auth/list`.
    #[serde(rename = "provider_auth/list")]
    ProviderAuthList(ProviderAuthListResponse),
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
    /// Acknowledges `thread/rollback`.
    #[serde(rename = "thread/rollback")]
    ThreadRollback(ThreadRollbackResponse),
    /// Acknowledges `memory/compile_now`.
    #[serde(rename = "memory/compile_now")]
    MemoryCompileNow(MemoryCompileNowResponse),
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

/// The response payload for `provider_auth/list`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderAuthListResponse {
    /// The matching persisted profiles.
    pub profiles: Vec<ProviderAuthProfile>,
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

/// The response payload for `memory/compile_now`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryCompileNowResponse {
    /// The thread whose memory was compiled.
    pub thread_id: ThreadId,
    /// The summary of the compilation result.
    pub compilation: MemoryCompilationSummary,
}
