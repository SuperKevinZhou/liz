//! Strongly typed identifiers shared across the liz protocol surface.

use serde::{Deserialize, Serialize};
use std::fmt;

macro_rules! string_identifier {
    ($name:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            /// Creates a new typed identifier from an owned or borrowed string.
            pub fn new(value: impl Into<String>) -> Self {
                Self(value.into())
            }

            /// Returns the raw identifier string.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl From<&str> for $name {
            fn from(value: &str) -> Self {
                Self::new(value)
            }
        }

        impl From<String> for $name {
            fn from(value: String) -> Self {
                Self::new(value)
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                self.as_str()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }
    };
}

string_identifier!(RequestId, "A unique identifier for a client request.");
string_identifier!(ThreadId, "A unique identifier for a persisted work thread.");
string_identifier!(TurnId, "A unique identifier for a single turn within a thread.");
string_identifier!(EventId, "A unique identifier for an emitted server event.");
string_identifier!(ApprovalId, "A unique identifier for an approval flow.");
string_identifier!(CheckpointId, "A unique identifier for a recovery checkpoint.");
string_identifier!(ArtifactId, "A unique identifier for a persisted artifact.");
string_identifier!(MemoryFactId, "A unique identifier for a compiled memory fact.");
string_identifier!(ExecutorTaskId, "A unique identifier for a background executor task.");
