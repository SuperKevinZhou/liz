//! Storage-layer error types.

use std::error::Error;
use std::fmt;

/// The result type shared by all storage interfaces.
pub type StorageResult<T> = Result<T, StorageError>;

/// Errors emitted by the filesystem-backed storage layer.
#[derive(Debug)]
pub enum StorageError {
    /// A filesystem operation failed.
    Io(std::io::Error),
    /// JSON serialization or deserialization failed.
    Serde(serde_json::Error),
}

impl fmt::Display for StorageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "storage I/O error: {error}"),
            Self::Serde(error) => write!(f, "storage serialization error: {error}"),
        }
    }
}

impl Error for StorageError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Serde(error) => Some(error),
        }
    }
}

impl From<std::io::Error> for StorageError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for StorageError {
    fn from(error: serde_json::Error) -> Self {
        Self::Serde(error)
    }
}
