//! Model-gateway orchestration over provider specs and family adapters.

use crate::model::adapters::{
    AnthropicAdapter, AwsBedrockAdapter, GoogleAdapter, OpenAiStyleAdapter,
};
use crate::model::capabilities::ModelCapabilities;
use crate::model::config::{ModelGatewayConfig, ProviderOverride, ResolvedProvider};
use crate::model::family::ModelProviderFamily;
use crate::model::normalized_stream::{NormalizedTurnEvent, UsageDelta};
use crate::model::registry::ProviderRegistry;
use liz_protocol::{Thread, Turn};
use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;

/// A fully assembled request handed to the model layer.
#[derive(Debug, Clone)]
pub struct ModelTurnRequest {
    /// The thread being advanced.
    pub thread: Thread,
    /// The turn being executed.
    pub turn: Turn,
    /// The final assembled prompt or context envelope rendered into text.
    pub prompt: String,
}

/// A normalized summary of a completed provider run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelRunSummary {
    /// The final assistant message, if one was produced.
    pub assistant_message: Option<String>,
    /// The accumulated token and cache accounting.
    pub usage: UsageDelta,
}

/// Errors emitted while driving a provider adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelError {
    /// The selected provider is not supported by the current configuration.
    UnsupportedProvider(String),
    /// The provider reported a runtime failure.
    ProviderFailure(String),
}

impl fmt::Display for ModelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedProvider(provider) => {
                write!(f, "provider {provider} is not configured")
            }
            Self::ProviderFailure(message) => f.write_str(message),
        }
    }
}

impl Error for ModelError {}

/// The model gateway mediates between the runtime and concrete provider families.
#[derive(Debug, Clone)]
pub struct ModelGateway {
    config: ModelGatewayConfig,
    simulate: bool,
    registry: ProviderRegistry,
    anthropic: AnthropicAdapter,
    bedrock: AwsBedrockAdapter,
    google: GoogleAdapter,
    openai_style: OpenAiStyleAdapter,
}

impl Default for ModelGateway {
    fn default() -> Self {
        Self::from_config(ModelGatewayConfig::from_env())
    }
}

impl ModelGateway {
    /// Creates a gateway from an explicit config.
    pub fn from_config(config: ModelGatewayConfig) -> Self {
        Self {
            config,
            simulate: false,
            registry: ProviderRegistry::default(),
            anthropic: AnthropicAdapter::default(),
            bedrock: AwsBedrockAdapter::default(),
            google: GoogleAdapter::default(),
            openai_style: OpenAiStyleAdapter::default(),
        }
    }

    /// Creates a gateway that simulates provider streaming for isolated tests.
    pub fn simulated() -> Self {
        Self::default().with_simulation(true)
    }

    /// Returns the configured primary provider identifier.
    pub fn primary_provider_id(&self) -> &str {
        &self.config.primary_provider
    }

    /// Overrides the primary provider while keeping the existing registry and overrides.
    pub fn with_primary_provider(mut self, provider_id: impl Into<String>) -> Self {
        self.config.primary_provider = provider_id.into();
        self
    }

    /// Adds or merges a provider override.
    pub fn with_provider_override(
        mut self,
        provider_id: impl Into<String>,
        override_config: ProviderOverride,
    ) -> Self {
        let provider_id = provider_id.into();
        match self.config.overrides.get_mut(&provider_id) {
            Some(existing) => {
                if override_config.base_url.is_some() {
                    existing.base_url = override_config.base_url;
                }
                if override_config.api_key.is_some() {
                    existing.api_key = override_config.api_key;
                }
                if override_config.model_id.is_some() {
                    existing.model_id = override_config.model_id;
                }
                existing.headers.extend(override_config.headers);
                existing.metadata.extend(override_config.metadata);
            }
            None => {
                self.config.overrides.insert(provider_id, override_config);
            }
        }
        self
    }

    /// Forces the gateway to simulate provider streaming instead of making live calls.
    pub fn with_simulation(mut self, simulate: bool) -> Self {
        self.simulate = simulate;
        self
    }

    /// Returns the capability matrix for the currently selected primary provider.
    pub fn primary_capabilities(&self) -> &ModelCapabilities {
        self.registry
            .provider(self.primary_provider_id())
            .map(|provider| &provider.capabilities)
            .unwrap_or_else(|| {
                self.registry
                    .provider("openai")
                    .map(|provider| &provider.capabilities)
                    .expect("openai spec")
            })
    }

    /// Returns the sorted list of supported provider identifiers.
    pub fn supported_provider_ids(&self) -> Vec<&'static str> {
        self.registry.supported_provider_ids()
    }

    /// Streams one turn through the selected provider family.
    pub fn run_turn<F>(
        &self,
        request: ModelTurnRequest,
        mut sink: F,
    ) -> Result<ModelRunSummary, ModelError>
    where
        F: FnMut(NormalizedTurnEvent),
    {
        let provider = self.resolve_primary_provider()?;
        match provider.spec.family {
            ModelProviderFamily::AnthropicMessages => {
                self.anthropic.stream_turn(&provider, request, self.simulate, &mut sink)
            }
            ModelProviderFamily::AwsBedrockConverse => {
                self.bedrock.stream_turn(&provider, request, self.simulate, &mut sink)
            }
            ModelProviderFamily::GoogleGenerativeAi
            | ModelProviderFamily::GoogleVertex
            | ModelProviderFamily::GoogleVertexAnthropic => {
                self.google.stream_turn(&provider, request, self.simulate, &mut sink)
            }
            ModelProviderFamily::OpenAiResponses
            | ModelProviderFamily::OpenAiCompatible
            | ModelProviderFamily::GitHubCopilot
            | ModelProviderFamily::GitLabDuo => {
                self.openai_style.stream_turn(&provider, request, self.simulate, &mut sink)
            }
        }
    }

    /// Returns a short summary of the configured provider registry for diagnostics.
    pub fn provider_summary(&self) -> BTreeMap<&'static str, &'static str> {
        self.registry
            .providers()
            .iter()
            .map(|(id, spec)| (*id, spec.family.transport_label()))
            .collect()
    }

    /// Resolves the primary provider after applying overrides and environment defaults.
    pub fn resolved_primary_provider(&self) -> Result<ResolvedProvider, ModelError> {
        self.resolve_primary_provider()
    }

    fn resolve_primary_provider(&self) -> Result<ResolvedProvider, ModelError> {
        let Some(spec) = self.registry.provider(self.primary_provider_id()) else {
            return Err(ModelError::UnsupportedProvider(self.primary_provider_id().to_owned()));
        };
        if !self.simulate && !spec.is_runtime_ready() {
            return Err(ModelError::UnsupportedProvider(format!(
                "provider {} is not yet implemented for live runtime use; see PROVIDER_SUPPORT.md",
                spec.id
            )));
        }

        Ok(ResolvedProvider::from_spec(spec, self.config.overrides.get(self.primary_provider_id())))
    }
}
