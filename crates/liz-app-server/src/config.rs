//! Filesystem-backed user configuration for provider selection and overrides.

use crate::model::{ModelGatewayConfig, ProviderOverride};
use crate::storage::StoragePaths;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

const CONFIG_FILE_NAME: &str = "config.json";

/// Persistent provider configuration rooted inside `.liz`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LizConfigFile {
    /// Selected primary provider identifier.
    pub primary_provider: Option<String>,
    /// Provider-specific overrides keyed by provider id.
    pub providers: BTreeMap<String, LizProviderConfig>,
}

/// One provider entry in the persisted config file.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LizProviderConfig {
    /// Optional base URL override.
    pub base_url: Option<String>,
    /// Optional API key override.
    pub api_key: Option<String>,
    /// Optional model id override.
    pub model_id: Option<String>,
    /// Optional extra headers.
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    /// Optional provider-specific metadata.
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
}

impl LizConfigFile {
    /// Loads config from the resolved `.liz/config.json` path when it exists.
    pub fn load(paths: &StoragePaths) -> Self {
        read_config_file(&config_file_path(paths)).unwrap_or_default()
    }

    /// Saves config to the resolved `.liz/config.json` path, creating parent directories.
    pub fn save(&self, paths: &StoragePaths) -> std::io::Result<()> {
        let path = config_file_path(paths);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let encoded = serde_json::to_string_pretty(self)
            .expect("config serialization should succeed for serializable config");
        fs::write(path, encoded)
    }

    /// Merges the file-backed settings with environment-backed defaults.
    pub fn into_gateway_config(self, env_config: ModelGatewayConfig) -> ModelGatewayConfig {
        let primary_provider =
            self.primary_provider.unwrap_or_else(|| env_config.primary_provider.clone());
        let mut overrides = env_config.overrides;

        for (provider_id, provider) in self.providers {
            let mut override_config =
                overrides.remove(&provider_id).unwrap_or_else(ProviderOverride::default);
            if provider.base_url.is_some() {
                override_config.base_url = provider.base_url;
            }
            if provider.api_key.is_some() {
                override_config.api_key = provider.api_key;
            }
            if provider.model_id.is_some() {
                override_config.model_id = provider.model_id;
            }
            override_config.headers.extend(provider.headers);
            override_config.metadata.extend(provider.metadata);
            overrides.insert(provider_id, override_config);
        }

        ModelGatewayConfig { primary_provider, overrides }
    }
}

/// Resolves the config file path inside the `.liz` root.
pub fn config_file_path(paths: &StoragePaths) -> PathBuf {
    paths.root().join(CONFIG_FILE_NAME)
}

fn read_config_file(path: &Path) -> Option<LizConfigFile> {
    let contents = fs::read_to_string(path).ok()?;
    serde_json::from_str(&contents).ok()
}
