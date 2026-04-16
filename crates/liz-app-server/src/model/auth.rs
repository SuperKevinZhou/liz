//! Authentication helpers for provider families that need runtime credentials.

use crate::model::gateway::ModelError;
use aws_credential_types::provider::ProvideCredentials;
use aws_sigv4::http_request::{sign, SignableBody, SignableRequest, SigningSettings};
use aws_types::region::Region;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::env;
use std::path::PathBuf;
use std::time::SystemTime;
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

/// Runtime GitHub Copilot credentials resolved from a GitHub user token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopilotRuntimeAuth {
    /// Short-lived Copilot API token.
    pub token: String,
    /// Final Copilot runtime base URL.
    pub base_url: String,
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

/// Signs an AWS Bedrock Runtime request with SigV4 using the AWS default credential chain.
pub fn sign_bedrock_request(
    method: &str,
    url: &str,
    headers: &BTreeMap<String, String>,
    body: &[u8],
    region: &str,
) -> Result<BTreeMap<String, String>, ModelError> {
    let runtime = tokio::runtime::Runtime::new().map_err(|error| {
        ModelError::ProviderFailure(format!(
            "failed to initialize AWS auth runtime: {error}"
        ))
    })?;
    runtime.block_on(async { sign_bedrock_request_async(method, url, headers, body, region).await })
}

async fn sign_bedrock_request_async(
    method: &str,
    url: &str,
    headers: &BTreeMap<String, String>,
    body: &[u8],
    region: &str,
) -> Result<BTreeMap<String, String>, ModelError> {
    if let (Some(access_key), Some(secret_key)) = (
        first_env(&["AWS_ACCESS_KEY_ID"]),
        first_env(&["AWS_SECRET_ACCESS_KEY"]),
    ) {
        return sign_bedrock_request_with_credentials(
            method,
            url,
            headers,
            body,
            region,
            &access_key,
            &secret_key,
            first_env(&["AWS_SESSION_TOKEN"]).as_deref(),
        );
    }

    let config = aws_config::from_env()
        .region(Region::new(region.to_owned()))
        .load()
        .await;
    let provider = config.credentials_provider().ok_or_else(|| {
        ModelError::ProviderFailure(
            "aws credential chain is not available for Amazon Bedrock".to_owned(),
        )
    })?;
    let credentials = provider.provide_credentials().await.map_err(|error| {
        ModelError::ProviderFailure(format!(
            "failed to resolve AWS credentials for Amazon Bedrock: {error}"
        ))
    })?;

    sign_bedrock_request_with_credentials(
        method,
        url,
        headers,
        body,
        region,
        credentials.access_key_id(),
        credentials.secret_access_key(),
        credentials.session_token(),
    )
}

