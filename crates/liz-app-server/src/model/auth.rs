//! Authentication helpers for provider families that need runtime credentials.

use crate::model::gateway::ModelError;
use aws_credential_types::{provider::ProvideCredentials, Credentials};
use aws_sigv4::http_request::{
    sign, SignableBody, SignableRequest, SignatureLocation, SigningSettings,
};
use aws_types::region::Region;
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use rand::RngCore;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::env;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use std::time::SystemTime;
use yup_oauth2::authenticator::ApplicationDefaultCredentialsTypes;
use yup_oauth2::authenticator::DefaultAuthenticator;
use yup_oauth2::authorized_user::AuthorizedUserSecret;
use yup_oauth2::{
    ApplicationDefaultCredentialsAuthenticator, ApplicationDefaultCredentialsFlowOpts,
    AuthorizedUserAuthenticator,
};

const GOOGLE_CLOUD_PLATFORM_SCOPE: &str = "https://www.googleapis.com/auth/cloud-platform";
const GITHUB_COPILOT_DEVICE_CLIENT_ID: &str = "Ov23li8tweQw6odWQebz";
const OPENAI_CODEX_OAUTH_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
const OPENAI_CODEX_OAUTH_ISSUER: &str = "https://auth.openai.com";
const MINIMAX_OAUTH_CLIENT_ID: &str = "78257093-7e40-4613-99e0-527b14b39113";
const MINIMAX_OAUTH_SCOPE: &str = "group_id profile model.completion";
const MINIMAX_OAUTH_DEVICE_GRANT_TYPE: &str = "urn:ietf:params:oauth:grant-type:user_code";
const BEDROCK_MANTLE_CONTROL_PLANE_URL: &str =
    "https://bedrock.amazonaws.com/?Action=CallWithBearerToken";
const BEDROCK_MANTLE_TOKEN_PREFIX: &str = "bedrock-api-key-";
const BEDROCK_MANTLE_TOKEN_SUFFIX: &str = "&Version=1";
const BEDROCK_MANTLE_TOKEN_EXPIRES_SECONDS: u64 = 7200;
const BEDROCK_MANTLE_CACHE_TTL_MS: u64 = 3600_000;
const BEDROCK_MANTLE_CACHE_SAFETY_WINDOW_MS: u64 = 60_000;
const BEDROCK_MANTLE_SUPPORTED_REGIONS: &[&str] = &[
    "us-east-1",
    "us-east-2",
    "us-west-2",
    "ap-northeast-1",
    "ap-south-1",
    "ap-southeast-3",
    "eu-central-1",
    "eu-west-1",
    "eu-west-2",
    "eu-south-1",
    "eu-north-1",
    "sa-east-1",
];
const OPENAI_CODEX_OAUTH_REQUIRED_SCOPES: &[&str] = &[
    "openid",
    "profile",
    "email",
    "offline_access",
    "model.request",
    "api.responses.write",
];

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

/// Device-code bootstrap information for GitHub Copilot login.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitHubCopilotDeviceCodeAuth {
    /// Verification URL the user should open in a browser.
    pub verification_uri: String,
    /// One-time user code shown to the user.
    pub user_code: String,
    /// Opaque device code used for polling.
    pub device_code: String,
    /// Suggested polling interval in seconds.
    pub interval_seconds: u32,
    /// Final Copilot API base URL for the selected deployment.
    pub api_base_url: String,
}

/// Device-code bootstrap information for MiniMax Portal OAuth.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MiniMaxOAuthDeviceCodeAuth {
    /// Verification URL the user should open.
    pub verification_uri: String,
    /// One-time user code.
    pub user_code: String,
    /// PKCE verifier retained for token polling.
    pub code_verifier: String,
    /// Suggested polling interval in milliseconds.
    pub interval_ms: u32,
    /// Expiry timestamp in milliseconds since epoch.
    pub expires_at_ms: u64,
    /// Selected MiniMax region.
    pub region: String,
}

/// Runtime MiniMax Portal OAuth credentials.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MiniMaxOAuthRuntimeAuth {
    /// Current access token.
    pub access_token: String,
    /// Refresh token retained for future refreshes.
    pub refresh_token: String,
    /// Expiry timestamp in milliseconds since epoch.
    pub expires_at_ms: u64,
    /// Resolved MiniMax resource base URL.
    pub resource_url: String,
}

/// Poll result for MiniMax Portal OAuth completion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MiniMaxOAuthPollOutcome {
    /// Authorization is still pending.
    Pending {
        /// Suggested retry delay in milliseconds.
        retry_after_ms: u32,
    },
    /// Authorization completed successfully.
    Complete {
        /// Resolved runtime credentials.
        auth: MiniMaxOAuthRuntimeAuth,
    },
}

/// Current poll result when completing a GitHub Copilot device-code login flow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitHubCopilotDevicePollOutcome {
    /// Authorization is still pending.
    Pending {
        /// Suggested retry delay in seconds.
        retry_after_seconds: u32,
    },
    /// The caller should back off and poll more slowly.
    SlowDown {
        /// Suggested retry delay in seconds.
        retry_after_seconds: u32,
    },
    /// Authorization completed successfully and yielded a GitHub token.
    Complete {
        /// The resulting GitHub token from the completed device flow.
        github_token: String,
        /// The Copilot API base URL associated with the selected deployment.
        api_base_url: String,
    },
}

/// OAuth bootstrap data used to authorize against GitLab.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitLabOAuthStartAuth {
    /// Final authorize URL the caller should open.
    pub authorize_url: String,
    /// CSRF state value that must round-trip through the callback.
    pub state: String,
    /// PKCE verifier that should be retained until code exchange.
    pub code_verifier: String,
}

/// Runtime GitLab OAuth credential used for agentic chat requests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitLabOAuthRuntimeAuth {
    /// Current access token.
    pub access_token: String,
    /// Optional refresh token.
    pub refresh_token: Option<String>,
    /// Expiry timestamp in milliseconds since epoch.
    pub expires_at_ms: Option<u64>,
}

/// Runtime OpenAI Codex OAuth credentials used for native Codex requests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenAiCodexRuntimeAuth {
    /// Current access token used for Codex requests.
    pub access_token: String,
    /// Refresh token retained for future refreshes.
    pub refresh_token: String,
    /// Expiry timestamp in milliseconds since epoch.
    pub expires_at_ms: u64,
    /// Optional ChatGPT account identifier for organization-scoped access.
    pub account_id: Option<String>,
    /// Optional email derived from JWT claims.
    pub email: Option<String>,
}

/// Input parameters used to resolve a live OpenAI Codex runtime credential.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenAiCodexRuntimeAuthRequest<'a> {
    /// Current access token, if any.
    pub access_token: Option<&'a str>,
    /// Refresh token used to mint a fresh access token.
    pub refresh_token: Option<&'a str>,
    /// Current expiry timestamp in milliseconds since epoch.
    pub expires_at_ms: Option<u64>,
    /// Optional ChatGPT account identifier.
    pub account_id: Option<&'a str>,
    /// Optional override for the OAuth token endpoint.
    pub token_url_override: Option<&'a str>,
}

