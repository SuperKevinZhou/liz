//! Provider specification types used by the builtin registry.

use crate::model::capabilities::ModelCapabilities;
use crate::model::family::ModelProviderFamily;
use std::collections::BTreeMap;

/// The primary auth mode exposed by a provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum ProviderAuthKind {
    /// Plain API key or bearer-token auth.
    ApiKey,
    /// OAuth or ChatGPT-style login.
    OAuth,
    /// Device-code auth with a follow-up token exchange.
    DeviceCode,
    /// AWS credential chain or Bedrock bearer token.
    AwsCredentialChain,
    /// Google ADC / service-account auth.
    GoogleApplicationDefault,
    /// Structured service-key auth such as SAP AI Core.
    ServiceKey,
    /// Local runtime with no remote credential.
    Local,
    /// Multiple auth modes are supported depending on the deployment.
    Hybrid,
}

impl ProviderAuthKind {
    /// Returns a short stable auth label used in diagnostics.
    pub fn label(self) -> &'static str {
        match self {
            Self::ApiKey => "api-key",
            Self::OAuth => "oauth",
            Self::DeviceCode => "device-code",
            Self::AwsCredentialChain => "aws-credential-chain",
            Self::GoogleApplicationDefault => "google-adc",
            Self::ServiceKey => "service-key",
            Self::Local => "local",
            Self::Hybrid => "hybrid",
        }
    }
}

/// A builtin provider definition that can be resolved into a runtime config.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderSpec {
    /// Stable provider identifier.
    pub id: &'static str,
    /// Human-friendly provider name.
    pub display_name: &'static str,
    /// The normalized family adapter used by the runtime.
    pub family: ModelProviderFamily,
    /// The primary auth mode for the provider.
    pub auth_kind: ProviderAuthKind,
    /// The default base URL when it is stable enough to bake into the registry.
    pub default_base_url: Option<&'static str>,
    /// The default model id used when no model override is present.
    pub default_model: &'static str,
    /// Environment variables that may provide the provider credential.
    pub api_key_envs: &'static [&'static str],
    /// Additional environment variables that influence routing or auth.
    pub required_envs: &'static [&'static str],
    /// Provider-owned default headers.
    pub default_headers: &'static [(&'static str, &'static str)],
    /// The provider capability matrix used by the runtime.
    pub capabilities: ModelCapabilities,
    /// Lightweight implementation notes captured from OpenClaw/OpenCode behavior.
    pub notes: &'static [&'static str],
}

impl ProviderSpec {
    /// Materializes the provider's static headers into an owned map.
    pub fn default_headers(&self) -> BTreeMap<String, String> {
        self.default_headers
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect()
    }
}
