//! Provider-resolution config and environment seams.

use crate::model::provider_spec::{ProviderAuthKind, ProviderSpec};
use std::collections::BTreeMap;
use std::env;

/// Overrides applied on top of the builtin provider spec.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProviderOverride {
    /// Overrides the provider base URL.
    pub base_url: Option<String>,
    /// Overrides the provider API key or bearer token.
    pub api_key: Option<String>,
    /// Overrides the default model for the provider.
    pub model_id: Option<String>,
    /// Additional request headers to send for the provider.
    pub headers: BTreeMap<String, String>,
    /// Additional provider metadata.
    pub metadata: BTreeMap<String, String>,
}

/// Gateway-level config used to resolve the primary provider and provider overrides.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelGatewayConfig {
    /// The active provider used for `run_turn`.
    pub primary_provider: String,
    /// Provider-specific override configs keyed by provider id.
    pub overrides: BTreeMap<String, ProviderOverride>,
}

impl ModelGatewayConfig {
    /// Reads a minimal config surface from environment variables.
    pub fn from_env() -> Self {
        let mut overrides = BTreeMap::new();
        let primary_provider = env::var("LIZ_PROVIDER").unwrap_or_else(|_| "openai".to_owned());

        let mut primary_override = ProviderOverride::default();
        primary_override.base_url = env::var("LIZ_PROVIDER_BASE_URL").ok();
        primary_override.api_key = env::var("LIZ_PROVIDER_API_KEY").ok();
        primary_override.model_id = env::var("LIZ_PROVIDER_MODEL").ok();

        if let Ok(value) = env::var("LIZ_PROVIDER_REFERER") {
            primary_override
                .headers
                .insert("HTTP-Referer".to_owned(), value);
        }
        if let Ok(value) = env::var("LIZ_PROVIDER_TITLE") {
            primary_override.headers.insert("X-Title".to_owned(), value);
        }

        if primary_override.base_url.is_some()
            || primary_override.api_key.is_some()
            || primary_override.model_id.is_some()
            || !primary_override.headers.is_empty()
        {
            overrides.insert(primary_provider.clone(), primary_override);
        }

        Self {
            primary_provider,
            overrides,
        }
    }
}

impl Default for ModelGatewayConfig {
    fn default() -> Self {
        Self::from_env()
    }
}

/// A builtin provider spec after environment and override resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedProvider {
    /// The underlying builtin provider spec.
    pub spec: ProviderSpec,
    /// The resolved provider base URL when the family uses one directly.
    pub base_url: Option<String>,
    /// The resolved runtime credential when it is available.
    pub api_key: Option<String>,
    /// The resolved default model identifier.
    pub model_id: String,
    /// Fully resolved request headers.
    pub headers: BTreeMap<String, String>,
    /// Provider metadata derived from env or overrides.
    pub metadata: BTreeMap<String, String>,
}

impl ResolvedProvider {
    /// Resolves a builtin provider spec into a concrete provider config.
    pub fn from_spec(spec: &ProviderSpec, override_config: Option<&ProviderOverride>) -> Self {
        let mut base_url = override_config
            .and_then(|config| config.base_url.clone())
            .or_else(|| spec.default_base_url.map(str::to_owned));
        let api_key = override_config
            .and_then(|config| config.api_key.clone())
            .or_else(|| first_env(spec.api_key_envs));
        let model_id = override_config
            .and_then(|config| config.model_id.clone())
            .unwrap_or_else(|| spec.default_model.to_owned());
        let mut headers = spec.default_headers();
        let mut metadata = BTreeMap::new();

        if let Some(config) = override_config {
            headers.extend(config.headers.clone());
            metadata.extend(config.metadata.clone());
        }

        match spec.id {
            "azure" => {
                if base_url.is_none() {
                    if let Ok(resource_name) = env::var("AZURE_RESOURCE_NAME") {
                        metadata.insert("azure.resource_name".to_owned(), resource_name.clone());
                        base_url = Some(format!("https://{resource_name}.openai.azure.com/openai"));
                    }
                }
            }
            "azure-cognitive-services" => {
                if base_url.is_none() {
                    if let Ok(resource_name) = env::var("AZURE_COGNITIVE_SERVICES_RESOURCE_NAME") {
                        metadata.insert(
                            "azure.cognitive.resource_name".to_owned(),
                            resource_name.clone(),
                        );
                        base_url = Some(format!(
                            "https://{resource_name}.cognitiveservices.azure.com/openai"
                        ));
                    }
                }
            }
            "cloudflare-ai-gateway" => {
                if base_url.is_none() {
                    if let (Ok(account_id), Ok(gateway_id)) = (
                        env::var("CLOUDFLARE_ACCOUNT_ID"),
                        env::var("CLOUDFLARE_GATEWAY_ID"),
                    ) {
                        metadata.insert("cloudflare.account_id".to_owned(), account_id.clone());
                        metadata.insert("cloudflare.gateway_id".to_owned(), gateway_id.clone());
                        base_url = Some(format!(
                            "https://gateway.ai.cloudflare.com/v1/{account_id}/{gateway_id}"
                        ));
                    }
                }
            }
            "cloudflare-workers-ai" => {
                if base_url.is_none() {
                    if let Ok(account_id) = env::var("CLOUDFLARE_ACCOUNT_ID") {
                        metadata.insert("cloudflare.account_id".to_owned(), account_id.clone());
                        base_url = Some(format!(
                            "https://api.cloudflare.com/client/v4/accounts/{account_id}/ai/v1"
                        ));
                    }
                }
            }
            "google-vertex" | "google-vertex-anthropic" | "anthropic-vertex" => {
                if let Some(project) = first_env(&[
                    "GOOGLE_CLOUD_PROJECT",
                    "GCP_PROJECT",
                    "GCLOUD_PROJECT",
                ]) {
                    metadata.insert("google.project".to_owned(), project);
                }
                if let Some(location) = first_env(&[
                    "GOOGLE_VERTEX_LOCATION",
                    "GOOGLE_CLOUD_LOCATION",
                    "VERTEX_LOCATION",
                ]) {
                    metadata.insert("google.location".to_owned(), location);
                }
            }
            "amazon-bedrock" | "amazon-bedrock-mantle" => {
                if let Some(region) = first_env(&["AWS_REGION"]) {
                    metadata.insert("aws.region".to_owned(), region);
                }
                if let Some(profile) = first_env(&["AWS_PROFILE"]) {
                    metadata.insert("aws.profile".to_owned(), profile);
                }
                if matches!(spec.auth_kind, ProviderAuthKind::AwsCredentialChain) {
                    if api_key.is_none() {
                        metadata.insert(
                            "aws.auth".to_owned(),
                            "credential-chain-or-bearer-token".to_owned(),
                        );
                    }
                }
            }
            _ => {}
        }

        Self {
            spec: spec.clone(),
            base_url,
            api_key,
            model_id,
            headers,
            metadata,
        }
    }
}

fn first_env(keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| env::var(key).ok().filter(|value| !value.trim().is_empty()))
}