/// A detected Z.AI endpoint and preferred bootstrap model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZaiDetectedEndpoint {
    /// Stable endpoint identifier.
    pub endpoint: String,
    /// Preferred model id proven to work for this endpoint.
    pub model_id: String,
}

#[derive(Debug, Deserialize)]
struct OpenAiCodexRawTokenResponse {
    access_token: String,
    refresh_token: String,
    expires_in: Option<u64>,
    id_token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAiCodexJwtClaims {
    chatgpt_account_id: Option<String>,
    organizations: Option<Vec<OpenAiCodexOrganization>>,
    email: Option<String>,
    #[serde(rename = "https://api.openai.com/auth")]
    openai_auth: Option<OpenAiCodexAuthClaims>,
    #[serde(rename = "https://api.openai.com/profile")]
    openai_profile: Option<OpenAiCodexProfileClaims>,
}

#[derive(Debug, Deserialize)]
struct OpenAiCodexOrganization {
    id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAiCodexAuthClaims {
    chatgpt_account_id: Option<String>,
    chatgpt_account_user_id: Option<String>,
    chatgpt_user_id: Option<String>,
    user_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAiCodexProfileClaims {
    email: Option<String>,
}

#[derive(Debug, Clone)]
struct CachedBedrockMantleToken {
    token: String,
    expires_at_ms: u64,
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

/// Detects the best Z.AI endpoint for the provided API key.
pub fn detect_zai_endpoint(
    api_key: &str,
    preferred_endpoint: Option<&str>,
) -> Result<Option<ZaiDetectedEndpoint>, ModelError> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .map_err(|error| {
            ModelError::ProviderFailure(format!(
                "failed to build Z.AI detection client: {error}"
            ))
        })?;

    for candidate in zai_endpoint_candidates(preferred_endpoint) {
        if probe_zai_endpoint(&client, api_key, candidate.base_url, &candidate.model_id)? {
            return Ok(Some(ZaiDetectedEndpoint {
                endpoint: candidate.endpoint.to_owned(),
                model_id: candidate.model_id.to_owned(),
            }));
        }
    }

    Ok(None)
}

/// Resolves a Bedrock Mantle bearer token from either an explicit API key or the AWS credential chain.
pub fn resolve_bedrock_mantle_runtime_auth(
    explicit_token: Option<&str>,
    region: &str,
) -> Result<String, ModelError> {
    if let Some(token) = explicit_token.map(str::trim).filter(|value| !value.is_empty()) {
        return Ok(token.to_owned());
    }

    validate_bedrock_mantle_region(region)?;

    let runtime = tokio::runtime::Runtime::new().map_err(|error| {
        ModelError::ProviderFailure(format!(
            "failed to initialize AWS Mantle auth runtime: {error}"
        ))
    })?;
    runtime.block_on(async { resolve_bedrock_mantle_runtime_auth_async(region).await })
}

async fn resolve_bedrock_mantle_runtime_auth_async(region: &str) -> Result<String, ModelError> {
    let now = current_unix_time_ms();
    if let Some(token) = load_cached_bedrock_mantle_token(region, now) {
        return Ok(token);
    }

    let credentials = resolve_aws_credentials(region).await?;
    let token = mint_bedrock_mantle_bearer_token(
        region,
        credentials.access_key_id(),
        credentials.secret_access_key(),
        credentials.session_token(),
    )?;
    store_cached_bedrock_mantle_token(region, token.clone(), mantle_cache_expiry_ms(now, &credentials));
    Ok(token)
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

async fn resolve_aws_credentials(region: &str) -> Result<Credentials, ModelError> {
    if let (Some(access_key), Some(secret_key)) = (
        first_env(&["AWS_ACCESS_KEY_ID"]),
        first_env(&["AWS_SECRET_ACCESS_KEY"]),
    ) {
        return Ok(Credentials::new(
            access_key,
            secret_key,
            first_env(&["AWS_SESSION_TOKEN"]),
            None,
            "env",
        ));
    }

    let config = aws_config::from_env()
        .region(Region::new(region.to_owned()))
        .load()
        .await;
    let provider = config.credentials_provider().ok_or_else(|| {
        ModelError::ProviderFailure(
            "aws credential chain is not available for Amazon Bedrock Mantle".to_owned(),
        )
    })?;
    provider.provide_credentials().await.map_err(|error| {
        ModelError::ProviderFailure(format!(
            "failed to resolve AWS credentials for Amazon Bedrock Mantle: {error}"
        ))
    })
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

fn mint_bedrock_mantle_bearer_token(
    region: &str,
    access_key: &str,
    secret_key: &str,
    session_token: Option<&str>,
) -> Result<String, ModelError> {
    let mut settings = SigningSettings::default();
    settings.signature_location = SignatureLocation::QueryParams;
    settings.expires_in = Some(Duration::from_secs(
        BEDROCK_MANTLE_TOKEN_EXPIRES_SECONDS,
    ));
    let mut signing_params_builder = aws_sigv4::SigningParams::builder()
        .access_key(access_key)
        .secret_key(secret_key)
        .region(region)
        .service_name("bedrock")
        .time(SystemTime::now())
        .settings(settings);
    signing_params_builder.set_security_token(session_token);
    let signing_params = signing_params_builder.build().map_err(|error| {
        ModelError::ProviderFailure(format!(
            "failed to build AWS Bedrock Mantle signing params: {error}"
        ))
    })?;

    let mut signed_request = http::Request::builder()
        .method("POST")
        .uri(BEDROCK_MANTLE_CONTROL_PLANE_URL)
        .body(Vec::<u8>::new())
        .map_err(|error| {
            ModelError::ProviderFailure(format!(
                "failed to build AWS Bedrock Mantle request shell: {error}"
            ))
        })?;

    let signable_request = SignableRequest::new(
        signed_request.method(),
        signed_request.uri(),
        signed_request.headers(),
        SignableBody::Bytes(signed_request.body()),
    );

    let (instructions, _) = sign(signable_request, &signing_params)
        .map_err(|error| {
            ModelError::ProviderFailure(format!(
                "failed to sign AWS Bedrock Mantle bearer-token request: {error}"
            ))
        })?
        .into_parts();
    instructions.apply_to_request(&mut signed_request);

    let presigned_url = signed_request.uri().to_string();
    let encoded = STANDARD.encode(
        presigned_url
            .strip_prefix("https://")
            .unwrap_or(&presigned_url)
            .as_bytes(),
    );
    Ok(format!(
        "{BEDROCK_MANTLE_TOKEN_PREFIX}{encoded}{BEDROCK_MANTLE_TOKEN_SUFFIX}"
    ))
}

fn validate_bedrock_mantle_region(region: &str) -> Result<(), ModelError> {
    let trimmed = region.trim();
    if BEDROCK_MANTLE_SUPPORTED_REGIONS.contains(&trimmed) {
        return Ok(());
    }

    Err(ModelError::ProviderFailure(format!(
        "amazon-bedrock-mantle is not available in region {trimmed}"
    )))
}

fn mantle_cache_expiry_ms(now_ms: u64, credentials: &Credentials) -> u64 {
    let preferred_expiry_ms = now_ms + BEDROCK_MANTLE_CACHE_TTL_MS;
    let credential_expiry_ms = credentials
        .expiry()
        .and_then(|expiry| expiry.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis() as u64)
        .map(|expiry_ms| expiry_ms.saturating_sub(BEDROCK_MANTLE_CACHE_SAFETY_WINDOW_MS));

    credential_expiry_ms
        .map(|expiry_ms| preferred_expiry_ms.min(expiry_ms))
        .unwrap_or(preferred_expiry_ms)
}

fn bedrock_mantle_token_cache() -> &'static Mutex<BTreeMap<String, CachedBedrockMantleToken>> {
    static CACHE: OnceLock<Mutex<BTreeMap<String, CachedBedrockMantleToken>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(BTreeMap::new()))
}

fn load_cached_bedrock_mantle_token(region: &str, now_ms: u64) -> Option<String> {
    bedrock_mantle_token_cache()
        .lock()
        .ok()
        .and_then(|cache| cache.get(region).cloned())
        .filter(|entry| entry.expires_at_ms > now_ms)
        .map(|entry| entry.token)
}

fn store_cached_bedrock_mantle_token(region: &str, token: String, expires_at_ms: u64) {
    if let Ok(mut cache) = bedrock_mantle_token_cache().lock() {
        cache.insert(
            region.to_owned(),
            CachedBedrockMantleToken {
                token,
                expires_at_ms,
            },
        );
    }
}

fn first_env(keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| env::var(key).ok().filter(|value| !value.trim().is_empty()))
}

#[derive(Clone, Copy)]
struct ZaiEndpointCandidate {
    endpoint: &'static str,
    base_url: &'static str,
    model_id: &'static str,
}

fn zai_endpoint_candidates(preferred_endpoint: Option<&str>) -> Vec<ZaiEndpointCandidate> {
    match preferred_endpoint.map(normalize_zai_endpoint_id).as_deref() {
        Some("coding-global") => vec![
            ZaiEndpointCandidate {
                endpoint: "coding-global",
                base_url: zai_detection_base_url("coding-global"),
                model_id: "glm-5.1",
            },
            ZaiEndpointCandidate {
                endpoint: "coding-global",
                base_url: zai_detection_base_url("coding-global"),
                model_id: "glm-4.7",
            },
        ],
        Some("coding-cn") => vec![
            ZaiEndpointCandidate {
                endpoint: "coding-cn",
                base_url: zai_detection_base_url("coding-cn"),
                model_id: "glm-5.1",
            },
            ZaiEndpointCandidate {
                endpoint: "coding-cn",
                base_url: zai_detection_base_url("coding-cn"),
                model_id: "glm-4.7",
            },
        ],
        Some("cn") => vec![ZaiEndpointCandidate {
            endpoint: "cn",
            base_url: zai_detection_base_url("cn"),
            model_id: "glm-5.1",
        }],
        Some("global") => vec![ZaiEndpointCandidate {
            endpoint: "global",
            base_url: zai_detection_base_url("global"),
            model_id: "glm-5.1",
        }],
        _ => vec![
            ZaiEndpointCandidate {
                endpoint: "global",
                base_url: zai_detection_base_url("global"),
                model_id: "glm-5.1",
            },
            ZaiEndpointCandidate {
                endpoint: "cn",
                base_url: zai_detection_base_url("cn"),
                model_id: "glm-5.1",
            },
            ZaiEndpointCandidate {
                endpoint: "coding-global",
                base_url: zai_detection_base_url("coding-global"),
                model_id: "glm-5.1",
            },
            ZaiEndpointCandidate {
                endpoint: "coding-global",
                base_url: zai_detection_base_url("coding-global"),
                model_id: "glm-4.7",
            },
            ZaiEndpointCandidate {
                endpoint: "coding-cn",
                base_url: zai_detection_base_url("coding-cn"),
                model_id: "glm-5.1",
            },
            ZaiEndpointCandidate {
                endpoint: "coding-cn",
                base_url: zai_detection_base_url("coding-cn"),
                model_id: "glm-4.7",
            },
        ],
    }
}

fn normalize_zai_endpoint_id(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "coding_global" | "coding-global" => "coding-global".to_owned(),
        "coding_cn" | "coding-cn" => "coding-cn".to_owned(),
        "cn" => "cn".to_owned(),
        "global" => "global".to_owned(),
        other => other.to_owned(),
    }
}

fn zai_detection_base_url(endpoint: &str) -> &'static str {
    match endpoint {
        "coding-global" => env_or_static(
            "LIZ_ZAI_DETECT_CODING_GLOBAL_BASE_URL",
            "https://api.z.ai/api/coding/paas/v4",
        ),
        "coding-cn" => env_or_static(
            "LIZ_ZAI_DETECT_CODING_CN_BASE_URL",
            "https://open.bigmodel.cn/api/coding/paas/v4",
        ),
        "cn" => env_or_static("LIZ_ZAI_DETECT_CN_BASE_URL", "https://open.bigmodel.cn/api/paas/v4"),
        _ => env_or_static("LIZ_ZAI_DETECT_GLOBAL_BASE_URL", "https://api.z.ai/api/paas/v4"),
    }
}

