//! Runtime-layer error types.

use crate::storage::StorageError;
use std::error::Error;
use std::fmt;

/// Shared result type used by the runtime layer.
pub type RuntimeResult<T> = Result<T, RuntimeError>;

/// Errors emitted while coordinating thread and turn state.
#[derive(Debug)]
pub enum RuntimeError {
    /// A requested resource could not be found.
    NotFound {
        /// The stable machine-readable error code.
        code: &'static str,
        /// The human-readable error message.
        message: String,
    },
    /// A requested action conflicts with current runtime state.
    InvalidState {
        /// The stable machine-readable error code.
        code: &'static str,
        /// The human-readable error message.
        message: String,
    },
    /// A request is recognized but intentionally unsupported for the current phase.
    Unsupported {
        /// The stable machine-readable error code.
        code: &'static str,
        /// The human-readable error message.
        message: String,
    },
    /// The storage layer failed while serving the request.
    Storage(StorageError),
}

impl RuntimeError {
    /// Builds a not-found runtime error.
    pub fn not_found(code: &'static str, message: impl Into<String>) -> Self {
        Self::NotFound { code, message: message.into() }
    }

    /// Builds an invalid-state runtime error.
    pub fn invalid_state(code: &'static str, message: impl Into<String>) -> Self {
        Self::InvalidState { code, message: message.into() }
    }

    /// Builds an unsupported runtime error.
    pub fn unsupported(code: &'static str, message: impl Into<String>) -> Self {
        Self::Unsupported { code, message: message.into() }
    }

    /// Returns the stable machine-readable error code.
    pub fn code(&self) -> &'static str {
        match self {
            Self::NotFound { code, .. } => code,
            Self::InvalidState { code, .. } => code,
            Self::Unsupported { code, .. } => code,
            Self::Storage(_) => "storage_error",
        }
    }

    /// Returns whether the action may succeed if retried.
    pub fn retryable(&self) -> bool {
        matches!(self, Self::Storage(_))
    }
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound { message, .. } => f.write_str(message),
            Self::InvalidState { message, .. } => f.write_str(message),
            Self::Unsupported { message, .. } => f.write_str(message),
            Self::Storage(error) => write!(f, "runtime storage failure: {error}"),
        }
    }
}

impl Error for RuntimeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Storage(error) => Some(error),
            _ => None,
        }
    }
}

impl From<StorageError> for RuntimeError {
    fn from(value: StorageError) -> Self {
        Self::Storage(value)
    }
}
