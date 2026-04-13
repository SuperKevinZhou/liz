//! Shared primitive protocol types.

use serde::{Deserialize, Serialize};

/// A protocol version string used during transport handshakes.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProtocolVersion(String);

impl ProtocolVersion {
    /// Creates a new protocol version wrapper.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Returns the raw version string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A serialized timestamp shared by requests, responses, and events.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Timestamp(String);

impl Timestamp {
    /// Creates a new timestamp wrapper from a preformatted string.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Returns the raw timestamp string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Describes the risk level associated with a user-visible action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    /// The action is bounded and expected to be reversible.
    Low,
    /// The action may have noticeable side effects and should be surfaced clearly.
    Medium,
    /// The action may touch important state and usually needs approval.
    High,
    /// The action is highly sensitive and should be treated with maximum caution.
    Critical,
}