fn env_or_static(key: &str, fallback: &'static str) -> &'static str {
    static CACHE: OnceLock<Mutex<BTreeMap<String, &'static str>>> = OnceLock::new();
    if let Ok(value) = env::var(key) {
        if !value.trim().is_empty() {
            let cache = CACHE.get_or_init(|| Mutex::new(BTreeMap::new()));
            if let Ok(mut cache) = cache.lock() {
                if let Some(existing) = cache.get(&value) {
                    return existing;
                }
                let leaked = Box::leak(value.clone().into_boxed_str());
                cache.insert(value, leaked);
                return leaked;
            }
        }
    }
    fallback
}

fn probe_zai_endpoint(
    client: &reqwest::blocking::Client,
    api_key: &str,
    base_url: &str,
    model_id: &str,
) -> Result<bool, ModelError> {
    let response = client
        .post(format!("{}/chat/completions", base_url.trim_end_matches('/')))
        .header("Authorization", format!("Bearer {api_key}"))
        .json(&serde_json::json!({
            "model": model_id,
            "messages": [{"role": "user", "content": "ping"}],
            "stream": false,
            "max_tokens": 1,
        }))
        .send()
        .map_err(|error| {
            ModelError::ProviderFailure(format!(
                "Z.AI endpoint probe failed for {base_url}: {error}"
            ))
        })?;

    Ok(response.status().is_success())
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

fn normalize_github_copilot_domain(enterprise_url: Option<&str>) -> String {
    let Some(value) = enterprise_url.map(str::trim).filter(|value| !value.is_empty()) else {
        return "github.com".to_owned();
    };

    let candidate = if value.contains("://") {
        value.to_owned()
    } else {
        format!("https://{value}")
    };
    reqwest::Url::parse(&candidate)
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned))
        .unwrap_or_else(|| "github.com".to_owned())
}

