//! Provider auth profile storage interfaces.

use crate::storage::fs::{ensure_layout, read_json, write_json};
use crate::storage::{StoragePaths, StorageResult};
use liz_protocol::ProviderAuthProfile;
use serde::{Deserialize, Serialize};

/// The persisted auth profile snapshot.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthProfileSnapshot {
    /// All persisted provider auth profiles.
    pub profiles: Vec<ProviderAuthProfile>,
}

/// Storage interface for provider auth profiles.
pub trait AuthProfileStore {
    /// Reads the full auth profile snapshot.
    fn read_snapshot(&self) -> StorageResult<AuthProfileSnapshot>;
    /// Writes the full auth profile snapshot.
    fn write_snapshot(&self, snapshot: &AuthProfileSnapshot) -> StorageResult<()>;
}

/// Filesystem-backed auth profile store.
#[derive(Debug, Clone)]
pub struct FsAuthProfileStore {
    paths: StoragePaths,
}

impl FsAuthProfileStore {
    /// Creates a new auth profile store.
    pub fn new(paths: StoragePaths) -> Self {
        Self { paths }
    }
}

impl AuthProfileStore for FsAuthProfileStore {
    fn read_snapshot(&self) -> StorageResult<AuthProfileSnapshot> {
        ensure_layout(&self.paths)?;
        Ok(read_json(&self.paths.auth_profiles_file())?.unwrap_or_default())
    }

    fn write_snapshot(&self, snapshot: &AuthProfileSnapshot) -> StorageResult<()> {
        ensure_layout(&self.paths)?;
        write_json(&self.paths.auth_profiles_file(), snapshot)
    }
}
