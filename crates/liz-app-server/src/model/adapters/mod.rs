//! Provider adapter contracts and concrete adapter implementations.

mod local_gateway;
mod openai;
mod openai_compatible;

pub use local_gateway::LocalGatewayAdapter;
pub use openai::OpenAiAdapter;
pub use openai_compatible::OpenAiCompatibleAdapter;

use crate::model::capabilities::ModelCapabilities;
use crate::model::gateway::{ModelError, ModelRunSummary, ModelTurnRequest};
use crate::model::normalized_stream::NormalizedTurnEvent;

/// A provider adapter that can stream one model turn into normalized runtime events.
pub trait ProviderAdapter: Send + Sync {
    /// Returns the stable provider name.
    fn provider_name(&self) -> &'static str;

    /// Returns the adapter's capability matrix.
    fn capabilities(&self) -> &ModelCapabilities;

    /// Streams a turn into normalized runtime events.
    fn stream_turn(
        &self,
        request: ModelTurnRequest,
        sink: &mut dyn FnMut(NormalizedTurnEvent),
    ) -> Result<ModelRunSummary, ModelError>;
}