fn sign_bedrock_request_with_credentials(
    method: &str,
    url: &str,
    headers: &BTreeMap<String, String>,
    body: &[u8],
    region: &str,
    access_key: &str,
    secret_key: &str,
    session_token: Option<&str>,
) -> Result<BTreeMap<String, String>, ModelError> {
    let mut signing_params_builder = aws_sigv4::SigningParams::builder()
        .access_key(access_key)
        .secret_key(secret_key)
        .region(region)
        .service_name("bedrock")
        .time(SystemTime::now())
        .settings(SigningSettings::default());
    signing_params_builder.set_security_token(session_token);
    let signing_params = signing_params_builder.build().map_err(|error| {
        ModelError::ProviderFailure(format!(
            "failed to build AWS Bedrock signing params: {error}"
        ))
    })?;

    let mut signed_request = http::Request::builder()
        .method(method)
        .uri(url)
        .body(body.to_vec())
        .map_err(|error| {
            ModelError::ProviderFailure(format!(
                "failed to build AWS Bedrock request shell: {error}"
            ))
        })?;

    for (key, value) in headers {
        let header_name = http::header::HeaderName::from_bytes(key.as_bytes()).map_err(|error| {
            ModelError::ProviderFailure(format!("invalid AWS Bedrock header name {key}: {error}"))
        })?;
        let header_value = http::header::HeaderValue::from_str(value).map_err(|error| {
            ModelError::ProviderFailure(format!(
                "invalid AWS Bedrock header value for {key}: {error}"
            ))
        })?;
        signed_request.headers_mut().insert(header_name, header_value);
    }

    let signable_request = SignableRequest::new(
        signed_request.method(),
        signed_request.uri(),
        signed_request.headers(),
        SignableBody::Bytes(signed_request.body()),
    );

    let (instructions, _signature) = sign(signable_request, &signing_params)
        .map_err(|error| {
            ModelError::ProviderFailure(format!("failed to sign AWS Bedrock request: {error}"))
        })?
        .into_parts();

    instructions.apply_to_request(&mut signed_request);

    let mut result = BTreeMap::new();
    for (name, value) in signed_request.headers() {
        let value = value.to_str().map_err(|error| {
            ModelError::ProviderFailure(format!(
                "invalid signed AWS Bedrock header {}: {error}",
                name.as_str()
            ))
        })?;
        result.insert(name.as_str().to_owned(), value.to_owned());
    }
    Ok(result)
}

fn first_env(keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| env::var(key).ok().filter(|value| !value.trim().is_empty()))
}

#[derive(Debug, Deserialize)]
struct CopilotTokenResponse {
    token: String,
}

/// Exchanges a GitHub token for a GitHub Copilot runtime token.
pub fn resolve_copilot_runtime_auth(
    github_token: &str,
    token_url_override: Option<&str>,
    base_url_override: Option<&str>,
) -> Result<CopilotRuntimeAuth, ModelError> {
    let token_url =
        token_url_override.unwrap_or("https://api.github.com/copilot_internal/v2/token");
    let client = reqwest::blocking::Client::builder()
        .build()
        .map_err(|error| {
            ModelError::ProviderFailure(format!(
                "failed to build GitHub Copilot auth client: {error}"
            ))
        })?;

    let response = client
        .get(token_url)
        .header("Accept", "application/json")
        .header("Authorization", format!("Bearer {github_token}"))
        .header("Editor-Version", "vscode/1.96.2")
        .header("User-Agent", "GitHubCopilotChat/0.26.7")
        .header("X-Github-Api-Version", "2025-04-01")
        .send()
        .map_err(|error| {
            ModelError::ProviderFailure(format!(
                "github copilot token exchange failed: {error}"
            ))
        })?;

    let status = response.status();
    let body = response.text().map_err(|error| {
        ModelError::ProviderFailure(format!(
            "failed to read GitHub Copilot token response: {error}"
        ))
    })?;
    if !status.is_success() {
        return Err(ModelError::ProviderFailure(format!(
            "github copilot token exchange returned {status}: {body}"
        )));
    }

    let payload: CopilotTokenResponse = serde_json::from_str(&body).map_err(|error| {
        ModelError::ProviderFailure(format!(
            "failed to parse GitHub Copilot token exchange response: {error}"
        ))
    })?;

    Ok(CopilotRuntimeAuth {
        base_url: base_url_override
            .map(str::to_owned)
            .or_else(|| derive_copilot_api_base_url_from_token(&payload.token))
            .unwrap_or_else(|| "https://api.individual.githubcopilot.com".to_owned()),
        token: payload.token,
    })
}

fn derive_copilot_api_base_url_from_token(token: &str) -> Option<String> {
    let proxy_endpoint = token
        .split(';')
        .find_map(|part| part.trim().strip_prefix("proxy-ep="))
        .map(str::trim)?;
    let mut url = reqwest::Url::parse(proxy_endpoint).ok()?;
    let host = url.host_str()?.replace("proxy.", "api.");
    url.set_host(Some(&host)).ok()?;
    Some(url.origin().ascii_serialization())
}
