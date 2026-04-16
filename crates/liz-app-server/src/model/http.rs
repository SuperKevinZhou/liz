//! Minimal blocking HTTP execution helpers for provider-family requests.

use crate::model::gateway::ModelError;
use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde_json::Value;
use std::collections::BTreeMap;
use std::time::Duration;

/// Builds a blocking HTTP client for provider requests.
pub fn build_client() -> Result<Client, ModelError> {
    Client::builder().timeout(Duration::from_secs(60)).build().map_err(|error| {
        ModelError::ProviderFailure(format!("failed to build HTTP client: {error}"))
    })
}

/// Converts a string map into a reqwest header map.
pub fn build_headers(headers: &BTreeMap<String, String>) -> Result<HeaderMap, ModelError> {
    let mut result = HeaderMap::new();
    for (key, value) in headers {
        let name = HeaderName::from_bytes(key.as_bytes()).map_err(|error| {
            ModelError::ProviderFailure(format!("invalid header name {key}: {error}"))
        })?;
        let header_value = HeaderValue::from_str(value).map_err(|error| {
            ModelError::ProviderFailure(format!("invalid header value for {key}: {error}"))
        })?;
        result.insert(name, header_value);
    }
    Ok(result)
}

/// Sends a JSON POST request and parses the JSON response.
pub fn post_json(
    client: &Client,
    url: &str,
    headers: &BTreeMap<String, String>,
    body: &Value,
) -> Result<Value, ModelError> {
    let response =
        client.post(url).headers(build_headers(headers)?).json(body).send().map_err(|error| {
            ModelError::ProviderFailure(format!("provider request failed: {error}"))
        })?;

    let status = response.status();
    let text = response.text().map_err(|error| {
        ModelError::ProviderFailure(format!("failed to read response body: {error}"))
    })?;
    if !status.is_success() {
        return Err(ModelError::ProviderFailure(format!(
            "provider request returned {status}: {text}"
        )));
    }

    serde_json::from_str(&text).map_err(|error| {
        ModelError::ProviderFailure(format!("failed to parse JSON response: {error}"))
    })
}
