//! Model-layer integration for provider-aware streaming turns.

pub mod adapters;
mod auth;
mod capabilities;
mod config;
mod family;
mod gateway;
mod http;
mod invocation;
mod normalized_stream;
mod provider_spec;
mod registry;

pub use capabilities::ModelCapabilities;
pub use config::{ModelGatewayConfig, ProviderOverride, ResolvedProvider};
pub use family::ModelProviderFamily;
pub use gateway::{ModelError, ModelGateway, ModelRunSummary, ModelTurnRequest};
pub use invocation::{InvocationTransport, ProviderInvocationPlan};
pub use normalized_stream::{NormalizedTurnEvent, UsageDelta};
pub use provider_spec::{ProviderAuthKind, ProviderSpec};
pub use registry::ProviderRegistry;