fn copilot_api_base_url_for_domain(enterprise_url: Option<&str>) -> String {
    let Some(raw) = enterprise_url.map(str::trim).filter(|value| !value.is_empty()) else {
        return "https://api.githubcopilot.com".to_owned();
    };
    format!("https://copilot-api.{}", normalize_github_copilot_domain(Some(raw)))
}

fn post_json_body<T: for<'de> Deserialize<'de>>(
    url: &str,
    body: &serde_json::Value,
    headers: &BTreeMap<String, String>,
) -> Result<T, ModelError> {
    let client = reqwest::blocking::Client::builder()
        .build()
        .map_err(|error| {
            ModelError::ProviderFailure(format!("failed to build HTTP client: {error}"))
        })?;
    let response = client
        .post(url)
        .headers(crate::model::http::build_headers(headers)?)
        .json(body)
        .send()
        .map_err(|error| {
            ModelError::ProviderFailure(format!("provider request failed: {error}"))
        })?;

    let status = response.status();
    let payload = response.text().map_err(|error| {
        ModelError::ProviderFailure(format!("failed to read response body: {error}"))
    })?;
    if !status.is_success() {
        return Err(ModelError::ProviderFailure(format!(
            "provider request returned {status}: {payload}"
        )));
    }

    serde_json::from_str(&payload).map_err(|error| {
        ModelError::ProviderFailure(format!("failed to parse JSON response: {error}"))
    })
}

/// Starts a GitHub Copilot device-code login flow.
pub fn start_github_copilot_device_authorization(
    enterprise_url: Option<&str>,
    device_code_url_override: Option<&str>,
) -> Result<GitHubCopilotDeviceCodeAuth, ModelError> {
    let domain = normalize_github_copilot_domain(enterprise_url);
    let device_code_url = device_code_url_override
        .map(str::to_owned)
        .or_else(|| first_env(&["LIZ_GITHUB_COPILOT_DEVICE_CODE_URL"]))
        .unwrap_or_else(|| format!("https://{domain}/login/device/code"));
    let response: GitHubCopilotDeviceCodeResponse = post_json_body(
        &device_code_url,
        &serde_json::json!({
            "client_id": GITHUB_COPILOT_DEVICE_CLIENT_ID,
            "scope": "read:user",
        }),
        &std::collections::BTreeMap::from([
            ("Accept".to_owned(), "application/json".to_owned()),
            ("Content-Type".to_owned(), "application/json".to_owned()),
            ("User-Agent".to_owned(), "liz-app-server".to_owned()),
        ]),
    )?;

    Ok(GitHubCopilotDeviceCodeAuth {
        verification_uri: response.verification_uri,
        user_code: response.user_code,
        device_code: response.device_code,
        interval_seconds: response.interval.max(1),
        api_base_url: copilot_api_base_url_for_domain(enterprise_url),
    })
}

/// Polls a GitHub Copilot device-code login flow until a GitHub token is available.
pub fn poll_github_copilot_device_authorization(
    device_code: &str,
    enterprise_url: Option<&str>,
    interval_seconds: Option<u32>,
    access_token_url_override: Option<&str>,
) -> Result<GitHubCopilotDevicePollOutcome, ModelError> {
    let domain = normalize_github_copilot_domain(enterprise_url);
    let access_token_url = access_token_url_override
        .map(str::to_owned)
        .or_else(|| first_env(&["LIZ_GITHUB_COPILOT_ACCESS_TOKEN_URL"]))
        .unwrap_or_else(|| format!("https://{domain}/login/oauth/access_token"));
    let headers = std::collections::BTreeMap::from([
        ("Accept".to_owned(), "application/json".to_owned()),
        ("Content-Type".to_owned(), "application/json".to_owned()),
        ("User-Agent".to_owned(), "liz-app-server".to_owned()),
    ]);
    let response: GitHubCopilotDevicePollResponseBody = post_json_body(
        &access_token_url,
        &serde_json::json!({
            "client_id": GITHUB_COPILOT_DEVICE_CLIENT_ID,
            "device_code": device_code,
            "grant_type": "urn:ietf:params:oauth:grant-type:device_code",
        }),
        &headers,
    )?;

    if let Some(access_token) = response.access_token {
        return Ok(GitHubCopilotDevicePollOutcome::Complete {
            github_token: access_token,
            api_base_url: copilot_api_base_url_for_domain(enterprise_url),
        });
    }

    let retry_after_seconds = response
        .interval
        .or(interval_seconds)
        .unwrap_or(5)
        .saturating_add(3);
    match response.error.as_deref() {
        Some("slow_down") => Ok(GitHubCopilotDevicePollOutcome::SlowDown {
            retry_after_seconds,
        }),
        Some("authorization_pending") | Some("expired_token") | Some("incorrect_device_code") => {
            Ok(GitHubCopilotDevicePollOutcome::Pending {
                retry_after_seconds,
            })
        }
        Some(error) => Err(ModelError::ProviderFailure(format!(
            "github copilot device-code login failed: {error}"
        ))),
        None => Err(ModelError::ProviderFailure(
            "github copilot device-code login returned neither access token nor error".to_owned(),
        )),
    }
}

/// Starts a MiniMax Portal OAuth device flow.
pub fn start_minimax_oauth_authorization(
    region: &str,
) -> Result<MiniMaxOAuthDeviceCodeAuth, ModelError> {
    let normalized_region = normalize_minimax_region(region)?;
    let code_verifier = generate_oauth_random_string(43);
    let code_challenge = pkce_code_challenge(&code_verifier);
    let state = generate_oauth_random_string(32);
    let code_endpoint = first_env(&["LIZ_MINIMAX_OAUTH_CODE_URL"])
        .unwrap_or_else(|| minimax_code_endpoint(normalized_region));
    let response: MiniMaxOAuthAuthorizationResponse = post_form_json(
        &code_endpoint,
        &[
            ("response_type", "code"),
            ("client_id", MINIMAX_OAUTH_CLIENT_ID),
            ("scope", MINIMAX_OAUTH_SCOPE),
            ("code_challenge", &code_challenge),
            ("code_challenge_method", "S256"),
            ("state", &state),
        ],
    )?;

    if !response.state.is_empty() && response.state != state {
        return Err(ModelError::ProviderFailure(
            "MiniMax OAuth state mismatch during authorization".to_owned(),
        ));
    }

    Ok(MiniMaxOAuthDeviceCodeAuth {
        verification_uri: response.verification_uri,
        user_code: response.user_code,
        code_verifier,
        interval_ms: response.interval.unwrap_or(2000),
        expires_at_ms: normalize_minimax_timestamp(response.expired_in),
        region: normalized_region.to_owned(),
    })
}

