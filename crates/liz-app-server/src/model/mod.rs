//! Model-layer integration for provider-aware streaming turns.

pub mod adapters;
mod capabilities;
mod gateway;
mod normalized_stream;

pub use capabilities::ModelCapabilities;
pub use gateway::{ModelError, ModelGateway, ModelRunSummary, ModelTurnRequest};
pub use normalized_stream::{NormalizedTurnEvent, UsageDelta};
