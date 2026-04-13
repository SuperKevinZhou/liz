//! Provider invocation plans derived from provider specs and family adapters.

use crate::model::family::ModelProviderFamily;
use crate::model::provider_spec::ProviderAuthKind;
use std::collections::BTreeMap;

/// The high-level transport a provider family expects for one turn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InvocationTransport {
    /// A plain JSON-over-HTTP request to a stable endpoint.
    HttpJson {
        /// The HTTP method.
        method: &'static str,
        /// The resolved provider base URL.
        base_url: String,
        /// The request path relative to the base URL.
        path: String,
    },
    /// A provider-owned operation whose concrete endpoint is family-specific or deferred.
    ProviderOperation {
        /// A stable label for the operation.
        operation: &'static str,
        /// An optional base URL when the provider still routes over HTTP.
        base_url: Option<String>,
    },
}

/// A provider invocation plan built before the runtime attempts a real network call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderInvocationPlan {
    /// Stable provider identifier.
    pub provider_id: String,
    /// Human-friendly provider display name.
    pub display_name: String,
    /// Provider family used to normalize runtime behavior.
    pub family: ModelProviderFamily,
    /// Default model identifier resolved for the turn.
    pub model_id: String,
    /// Primary auth mode used by the provider.
    pub auth_kind: ProviderAuthKind,
    /// The transport the runtime should drive.
    pub transport: InvocationTransport,
    /// The final headers that would be sent to the provider.
    pub headers: BTreeMap<String, String>,
    /// A compact payload preview helpful for tests and event traces.
    pub payload_preview: String,
    /// Important execution notes and caveats.
    pub notes: Vec<String>,
}