/// Polls a MiniMax Portal OAuth device flow.
pub fn poll_minimax_oauth_authorization(
    region: &str,
    user_code: &str,
    code_verifier: &str,
    interval_ms: Option<u32>,
) -> Result<MiniMaxOAuthPollOutcome, ModelError> {
    let normalized_region = normalize_minimax_region(region)?;
    let (_base_url, default_token_endpoint) = minimax_oauth_endpoints(normalized_region);
    let token_endpoint = first_env(&["LIZ_MINIMAX_OAUTH_TOKEN_URL"])
        .unwrap_or(default_token_endpoint);
    let response: MiniMaxOAuthTokenResponseBody = post_form_json(
        &token_endpoint,
        &[
            ("grant_type", MINIMAX_OAUTH_DEVICE_GRANT_TYPE),
            ("client_id", MINIMAX_OAUTH_CLIENT_ID),
            ("user_code", user_code),
            ("code_verifier", code_verifier),
        ],
    )?;

    match response.status.as_deref() {
        Some("success") => Ok(MiniMaxOAuthPollOutcome::Complete {
            auth: materialize_minimax_oauth_runtime_auth(
                response,
                minimax_default_resource_url(normalized_region),
            )?,
        }),
        Some("error") => Err(ModelError::ProviderFailure(
            "MiniMax OAuth authorization failed".to_owned(),
        )),
        _ => Ok(MiniMaxOAuthPollOutcome::Pending {
            retry_after_ms: interval_ms.unwrap_or(2000).max(2000),
        }),
    }
}

/// Refreshes a MiniMax Portal OAuth credential.
pub fn refresh_minimax_oauth_token(
    region: &str,
    refresh_token: &str,
    fallback_resource_url: Option<&str>,
) -> Result<MiniMaxOAuthRuntimeAuth, ModelError> {
    let normalized_region = normalize_minimax_region(region)?;
    let (_base_url, default_token_endpoint) = minimax_oauth_endpoints(normalized_region);
    let token_endpoint = first_env(&["LIZ_MINIMAX_OAUTH_TOKEN_URL"])
        .unwrap_or(default_token_endpoint);
    let response: MiniMaxOAuthTokenResponseBody = post_form_json(
        &token_endpoint,
        &[
            ("grant_type", "refresh_token"),
            ("client_id", MINIMAX_OAUTH_CLIENT_ID),
            ("refresh_token", refresh_token),
        ],
    )?;

    materialize_minimax_oauth_runtime_auth(
        response,
        fallback_resource_url.unwrap_or(minimax_default_resource_url(normalized_region)),
    )
}

/// Resolves a live MiniMax Portal OAuth credential, refreshing it when required.
pub fn resolve_minimax_oauth_runtime_auth(
    access_token: Option<&str>,
    refresh_token: Option<&str>,
    expires_at_ms: Option<u64>,
    region: &str,
    resource_url: Option<&str>,
) -> Result<MiniMaxOAuthRuntimeAuth, ModelError> {
    let normalized_region = normalize_minimax_region(region)?;
    let now = current_unix_time_ms();
    if let (Some(access_token), Some(expires_at_ms)) = (
        access_token.map(str::trim).filter(|value| !value.is_empty()),
        expires_at_ms,
    ) {
        if expires_at_ms > now {
            return Ok(MiniMaxOAuthRuntimeAuth {
                access_token: access_token.to_owned(),
                refresh_token: refresh_token.unwrap_or_default().to_owned(),
                expires_at_ms,
                resource_url: resource_url
                    .map(str::to_owned)
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or_else(|| minimax_default_resource_url(normalized_region).to_owned()),
            });
        }
    }

    let refresh_token = refresh_token
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            ModelError::ProviderFailure(
                "MiniMax OAuth refresh requires a refresh token".to_owned(),
            )
        })?;
    refresh_minimax_oauth_token(
        normalized_region,
        refresh_token,
        resource_url,
    )
}

/// Starts a GitLab OAuth authorization flow using the standard authorize endpoint and PKCE.
pub fn start_gitlab_oauth_authorization(
    instance_url: &str,
    client_id: &str,
    redirect_uri: &str,
    scopes: &[String],
) -> Result<GitLabOAuthStartAuth, ModelError> {
    let state = generate_oauth_random_string(32);
    let code_verifier = generate_oauth_random_string(43);
    let code_challenge = pkce_code_challenge(&code_verifier);

    let mut url = reqwest::Url::parse(&format!(
        "{}/oauth/authorize",
        instance_url.trim_end_matches('/')
    ))
    .map_err(|error| {
        ModelError::ProviderFailure(format!("failed to build GitLab authorize URL: {error}"))
    })?;
    url.query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", client_id)
        .append_pair("redirect_uri", redirect_uri)
        .append_pair("state", &state)
        .append_pair("code_challenge", &code_challenge)
        .append_pair("code_challenge_method", "S256");
    if !scopes.is_empty() {
        url.query_pairs_mut()
            .append_pair("scope", &scopes.join(" "));
    }

    Ok(GitLabOAuthStartAuth {
        authorize_url: url.to_string(),
        state,
        code_verifier,
    })
}

/// Completes a GitLab OAuth authorization flow by exchanging the callback code for tokens.
pub fn exchange_gitlab_oauth_code(
    instance_url: &str,
    client_id: &str,
    client_secret: Option<&str>,
    redirect_uri: &str,
    code: &str,
    code_verifier: Option<&str>,
    token_url_override: Option<&str>,
) -> Result<GitLabOAuthRuntimeAuth, ModelError> {
    let url = token_url_override
        .map(str::to_owned)
        .unwrap_or_else(|| format!("{}/oauth/token", instance_url.trim_end_matches('/')));
    let mut body = vec![
        ("grant_type", "authorization_code"),
        ("client_id", client_id),
        ("redirect_uri", redirect_uri),
        ("code", code),
    ];
    if let Some(client_secret) = client_secret.filter(|value| !value.trim().is_empty()) {
        body.push(("client_secret", client_secret));
    }
    if let Some(code_verifier) = code_verifier.filter(|value| !value.trim().is_empty()) {
        body.push(("code_verifier", code_verifier));
    }
    materialize_gitlab_oauth_runtime_auth(post_form_json(&url, &body)?)
}

