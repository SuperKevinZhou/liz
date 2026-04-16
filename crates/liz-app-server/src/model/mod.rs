//! Model-layer integration for provider-aware streaming turns.

pub mod adapters;
mod auth;
mod capabilities;
mod config;
mod family;
mod gateway;
mod http;
mod invocation;
mod normalized_stream;
mod provider_spec;
mod registry;

pub use capabilities::ModelCapabilities;
pub use auth::{
    build_openai_codex_authorize_url, exchange_openai_codex_authorization_code,
    exchange_gitlab_oauth_code, resolve_gitlab_oauth_runtime_auth,
    poll_minimax_oauth_authorization, refresh_minimax_oauth_token,
    resolve_minimax_oauth_runtime_auth, start_minimax_oauth_authorization,
    normalize_openai_codex_authorize_url, poll_github_copilot_device_authorization,
    refresh_openai_codex_token, resolve_copilot_runtime_auth,
    resolve_openai_codex_runtime_auth, resolve_openai_codex_stable_subject,
    start_github_copilot_device_authorization, start_gitlab_oauth_authorization,
    CopilotRuntimeAuth, GitHubCopilotDeviceCodeAuth, GitHubCopilotDevicePollOutcome,
    GitLabOAuthRuntimeAuth, GitLabOAuthStartAuth, MiniMaxOAuthDeviceCodeAuth,
    MiniMaxOAuthPollOutcome, MiniMaxOAuthRuntimeAuth,
    OpenAiCodexRuntimeAuth, OpenAiCodexRuntimeAuthRequest,
};
pub use config::{ModelGatewayConfig, ProviderOverride, ResolvedProvider};
pub use family::ModelProviderFamily;
pub use gateway::{ModelError, ModelGateway, ModelRunSummary, ModelTurnRequest};
pub use invocation::{InvocationTransport, ProviderInvocationPlan};
pub use normalized_stream::{NormalizedTurnEvent, UsageDelta};
pub use provider_spec::{ProviderAuthKind, ProviderSpec};
pub use registry::ProviderRegistry;
