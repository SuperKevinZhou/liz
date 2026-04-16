//! Authentication helpers for provider families that need runtime credentials.

use crate::model::gateway::ModelError;
use serde::Deserialize;
use std::env;
use std::path::PathBuf;
use yup_oauth2::authenticator::ApplicationDefaultCredentialsTypes;
use yup_oauth2::authenticator::DefaultAuthenticator;
use yup_oauth2::authorized_user::AuthorizedUserSecret;
use yup_oauth2::{
    ApplicationDefaultCredentialsAuthenticator, ApplicationDefaultCredentialsFlowOpts,
    AuthorizedUserAuthenticator,
};

const GOOGLE_CLOUD_PLATFORM_SCOPE: &str = "https://www.googleapis.com/auth/cloud-platform";

#[derive(Debug, Deserialize)]
struct GoogleCredentialType {
    #[serde(rename = "type")]
    credential_type: String,
}

/// Resolves a Google ADC bearer token for Vertex AI requests.
pub fn google_vertex_bearer_token() -> Result<String, ModelError> {
    let runtime = tokio::runtime::Runtime::new().map_err(|error| {
        ModelError::ProviderFailure(format!(
            "failed to initialize Google auth runtime: {error}"
        ))
    })?;
    runtime.block_on(async { google_vertex_bearer_token_async().await })
}

async fn google_vertex_bearer_token_async() -> Result<String, ModelError> {
    let scopes = [GOOGLE_CLOUD_PLATFORM_SCOPE];

    if let Some(secret) = load_authorized_user_secret()? {
        let authenticator =
            AuthorizedUserAuthenticator::builder(secret)
                .build()
                .await
                .map_err(|error| {
                    ModelError::ProviderFailure(format!(
                        "failed to initialize Google authorized-user auth: {error}"
                    ))
                })?;
        return extract_google_token(&authenticator, &scopes).await;
    }

    let opts = ApplicationDefaultCredentialsFlowOpts::default();
    let authenticator = match ApplicationDefaultCredentialsAuthenticator::builder(opts).await {
        ApplicationDefaultCredentialsTypes::InstanceMetadata(builder) => builder.build().await,
        ApplicationDefaultCredentialsTypes::ServiceAccount(builder) => builder.build().await,
    }
    .map_err(|error| {
        ModelError::ProviderFailure(format!(
            "failed to initialize Google application-default credentials: {error}"
        ))
    })?;

    extract_google_token(&authenticator, &scopes).await
}

async fn extract_google_token(
    authenticator: &DefaultAuthenticator,
    scopes: &[&str],
) -> Result<String, ModelError> {
    let token = authenticator.token(scopes).await.map_err(|error| {
        ModelError::ProviderFailure(format!("failed to fetch Google access token: {error}"))
    })?;

    token
        .token()
        .map(str::to_owned)
        .ok_or_else(|| ModelError::ProviderFailure("google access token response was empty".to_owned()))
}

fn load_authorized_user_secret() -> Result<Option<AuthorizedUserSecret>, ModelError> {
    let Some(path) = google_adc_path() else {
        return Ok(None);
    };
    let Ok(contents) = std::fs::read_to_string(&path) else {
        return Ok(None);
    };

    let credential_type: GoogleCredentialType = serde_json::from_str(&contents).map_err(|error| {
        ModelError::ProviderFailure(format!(
            "failed to parse Google ADC credentials type from {}: {error}",
            path.display()
        ))
    })?;
    if credential_type.credential_type != "authorized_user" {
        return Ok(None);
    }

    serde_json::from_str(&contents).map(Some).map_err(|error| {
        ModelError::ProviderFailure(format!(
            "failed to parse Google authorized-user ADC credentials from {}: {error}",
            path.display()
        ))
    })
}

fn google_adc_path() -> Option<PathBuf> {
    env::var("GOOGLE_APPLICATION_CREDENTIALS")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
        .or_else(default_google_adc_path)
}

fn default_google_adc_path() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        env::var("APPDATA").ok().map(|appdata| {
            PathBuf::from(appdata).join("gcloud").join("application_default_credentials.json")
        })
    }

    #[cfg(not(target_os = "windows"))]
    {
        env::var("HOME").ok().map(|home| {
            PathBuf::from(home)
                .join(".config")
                .join("gcloud")
                .join("application_default_credentials.json")
        })
    }
}
