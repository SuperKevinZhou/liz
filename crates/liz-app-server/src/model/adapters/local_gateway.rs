//! Reserved local-gateway adapter surface.

use crate::model::adapters::ProviderAdapter;
use crate::model::capabilities::ModelCapabilities;
use crate::model::gateway::{ModelError, ModelRunSummary, ModelTurnRequest};
use crate::model::normalized_stream::NormalizedTurnEvent;

/// Reserved adapter surface for a future local model gateway.
#[derive(Debug, Clone)]
pub struct LocalGatewayAdapter {
    capabilities: ModelCapabilities,
}

impl Default for LocalGatewayAdapter {
    fn default() -> Self {
        Self { capabilities: ModelCapabilities::local_gateway() }
    }
}

impl ProviderAdapter for LocalGatewayAdapter {
    fn provider_name(&self) -> &'static str {
        "local-gateway"
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
            "local-gateway adapter is reserved for a later phase".to_owned(),
        ))
    }
}