/// Refreshes a GitLab OAuth credential using the standard token endpoint.
pub fn refresh_gitlab_oauth_token(
    instance_url: &str,
    client_id: &str,
    client_secret: Option<&str>,
    refresh_token: &str,
    token_url_override: Option<&str>,
) -> Result<GitLabOAuthRuntimeAuth, ModelError> {
    let url = token_url_override
        .map(str::to_owned)
        .unwrap_or_else(|| format!("{}/oauth/token", instance_url.trim_end_matches('/')));
    let mut body = vec![
        ("grant_type", "refresh_token"),
        ("client_id", client_id),
        ("refresh_token", refresh_token),
    ];
    if let Some(client_secret) = client_secret.filter(|value| !value.trim().is_empty()) {
        body.push(("client_secret", client_secret));
    }
    materialize_gitlab_oauth_runtime_auth(post_form_json(&url, &body)?)
}

/// Resolves a live GitLab OAuth credential, refreshing it when required.
pub fn resolve_gitlab_oauth_runtime_auth(
    access_token: Option<&str>,
    refresh_token: Option<&str>,
    expires_at_ms: Option<u64>,
    instance_url: &str,
    client_id: Option<&str>,
    client_secret: Option<&str>,
    token_url_override: Option<&str>,
) -> Result<GitLabOAuthRuntimeAuth, ModelError> {
    let now = current_unix_time_ms();
    if let (Some(access_token), Some(expires_at_ms)) = (
        access_token.map(str::trim).filter(|value| !value.is_empty()),
        expires_at_ms,
    ) {
        if expires_at_ms > now {
            return Ok(GitLabOAuthRuntimeAuth {
                access_token: access_token.to_owned(),
                refresh_token: refresh_token.map(str::to_owned),
                expires_at_ms: Some(expires_at_ms),
            });
        }
    }

    let refresh_token = refresh_token
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            ModelError::ProviderFailure(
                "gitlab oauth requires a refresh token when the access token is expired".to_owned(),
            )
        })?;
    let client_id = client_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            ModelError::ProviderFailure(
                "gitlab oauth refresh requires a client id".to_owned(),
            )
        })?;
    refresh_gitlab_oauth_token(
        instance_url,
        client_id,
        client_secret,
        refresh_token,
        token_url_override,
    )
}

#[derive(Debug, Deserialize)]
struct GitHubCopilotDeviceCodeResponse {
    verification_uri: String,
    user_code: String,
    device_code: String,
    interval: u32,
}

#[derive(Debug, Deserialize)]
struct GitHubCopilotDevicePollResponseBody {
    access_token: Option<String>,
    error: Option<String>,
    interval: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct GitLabOAuthTokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct MiniMaxOAuthAuthorizationResponse {
    user_code: String,
    verification_uri: String,
    expired_in: u64,
    interval: Option<u32>,
    state: String,
}

#[derive(Debug, Deserialize)]
struct MiniMaxOAuthTokenResponseBody {
    status: Option<String>,
    access_token: Option<String>,
    refresh_token: Option<String>,
    expired_in: Option<u64>,
    resource_url: Option<String>,
}

/// Normalizes an OpenAI Codex authorize URL so the required OAuth scopes are always present.
pub fn normalize_openai_codex_authorize_url(raw_url: &str) -> String {
    let trimmed = raw_url.trim();
    let Ok(mut url) = reqwest::Url::parse(trimmed) else {
        return raw_url.to_owned();
    };
    if !url
        .host_str()
        .map(|host| host == "auth.openai.com" || host.ends_with(".openai.com"))
        .unwrap_or(false)
    {
        return raw_url.to_owned();
    }
    if !url.path().trim_end_matches('/').ends_with("/oauth/authorize") {
        return raw_url.to_owned();
    }

    let mut scopes = url
        .query_pairs()
        .find(|(key, _)| key == "scope")
        .map(|(_, value)| value.split_whitespace().map(str::to_owned).collect::<Vec<_>>())
        .unwrap_or_default();
    for required in OPENAI_CODEX_OAUTH_REQUIRED_SCOPES {
        if !scopes.iter().any(|scope| scope == required) {
            scopes.push((*required).to_owned());
        }
    }

    let mut query = url.query_pairs().into_owned().collect::<Vec<_>>();
    query.retain(|(key, _)| key != "scope");
    query.push(("scope".to_owned(), scopes.join(" ")));
    url.query_pairs_mut().clear().extend_pairs(query);
    url.to_string()
}

/// Builds an OpenAI Codex OAuth authorize URL with the required PKCE and scope parameters.
pub fn build_openai_codex_authorize_url(
    redirect_uri: &str,
    code_challenge: &str,
    state: &str,
    originator: &str,
) -> Result<String, ModelError> {
    let mut url = reqwest::Url::parse(&format!("{OPENAI_CODEX_OAUTH_ISSUER}/oauth/authorize"))
        .map_err(|error| {
            ModelError::ProviderFailure(format!(
                "failed to build OpenAI Codex authorize URL: {error}"
            ))
        })?;
    url.query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", OPENAI_CODEX_OAUTH_CLIENT_ID)
        .append_pair("redirect_uri", redirect_uri)
        .append_pair("scope", &OPENAI_CODEX_OAUTH_REQUIRED_SCOPES.join(" "))
        .append_pair("code_challenge", code_challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("id_token_add_organizations", "true")
        .append_pair("codex_cli_simplified_flow", "true")
        .append_pair("state", state)
        .append_pair("originator", originator);
    Ok(url.to_string())
}

/// Exchanges an OpenAI Codex OAuth authorization code for runtime tokens.
pub fn exchange_openai_codex_authorization_code(
    code: &str,
    redirect_uri: &str,
    code_verifier: &str,
    token_url_override: Option<&str>,
) -> Result<OpenAiCodexRuntimeAuth, ModelError> {
    let body = [
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", redirect_uri),
        ("client_id", OPENAI_CODEX_OAUTH_CLIENT_ID),
        ("code_verifier", code_verifier),
    ];
    let response = post_form(
        token_url_override.unwrap_or(&format!("{OPENAI_CODEX_OAUTH_ISSUER}/oauth/token")),
        &body,
    )?;
    Ok(materialize_openai_codex_runtime_auth(response, None))
}

/// Refreshes an OpenAI Codex OAuth access token.
pub fn refresh_openai_codex_token(
    refresh_token: &str,
    token_url_override: Option<&str>,
) -> Result<OpenAiCodexRuntimeAuth, ModelError> {
    let body = [
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token),
        ("client_id", OPENAI_CODEX_OAUTH_CLIENT_ID),
    ];
    let response = post_form(
        token_url_override.unwrap_or(&format!("{OPENAI_CODEX_OAUTH_ISSUER}/oauth/token")),
        &body,
    )?;
    Ok(materialize_openai_codex_runtime_auth(response, None))
}

/// Resolves a live OpenAI Codex runtime credential, refreshing it when needed.
pub fn resolve_openai_codex_runtime_auth(
    params: OpenAiCodexRuntimeAuthRequest<'_>,
) -> Result<OpenAiCodexRuntimeAuth, ModelError> {
    let now_ms = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_err(|error| {
            ModelError::ProviderFailure(format!("failed to read system clock for Codex OAuth: {error}"))
        })?
        .as_millis() as u64;

    let current_access = params
        .access_token
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let current_refresh = params
        .refresh_token
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            ModelError::ProviderFailure(
                "openai-codex requires a refresh token or externally managed OAuth credential"
                    .to_owned(),
            )
        })?;

    if let (Some(access), Some(expires_at_ms)) = (current_access, params.expires_at_ms) {
        if expires_at_ms > now_ms {
            let claims = decode_openai_codex_jwt_claims(access);
            return Ok(OpenAiCodexRuntimeAuth {
                access_token: access.to_owned(),
                refresh_token: current_refresh.to_owned(),
                expires_at_ms,
                account_id: params
                    .account_id
                    .map(str::to_owned)
                    .or_else(|| claims.as_ref().and_then(extract_openai_codex_account_id)),
                email: claims
                    .as_ref()
                    .and_then(extract_openai_codex_email),
            });
        }
    }

    let refreshed = refresh_openai_codex_token(current_refresh, params.token_url_override)?;
    Ok(OpenAiCodexRuntimeAuth {
        account_id: refreshed
            .account_id
            .or_else(|| params.account_id.map(str::to_owned)),
        ..refreshed
    })
}

