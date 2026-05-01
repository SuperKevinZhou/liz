//! Model-gateway orchestration over provider specs and family adapters.

use crate::model::adapters::{
    AnthropicAdapter, AwsBedrockAdapter, GoogleAdapter, OpenAiStyleAdapter,
};
use crate::model::capabilities::ModelCapabilities;
use crate::model::config::{ModelGatewayConfig, ProviderOverride, ResolvedProvider};
use crate::model::family::ModelProviderFamily;
use crate::model::normalized_stream::{NormalizedTurnEvent, UsageDelta};
use crate::model::registry::ProviderRegistry;
use crate::model::{
    ProviderToolCall, ProviderToolProtocol, ToolResultInjection, ToolSurfaceMode, ToolSurfaceSpec,
};
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
    /// The stable system-level instructions for `liz`.
    pub system_prompt: String,
    /// The dynamic runtime-owned operating context for the current turn.
    pub developer_prompt: String,
    /// The user-authored input for the current turn.
    pub user_prompt: String,
    /// The flattened prompt transcript used by fallback and simulation paths.
    pub prompt: String,
    /// Structured tool results injected into continuation requests.
    pub tool_result_injections: Vec<ToolResultInjection>,
    /// The tool surface exposed to the provider for this turn.
    pub tool_surface_mode: ToolSurfaceMode,
}

impl ModelTurnRequest {
    /// Builds a request from structured prompt parts and keeps a flattened fallback transcript.
    pub fn from_prompt_parts(
        thread: Thread,
        turn: Turn,
        system_prompt: String,
        developer_prompt: String,
        user_prompt: String,
    ) -> Self {
        let prompt = render_flattened_prompt(&system_prompt, &developer_prompt, &user_prompt);
        Self {
            thread,
            turn,
            system_prompt,
            developer_prompt,
            user_prompt,
            prompt,
            tool_result_injections: Vec::new(),
            tool_surface_mode: ToolSurfaceMode::Standard,
        }
    }

    /// Returns the instruction block that should stay above user input at transport time.
    pub fn instruction_prompt(&self) -> String {
        render_instruction_prompt(&self.system_prompt, &self.developer_prompt)
    }

    /// Returns a copy with tool-result injections appended for continuation.
    pub fn with_tool_result_injections(
        mut self,
        tool_result_injections: Vec<ToolResultInjection>,
    ) -> Self {
        self.tool_result_injections = tool_result_injections;
        self
    }

    /// Returns a copy with the selected tool surface mode.
    pub fn with_tool_surface_mode(mut self, mode: ToolSurfaceMode) -> Self {
        self.tool_surface_mode = mode;
        self
    }
}

/// A normalized summary of a completed provider run.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelRunSummary {
    /// The final assistant message, if one was produced.
    pub assistant_message: Option<String>,
    /// The accumulated token and cache accounting.
    pub usage: UsageDelta,
    /// Tool calls committed during this provider invocation.
    pub tool_calls: Vec<ProviderToolCall>,
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

fn render_instruction_prompt(system_prompt: &str, developer_prompt: &str) -> String {
    let system_prompt = system_prompt.trim();
    let developer_prompt = developer_prompt.trim();
    match (system_prompt.is_empty(), developer_prompt.is_empty()) {
        (true, true) => String::new(),
        (false, true) => system_prompt.to_owned(),
        (true, false) => developer_prompt.to_owned(),
        (false, false) => format!("{system_prompt}\n\n{developer_prompt}"),
    }
}

fn render_flattened_prompt(
    system_prompt: &str,
    developer_prompt: &str,
    user_prompt: &str,
) -> String {
    let instruction_prompt = render_instruction_prompt(system_prompt, developer_prompt);
    let user_prompt = user_prompt.trim();
    match (instruction_prompt.is_empty(), user_prompt.is_empty()) {
        (true, true) => String::new(),
        (false, true) => format!("system:\n{instruction_prompt}"),
        (true, false) => format!("user:\n{user_prompt}"),
        (false, false) => format!("system:\n{instruction_prompt}\n\nuser:\n{user_prompt}"),
    }
}

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
        let protocol = provider_tool_protocol(&provider);
        let tool_surface = match request.tool_surface_mode {
            ToolSurfaceMode::Standard => ToolSurfaceSpec::standard(protocol),
            ToolSurfaceMode::ConversationOnly => ToolSurfaceSpec::conversation_only(protocol),
        };
        match provider.spec.family {
            ModelProviderFamily::AnthropicMessages => self.anthropic.stream_turn(
                &provider,
                request,
                tool_surface,
                self.simulate,
                &mut sink,
            ),
            ModelProviderFamily::AwsBedrockConverse => {
                self.bedrock.stream_turn(&provider, request, tool_surface, self.simulate, &mut sink)
            }
            ModelProviderFamily::GoogleGenerativeAi
            | ModelProviderFamily::GoogleVertex
            | ModelProviderFamily::GoogleVertexAnthropic => {
                self.google.stream_turn(&provider, request, tool_surface, self.simulate, &mut sink)
            }
            ModelProviderFamily::OpenAiResponses
            | ModelProviderFamily::OpenAiCompatible
            | ModelProviderFamily::GitHubCopilot
            | ModelProviderFamily::GitLabDuo => self.openai_style.stream_turn(
                &provider,
                request,
                tool_surface,
                self.simulate,
                &mut sink,
            ),
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

    /// Returns the builtin provider spec for a provider identifier.
    pub fn provider_spec(&self, provider_id: &str) -> Option<&crate::model::ProviderSpec> {
        self.registry.provider(provider_id)
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
                "provider {} is not yet implemented for live runtime use",
                spec.id
            )));
        }

        Ok(ResolvedProvider::from_spec(spec, self.config.overrides.get(self.primary_provider_id())))
    }
}

fn provider_tool_protocol(provider: &ResolvedProvider) -> ProviderToolProtocol {
    if matches!(provider.spec.family, ModelProviderFamily::GitLabDuo)
        && provider.spec.capabilities.structured_tool_protocol
    {
        return ProviderToolProtocol::StructuredFallback;
    }
    if provider.spec.capabilities.native_tool_calls {
        ProviderToolProtocol::Native
    } else {
        ProviderToolProtocol::StructuredFallback
    }
}
