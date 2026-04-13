//! Model-gateway orchestration over provider adapters.

use crate::model::adapters::{
    LocalGatewayAdapter, OpenAiAdapter, OpenAiCompatibleAdapter, ProviderAdapter,
};
use crate::model::capabilities::ModelCapabilities;
use crate::model::normalized_stream::{NormalizedTurnEvent, UsageDelta};
use liz_protocol::{Thread, Turn};
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

/// The model gateway mediates between the runtime and concrete provider adapters.
#[derive(Debug, Clone)]
pub struct ModelGateway {
    primary_provider: &'static str,
    openai: OpenAiAdapter,
    openai_compatible: OpenAiCompatibleAdapter,
    local_gateway: LocalGatewayAdapter,
}

impl Default for ModelGateway {
    fn default() -> Self {
        Self {
            primary_provider: "openai",
            openai: OpenAiAdapter::default(),
            openai_compatible: OpenAiCompatibleAdapter::default(),
            local_gateway: LocalGatewayAdapter::default(),
        }
    }
}

impl ModelGateway {
    /// Returns the capability matrix for the currently selected primary provider.
    pub fn primary_capabilities(&self) -> &ModelCapabilities {
        self.adapter(self.primary_provider).capabilities()
    }

    /// Streams one turn through the selected provider adapter.
    pub fn run_turn<F>(
        &self,
        request: ModelTurnRequest,
        mut sink: F,
    ) -> Result<ModelRunSummary, ModelError>
    where
        F: FnMut(NormalizedTurnEvent),
    {
        self.adapter(self.primary_provider)
            .stream_turn(request, &mut sink)
    }

    /// Returns the provider interfaces reserved for future expansion.
    pub fn reserved_interfaces(&self) -> Vec<&'static str> {
        vec![
            self.openai_compatible.provider_name(),
            self.local_gateway.provider_name(),
        ]
    }

    fn adapter(&self, provider: &str) -> &dyn ProviderAdapter {
        match provider {
            "openai" => &self.openai,
            "openai-compatible" => &self.openai_compatible,
            "local-gateway" => &self.local_gateway,
            _ => &self.openai,
        }
    }
}
