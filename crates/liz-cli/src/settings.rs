//! Local `.liz` settings helpers for provider configuration.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const CONFIG_DIR_NAME: &str = ".liz";
const CONFIG_FILE_NAME: &str = "config.json";

/// Persistent provider configuration rooted inside `.liz`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LizConfigFile {
    /// Selected primary provider identifier.
    pub primary_provider: Option<String>,
    /// Provider-specific overrides keyed by provider id.
    #[serde(default)]
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

/// Resolved local settings location.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsLocation {
    /// The resolved `.liz` directory.
    pub config_dir: PathBuf,
    /// The config file path inside the directory.
    pub config_file: PathBuf,
}

impl SettingsLocation {
    /// Resolves the `.liz` directory by searching upward from cwd, falling back to cwd.
    pub fn discover() -> Self {
        let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let config_dir = find_existing_liz_dir(&cwd).unwrap_or_else(|| cwd.join(CONFIG_DIR_NAME));
        let config_file = config_dir.join(CONFIG_FILE_NAME);
        Self { config_dir, config_file }
    }
}

impl LizConfigFile {
    /// Loads the config file from the resolved settings location.
    pub fn load(location: &SettingsLocation) -> std::io::Result<Self> {
        if !location.config_file.exists() {
            return Ok(Self::default());
        }
        let contents = fs::read_to_string(&location.config_file)?;
        Ok(serde_json::from_str(&contents).unwrap_or_default())
    }

    /// Saves the config file to the resolved settings location.
    pub fn save(&self, location: &SettingsLocation) -> std::io::Result<()> {
        fs::create_dir_all(&location.config_dir)?;
        let encoded = serde_json::to_string_pretty(self)
            .expect("config serialization should succeed for serializable config");
        fs::write(&location.config_file, encoded)
    }

    /// Sets the selected primary provider.
    pub fn set_primary_provider(&mut self, provider_id: String) {
        self.primary_provider = Some(provider_id);
    }

    /// Upserts one provider entry with the provided fields.
    pub fn upsert_provider(
        &mut self,
        provider_id: String,
        field: ProviderField,
        value: String,
    ) -> String {
        let provider = self.providers.entry(provider_id.clone()).or_default();
        match field {
            ProviderField::BaseUrl => provider.base_url = Some(value),
            ProviderField::ApiKey => provider.api_key = Some(value),
            ProviderField::Model => provider.model_id = Some(value),
        }
        provider_id
    }
}

/// Supported direct provider fields for `/settings set-provider`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderField {
    /// Provider base URL.
    BaseUrl,
    /// Provider API key.
    ApiKey,
    /// Provider default model.
    Model,
}

impl ProviderField {
    /// Parses a user-supplied field token.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "base-url" | "base_url" => Some(Self::BaseUrl),
            "api-key" | "api_key" => Some(Self::ApiKey),
            "model" | "model-id" | "model_id" => Some(Self::Model),
            _ => None,
        }
    }

    /// Returns the canonical display token.
    pub fn display_name(self) -> &'static str {
        match self {
            Self::BaseUrl => "base-url",
            Self::ApiKey => "api-key",
            Self::Model => "model",
        }
    }
}

fn find_existing_liz_dir(start: &Path) -> Option<PathBuf> {
    for ancestor in start.ancestors() {
        let candidate = ancestor.join(CONFIG_DIR_NAME);
        if candidate.is_dir() {
            return Some(candidate);
        }
    }
    None
}
