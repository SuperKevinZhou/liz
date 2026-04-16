//! Adapter for Google-family providers.

use crate::model::auth::google_vertex_bearer_token;
use crate::model::config::ResolvedProvider;
use crate::model::family::ModelProviderFamily;
use crate::model::gateway::{ModelError, ModelRunSummary, ModelTurnRequest};
use crate::model::http::{build_client, post_json};
use crate::model::invocation::{InvocationTransport, ProviderInvocationPlan};
use crate::model::normalized_stream::{NormalizedTurnEvent, UsageDelta};
use serde_json::json;

/// Google-family adapter for direct Gemini and Vertex providers.
#[derive(Debug, Clone, Default)]
pub struct GoogleAdapter;

impl GoogleAdapter {
    /// Streams one turn through a Google-family provider.
    pub fn stream_turn(
        &self,
        provider: &ResolvedProvider,
        request: ModelTurnRequest,
        sink: &mut dyn FnMut(NormalizedTurnEvent),
    ) -> Result<ModelRunSummary, ModelError> {
        let transport = match provider.spec.family {
            ModelProviderFamily::GoogleGenerativeAi => InvocationTransport::ProviderOperation {
                operation: "google.generate_content",
                base_url: provider.base_url.clone(),
            },
            ModelProviderFamily::GoogleVertex => InvocationTransport::ProviderOperation {
                operation: "google-vertex.generate_content",
                base_url: provider.base_url.clone(),
            },
            ModelProviderFamily::GoogleVertexAnthropic => InvocationTransport::ProviderOperation {
                operation: "google-vertex.anthropic.raw_predict",
                base_url: provider.base_url.clone(),
            },
            _ => {
                return Err(ModelError::ProviderFailure(format!(
                    "provider {} is not a Google-family provider",
                    provider.spec.id
                )))
            }
        };

        let plan = ProviderInvocationPlan {
            provider_id: provider.spec.id.to_owned(),
            display_name: provider.spec.display_name.to_owned(),
            family: provider.spec.family,
            model_id: provider.model_id.clone(),
            auth_kind: provider.spec.auth_kind,
            transport,
            headers: provider.headers.clone(),
            payload_preview: json!({
                "model": provider.model_id,
                "contents": [{"role": "user", "parts": [{"text": request.prompt}]}],
            })
            .to_string(),
            notes: provider.spec.notes.iter().map(|note| (*note).to_owned()).collect(),
        };

        if should_attempt_live_http(provider) {
            return execute_live_http(provider, &plan, request, sink);
        }

        let location_hint = provider
            .metadata
            .get("google.location")
            .cloned()
            .unwrap_or_else(|| "default-location".to_owned());

        sink(NormalizedTurnEvent::AssistantDelta {
            chunk: format!("Using {}. ", plan.display_name),
        });
        sink(NormalizedTurnEvent::AssistantDelta {
            chunk: format!("Routing model {} in {}.", plan.model_id, location_hint),
        });
        sink(NormalizedTurnEvent::ProviderRawEvent {
            label: format!("request-plan {}", plan.payload_preview),
        });
        let usage = UsageDelta {
            input_tokens: estimate_tokens(&request.prompt),
            output_tokens: estimate_tokens(&request.prompt) + 9,
            reasoning_tokens: 0,
            cache_hit_tokens: 0,
            cache_write_tokens: 0,
        };
        sink(NormalizedTurnEvent::UsageDelta(usage.clone()));
        let final_message = format!(
            "{} request prepared for {} using {}.",
            plan.display_name,
            plan.model_id,
            plan.family.transport_label()
        );
        sink(NormalizedTurnEvent::AssistantMessage { message: final_message.clone() });

        Ok(ModelRunSummary { assistant_message: Some(final_message), usage })
    }
}

fn execute_live_http(
    provider: &ResolvedProvider,
    plan: &ProviderInvocationPlan,
    request: ModelTurnRequest,
    sink: &mut dyn FnMut(NormalizedTurnEvent),
) -> Result<ModelRunSummary, ModelError> {
    let (url, body, headers) = match plan.family {
        ModelProviderFamily::GoogleGenerativeAi => {
            let api_key = provider.api_key.clone().ok_or_else(|| {
                ModelError::ProviderFailure(
                    "google provider requires API key for live mode".to_owned(),
                )
            })?;
            let base = provider
                .base_url
                .clone()
                .unwrap_or_else(|| "https://generativelanguage.googleapis.com".to_owned());
            (
                format!(
                    "{}/v1beta/models/{}:generateContent?key={}",
                    trim_trailing_slash(&base),
                    provider.model_id,
                    api_key
                ),
                json!({
                    "contents": [{"role": "user", "parts": [{"text": request.prompt}]}],
                }),
                provider.headers.clone(),
            )
        }
        ModelProviderFamily::GoogleVertex => {
            let project = google_vertex_project(provider)?;
            let location = google_vertex_location(provider);
            let host = google_vertex_host(provider, &location);
            let bearer = google_vertex_runtime_bearer(provider)?;
            let mut headers = provider.headers.clone();
            headers.entry("Authorization".to_owned()).or_insert_with(|| format!("Bearer {bearer}"));
            (
                format!(
                    "{host}/v1/projects/{project}/locations/{location}/publishers/google/models/{}:generateContent",
                    provider.model_id
                ),
                json!({
                    "contents": [{"role": "user", "parts": [{"text": request.prompt}]}],
                }),
                headers,
            )
        }
        ModelProviderFamily::GoogleVertexAnthropic => {
            let project = google_vertex_project(provider)?;
            let location = google_vertex_location(provider);
            let host = google_vertex_host(provider, &location);
            let bearer = google_vertex_runtime_bearer(provider)?;
            let mut headers = provider.headers.clone();
            headers.entry("Authorization".to_owned()).or_insert_with(|| format!("Bearer {bearer}"));
            (
                format!(
                    "{host}/v1/projects/{project}/locations/{location}/publishers/anthropic/models/{}:rawPredict",
                    provider.model_id
                ),
                json!({
                    "anthropic_version": "vertex-2023-10-16",
                    "messages": [{"role": "user", "content": request.prompt}],
                    "max_tokens": 4096,
                }),
                headers,
            )
        }
        _ => return simulate_only(plan, request, sink),
    };

    let response = post_json(&build_client()?, &url, &headers, &body)?;
    let assistant_message = extract_google_text(plan, &response);

    sink(NormalizedTurnEvent::AssistantDelta {
        chunk: format!("Live response from {}.", plan.display_name),
    });
    sink(NormalizedTurnEvent::AssistantMessage { message: assistant_message.clone() });

    Ok(ModelRunSummary {
        assistant_message: Some(assistant_message),
        usage: UsageDelta {
            input_tokens: estimate_tokens(&request.prompt),
            output_tokens: estimate_tokens(&plan.model_id),
            reasoning_tokens: 0,
            cache_hit_tokens: 0,
            cache_write_tokens: 0,
        },
    })
}