fn post_form(url: &str, body: &[(&str, &str)]) -> Result<OpenAiCodexRawTokenResponse, ModelError> {
    let client = reqwest::blocking::Client::builder()
        .build()
        .map_err(|error| {
            ModelError::ProviderFailure(format!(
                "failed to build OpenAI Codex OAuth client: {error}"
            ))
        })?;
    let response = client
        .post(url)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .form(body)
        .send()
        .map_err(|error| {
            ModelError::ProviderFailure(format!(
                "OpenAI Codex OAuth request failed: {error}"
            ))
        })?;

    let status = response.status();
    let body = response.text().map_err(|error| {
        ModelError::ProviderFailure(format!(
            "failed to read OpenAI Codex OAuth response body: {error}"
        ))
    })?;
    if !status.is_success() {
        return Err(ModelError::ProviderFailure(format!(
            "OpenAI Codex OAuth endpoint returned {status}: {body}"
        )));
    }

    serde_json::from_str(&body).map_err(|error| {
        ModelError::ProviderFailure(format!(
            "failed to parse OpenAI Codex OAuth response: {error}"
        ))
    })
}

fn post_form_json<T: for<'de> Deserialize<'de>>(
    url: &str,
    body: &[(&str, &str)],
) -> Result<T, ModelError> {
    let client = reqwest::blocking::Client::builder()
        .build()
        .map_err(|error| {
            ModelError::ProviderFailure(format!("failed to build OAuth client: {error}"))
        })?;
    let response = client
        .post(url)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .form(body)
        .send()
        .map_err(|error| {
            ModelError::ProviderFailure(format!("OAuth request failed: {error}"))
        })?;

    let status = response.status();
    let body = response.text().map_err(|error| {
        ModelError::ProviderFailure(format!("failed to read OAuth response body: {error}"))
    })?;
    if !status.is_success() {
        return Err(ModelError::ProviderFailure(format!(
            "OAuth endpoint returned {status}: {body}"
        )));
    }

    serde_json::from_str(&body).map_err(|error| {
        ModelError::ProviderFailure(format!("failed to parse OAuth response: {error}"))
    })
}

fn materialize_gitlab_oauth_runtime_auth(
    response: GitLabOAuthTokenResponse,
) -> Result<GitLabOAuthRuntimeAuth, ModelError> {
    Ok(GitLabOAuthRuntimeAuth {
        access_token: response.access_token,
        refresh_token: response.refresh_token,
        expires_at_ms: response
            .expires_in
            .map(|expires_in| current_unix_time_ms() + expires_in * 1000),
    })
}

fn materialize_minimax_oauth_runtime_auth(
    response: MiniMaxOAuthTokenResponseBody,
    fallback_resource_url: &str,
) -> Result<MiniMaxOAuthRuntimeAuth, ModelError> {
    let access_token = response.access_token.ok_or_else(|| {
        ModelError::ProviderFailure(
            "MiniMax OAuth returned an incomplete token payload (missing access token)"
                .to_owned(),
        )
    })?;
    let refresh_token = response.refresh_token.ok_or_else(|| {
        ModelError::ProviderFailure(
            "MiniMax OAuth returned an incomplete token payload (missing refresh token)"
                .to_owned(),
        )
    })?;
    let expires_at_ms = response
        .expired_in
        .map(normalize_minimax_timestamp)
        .ok_or_else(|| {
            ModelError::ProviderFailure(
                "MiniMax OAuth returned an incomplete token payload (missing expiry)".to_owned(),
            )
        })?;

    Ok(MiniMaxOAuthRuntimeAuth {
        access_token,
        refresh_token,
        expires_at_ms,
        resource_url: response
            .resource_url
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| fallback_resource_url.to_owned()),
    })
}

fn generate_oauth_random_string(length: usize) -> String {
    const ALPHABET: &[u8] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~";
    let mut bytes = vec![0_u8; length];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    bytes
        .into_iter()
        .map(|value| ALPHABET[usize::from(value) % ALPHABET.len()] as char)
        .collect()
}

fn pkce_code_challenge(verifier: &str) -> String {
    let hash = Sha256::digest(verifier.as_bytes());
    STANDARD
        .encode(hash)
        .replace('+', "-")
        .replace('/', "_")
        .trim_end_matches('=')
        .to_owned()
}

fn materialize_openai_codex_runtime_auth(
    response: OpenAiCodexRawTokenResponse,
    fallback_account_id: Option<String>,
) -> OpenAiCodexRuntimeAuth {
    let claims = response
        .id_token
        .as_deref()
        .and_then(decode_openai_codex_jwt_claims)
        .or_else(|| decode_openai_codex_jwt_claims(&response.access_token));
    let account_id = claims
        .as_ref()
        .and_then(extract_openai_codex_account_id)
        .or(fallback_account_id);
    let email = claims.as_ref().and_then(extract_openai_codex_email);

    OpenAiCodexRuntimeAuth {
        access_token: response.access_token,
        refresh_token: response.refresh_token,
        expires_at_ms: current_unix_time_ms() + response.expires_in.unwrap_or(3600) * 1000,
        account_id,
        email,
    }
}

fn current_unix_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

fn minimax_oauth_endpoints(region: &str) -> (&'static str, String) {
    let base_url = if region == "cn" {
        "https://api.minimaxi.com"
    } else {
        "https://api.minimax.io"
    };
    (base_url, format!("{base_url}/oauth/token"))
}

