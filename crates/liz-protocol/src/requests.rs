//! Client request envelopes and request payloads.

use crate::approval::ApprovalDecision;
use crate::auth::ProviderAuthProfile;
use crate::checkpoint::CheckpointScope;
use crate::ids::{ApprovalId, CheckpointId, RequestId, ThreadId, TurnId};
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
    /// Lists persisted provider auth profiles.
    #[serde(rename = "provider_auth/list")]
    ProviderAuthList(ProviderAuthListRequest),
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
    /// Rolls back a thread to a prior checkpoint.
    #[serde(rename = "thread/rollback")]
    ThreadRollback(ThreadRollbackRequest),
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

/// Lists provider auth profiles, optionally scoped to one provider.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderAuthListRequest {
    /// Optional provider identifier filter.
    pub provider_id: Option<String>,
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
}

/// Resumes an existing thread.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreadResumeRequest {
    /// The identifier of the thread to resume.
    pub thread_id: ThreadId,
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