fn simulate_only(
    plan: &ProviderInvocationPlan,
    request: ModelTurnRequest,
    sink: &mut dyn FnMut(NormalizedTurnEvent),
) -> Result<ModelRunSummary, ModelError> {
    let location_hint = "default-location";
    sink(NormalizedTurnEvent::AssistantDelta { chunk: format!("Using {}. ", plan.display_name) });
    sink(NormalizedTurnEvent::AssistantDelta {
        chunk: format!("Routing model {} in {}.", plan.model_id, location_hint),
    });
    sink(NormalizedTurnEvent::ProviderRawEvent {
        label: format!("request-plan {}", plan.payload_preview),
    });
    let usage = UsageDelta {
        input_tokens: estimate_tokens(&request.prompt),
        output_tokens: estimate_tokens(&request.prompt) + 9,
        reasoning_tokens: 0,
        cache_hit_tokens: 0,
        cache_write_tokens: 0,
    };
    sink(NormalizedTurnEvent::UsageDelta(usage.clone()));
    let final_message = format!(
        "{} request prepared for {} using {}.",
        plan.display_name,
        plan.model_id,
        plan.family.transport_label()
    );
    sink(NormalizedTurnEvent::AssistantMessage { message: final_message.clone() });

    Ok(ModelRunSummary { assistant_message: Some(final_message), usage })
}

fn should_attempt_live_http(provider: &ResolvedProvider) -> bool {
    if std::env::var("LIZ_PROVIDER_ENABLE_LIVE").ok().as_deref() != Some("1") {
        return false;
    }

    match provider.spec.family {
        ModelProviderFamily::GoogleGenerativeAi => provider.api_key.is_some(),
        ModelProviderFamily::GoogleVertex | ModelProviderFamily::GoogleVertexAnthropic => {
            provider.api_key.is_some() || provider.metadata.contains_key("google.project")
        }
        _ => false,
    }
}

fn trim_trailing_slash(value: &str) -> &str {
    value.trim_end_matches('/')
}

fn extract_google_text(plan: &ProviderInvocationPlan, response: &serde_json::Value) -> String {
    match plan.family {
        ModelProviderFamily::GoogleVertexAnthropic => response
            .get("content")
            .and_then(|value| value.as_array())
            .and_then(|items| items.first())
            .and_then(|item| item.get("text"))
            .and_then(|value| value.as_str())
            .map(str::to_owned)
            .unwrap_or_else(|| format!("{} response received.", plan.display_name)),
        _ => response
            .get("candidates")
            .and_then(|value| value.as_array())
            .and_then(|items| items.first())
            .and_then(|candidate| candidate.get("content"))
            .and_then(|content| content.get("parts"))
            .and_then(|parts| parts.as_array())
            .and_then(|parts| parts.first())
            .and_then(|part| part.get("text"))
            .and_then(|value| value.as_str())
            .map(str::to_owned)
            .unwrap_or_else(|| format!("{} response received.", plan.display_name)),
    }
}

fn google_vertex_project(provider: &ResolvedProvider) -> Result<&str, ModelError> {
    provider.metadata.get("google.project").map(String::as_str).ok_or_else(|| {
        ModelError::ProviderFailure(format!(
            "provider {} requires GOOGLE_CLOUD_PROJECT or equivalent project metadata",
            provider.spec.id
        ))
    })
}

fn google_vertex_location(provider: &ResolvedProvider) -> String {
    provider.metadata.get("google.location").cloned().unwrap_or_else(|| "global".to_owned())
}

fn google_vertex_host(provider: &ResolvedProvider, location: &str) -> String {
    provider.base_url.clone().unwrap_or_else(|| {
        if location == "global" {
            "https://aiplatform.googleapis.com".to_owned()
        } else {
            format!("https://{location}-aiplatform.googleapis.com")
        }
    })
}

fn google_vertex_runtime_bearer(provider: &ResolvedProvider) -> Result<String, ModelError> {
    if let Some(token) = provider.api_key.as_ref() {
        return Ok(token.clone());
    }

    google_vertex_bearer_token()
}

fn estimate_tokens(text: &str) -> u32 {
    let words = text.split_whitespace().count().max(1);
    u32::try_from(words.saturating_mul(3)).unwrap_or(u32::MAX)
}
