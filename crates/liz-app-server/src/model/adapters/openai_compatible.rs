//! Reserved OpenAI-compatible adapter surface.

use crate::model::adapters::ProviderAdapter;
use crate::model::capabilities::ModelCapabilities;
use crate::model::gateway::{ModelError, ModelRunSummary, ModelTurnRequest};
use crate::model::normalized_stream::NormalizedTurnEvent;

/// Reserved adapter surface for generic OpenAI-compatible gateways.
#[derive(Debug, Clone)]
pub struct OpenAiCompatibleAdapter {
    capabilities: ModelCapabilities,
}

impl Default for OpenAiCompatibleAdapter {
    fn default() -> Self {
        Self { capabilities: ModelCapabilities::openai_compatible() }
    }
}

impl ProviderAdapter for OpenAiCompatibleAdapter {
    fn provider_name(&self) -> &'static str {
        "openai-compatible"
    }

    fn capabilities(&self) -> &ModelCapabilities {
        &self.capabilities
    }

    fn stream_turn(
        &self,
        _request: ModelTurnRequest,
        _sink: &mut dyn FnMut(NormalizedTurnEvent),
    ) -> Result<ModelRunSummary, ModelError> {
        Err(ModelError::UnsupportedProvider(
            "openai-compatible adapter is reserved for a later phase".to_owned(),
        ))
    }
}
