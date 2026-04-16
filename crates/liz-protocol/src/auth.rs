//! Provider auth profile payloads shared across clients and servers.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A persisted provider auth profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderAuthProfile {
    /// Stable profile identifier.
    pub profile_id: String,
    /// Stable provider identifier that owns this credential.
    pub provider_id: String,
    /// Optional human-facing profile label.
    pub display_name: Option<String>,
    /// Typed credential payload.
    pub credential: ProviderCredential,
}

/// A typed provider credential payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProviderCredential {
    /// Static API key or bearer token credential.
    ApiKey {
        /// Secret API key or bearer token value.
        api_key: String,
    },
    /// OAuth-style credential with optional refresh metadata.
    OAuth {
        /// Access token used for runtime requests.
        access_token: String,
        /// Optional refresh token when the provider supports refresh.
        refresh_token: Option<String>,
        /// Optional expiry timestamp in milliseconds since epoch.
        expires_at_ms: Option<u64>,
        /// Optional provider-specific account identifier.
        account_id: Option<String>,
        /// Optional email or principal identity.
        email: Option<String>,
    },
    /// Generic opaque token credential.
    Token {
        /// Token value used for runtime requests.
        token: String,
        /// Optional expiry timestamp in milliseconds since epoch.
        expires_at_ms: Option<u64>,
        /// Additional provider-specific metadata.
        metadata: BTreeMap<String, String>,
    },
}

/// Device-code login bootstrap information for GitHub Copilot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitHubCopilotDeviceCode {
    /// Verification URL the user should open in a browser.
    pub verification_uri: String,
    /// One-time user code the user should enter in the browser flow.
    pub user_code: String,
    /// Opaque device code used for polling.
    pub device_code: String,
    /// Polling interval in seconds suggested by the provider.
    pub interval_seconds: u32,
    /// Final Copilot API base URL derived from the chosen deployment.
    pub api_base_url: String,
}

/// The polling status returned by GitHub Copilot device-code completion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GitHubCopilotDevicePollStatus {
    /// Authorization is still pending.
    Pending,
    /// The caller should back off and poll more slowly.
    SlowDown,
    /// Authorization completed successfully and a profile was stored.
    Complete,
}

/// Device-code bootstrap data for a MiniMax Portal OAuth flow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MiniMaxOAuthDeviceCode {
    /// Verification URL the user should open.
    pub verification_uri: String,
    /// One-time user code shown by MiniMax.
    pub user_code: String,
    /// PKCE verifier retained until token exchange.
    pub code_verifier: String,
    /// Polling interval in milliseconds suggested by MiniMax.
    pub interval_ms: u32,
    /// Expiry timestamp in milliseconds since epoch.
    pub expires_at_ms: u64,
    /// Selected MiniMax region.
    pub region: String,
}

/// The polling status returned by MiniMax Portal OAuth completion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MiniMaxOAuthPollStatus {
    /// Authorization is still pending.
    Pending,
    /// Authorization completed successfully and a profile was stored.
    Complete,
}

/// OAuth bootstrap data for a GitLab authorization flow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitLabOAuthStart {
    /// Final authorize URL the client should open.
    pub authorize_url: String,
    /// CSRF state value that must round-trip through the callback.
    pub state: String,
    /// PKCE verifier that should be retained until code exchange.
    pub code_verifier: String,
}

/// OAuth bootstrap data for an OpenAI Codex authorization flow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenAiCodexOAuthStart {
    /// Final authorize URL the client should open.
    pub authorize_url: String,
    /// CSRF state value that must round-trip through the callback.
    pub state: String,
    /// PKCE verifier that should be retained until code exchange.
    pub code_verifier: String,
}
