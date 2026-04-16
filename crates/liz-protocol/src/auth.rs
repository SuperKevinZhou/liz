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
