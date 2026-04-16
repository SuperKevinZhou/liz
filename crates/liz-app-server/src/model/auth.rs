//! Authentication helpers for provider families that need runtime credentials.

use crate::model::gateway::ModelError;
use aws_credential_types::provider::ProvideCredentials;
use aws_sigv4::http_request::{sign, SignableBody, SignableRequest, SigningSettings};
use aws_types::region::Region;
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
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
const GITHUB_COPILOT_DEVICE_CLIENT_ID: &str = "Ov23li8tweQw6odWQebz";
const OPENAI_CODEX_OAUTH_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
const OPENAI_CODEX_OAUTH_ISSUER: &str = "https://auth.openai.com";
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
        build_openai_codex_authorize_url, normalize_openai_codex_authorize_url,
        resolve_openai_codex_runtime_auth, OpenAiCodexRuntimeAuthRequest,
    };

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
}