fn minimax_code_endpoint(region: &str) -> String {
    let base_url = if region == "cn" {
        "https://api.minimaxi.com"
    } else {
        "https://api.minimax.io"
    };
    format!("{base_url}/oauth/code")
}

fn minimax_default_resource_url(region: &str) -> &'static str {
    if region == "cn" {
        "https://api.minimaxi.com/anthropic"
    } else {
        "https://api.minimax.io/anthropic"
    }
}

fn normalize_minimax_region(region: &str) -> Result<&str, ModelError> {
    match region.trim().to_ascii_lowercase().as_str() {
        "global" => Ok("global"),
        "cn" => Ok("cn"),
        other => Err(ModelError::ProviderFailure(format!(
            "unsupported MiniMax region {other}"
        ))),
    }
}

fn normalize_minimax_timestamp(raw: u64) -> u64 {
    if raw > 1_000_000_000_000 {
        raw
    } else if raw > 10_000_000_000 {
        raw * 1000
    } else {
        current_unix_time_ms() + raw * 1000
    }
}

fn decode_openai_codex_jwt_claims(token: &str) -> Option<OpenAiCodexJwtClaims> {
    let payload = token.split('.').nth(1)?;
    let bytes = base64url_decode(payload)?;
    serde_json::from_slice(&bytes).ok()
}

fn extract_openai_codex_account_id(claims: &OpenAiCodexJwtClaims) -> Option<String> {
    claims
        .chatgpt_account_id
        .as_ref()
        .cloned()
        .or_else(|| claims.openai_auth.as_ref()?.chatgpt_account_id.clone())
        .or_else(|| {
            claims
                .organizations
                .as_ref()?
                .first()?
                .id
                .as_ref()
                .cloned()
        })
}

fn extract_openai_codex_email(claims: &OpenAiCodexJwtClaims) -> Option<String> {
    claims
        .email
        .as_ref()
        .cloned()
        .or_else(|| claims.openai_profile.as_ref()?.email.clone())
}

/// Resolves a stable fallback identity from OpenAI Codex JWT claims when email is unavailable.
pub fn resolve_openai_codex_stable_subject(access_token: &str) -> Option<String> {
    let claims = decode_openai_codex_jwt_claims(access_token)?;
    claims
        .openai_auth
        .as_ref()
        .and_then(|auth| {
            auth.chatgpt_account_user_id
                .clone()
                .or_else(|| auth.chatgpt_user_id.clone())
                .or_else(|| auth.user_id.clone())
        })
}

fn base64url_decode(input: &str) -> Option<Vec<u8>> {
    let mut normalized = input.replace('-', "+").replace('_', "/");
    while normalized.len() % 4 != 0 {
        normalized.push('=');
    }
    STANDARD.decode(normalized).ok()
}

#[cfg(test)]
mod tests {
    use super::{
        build_openai_codex_authorize_url, detect_zai_endpoint, normalize_openai_codex_authorize_url,
        resolve_openai_codex_runtime_auth, OpenAiCodexRuntimeAuthRequest,
    };
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    #[test]
    fn normalize_openai_codex_authorize_url_adds_required_scopes() {
        let normalized = normalize_openai_codex_authorize_url(
            "https://auth.openai.com/oauth/authorize?scope=openid%20profile&state=abc",
        );
        assert!(normalized.contains("openid"));
        assert!(normalized.contains("profile"));
        assert!(normalized.contains("email"));
        assert!(normalized.contains("offline_access"));
        assert!(normalized.contains("model.request"));
        assert!(normalized.contains("api.responses.write"));
    }

    #[test]
    fn build_openai_codex_authorize_url_uses_expected_pkce_parameters() {
        let url = build_openai_codex_authorize_url(
            "http://127.0.0.1:1455/auth/callback",
            "challenge",
            "state123",
            "liz",
        )
        .expect("authorize url should build");
        assert!(url.contains("client_id=app_EMoamEEZ73f0CkXaXp7hrann"));
        assert!(url.contains("redirect_uri=http%3A%2F%2F127.0.0.1%3A1455%2Fauth%2Fcallback"));
        assert!(url.contains("code_challenge=challenge"));
        assert!(url.contains("state=state123"));
        assert!(url.contains("originator=liz"));
    }

    #[test]
    fn resolve_openai_codex_runtime_auth_keeps_unexpired_access_tokens() {
        let token = "header.eyJodHRwczovL2FwaS5vcGVuYWkuY29tL2F1dGgiOnsiY2hhdGdwdF9hY2NvdW50X2lkIjoiYWNjdC0xIn0sImh0dHBzOi8vYXBpLm9wZW5haS5jb20vcHJvZmlsZSI6eyJlbWFpbCI6InVzZXJAZXhhbXBsZS5jb20ifX0.sig";
        let auth = resolve_openai_codex_runtime_auth(OpenAiCodexRuntimeAuthRequest {
            access_token: Some(token),
            refresh_token: Some("refresh-token"),
            expires_at_ms: Some(u64::MAX),
            account_id: None,
            token_url_override: None,
        })
        .expect("runtime auth should resolve");

        assert_eq!(auth.access_token, token);
        assert_eq!(auth.refresh_token, "refresh-token");
        assert_eq!(auth.account_id.as_deref(), Some("acct-1"));
        assert_eq!(auth.email.as_deref(), Some("user@example.com"));
    }

    #[test]
    fn detect_zai_endpoint_falls_back_to_coding_plan_model() {
        let server = TcpListener::bind("127.0.0.1:0").expect("bind");
        let address = server.local_addr().expect("addr");
        let base_url = format!("http://{}", address);

        std::env::set_var("LIZ_ZAI_DETECT_CODING_GLOBAL_BASE_URL", &base_url);

        thread::spawn(move || {
            for _ in 0..2 {
                let (mut stream, _) = server.accept().expect("accept");
                let mut buffer = vec![0_u8; 8192];
                let bytes = stream.read(&mut buffer).expect("read");
                let request = String::from_utf8_lossy(&buffer[..bytes]);
                let body = request.split("\r\n\r\n").nth(1).unwrap_or_default();
                let status_line = if body.contains(r#""model":"glm-5.1""#) {
                    "HTTP/1.1 404 Not Found"
                } else {
                    "HTTP/1.1 200 OK"
                };
                let response = format!(
                    "{status_line}\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{{}}"
                );
                stream.write_all(response.as_bytes()).expect("write");
            }
        });

        let detected = detect_zai_endpoint("sk-test", Some("coding-global"))
            .expect("detect should succeed")
            .expect("endpoint should be detected");

        assert_eq!(detected.endpoint, "coding-global");
        assert_eq!(detected.model_id, "glm-4.7");

        std::env::remove_var("LIZ_ZAI_DETECT_CODING_GLOBAL_BASE_URL");
    }
}
