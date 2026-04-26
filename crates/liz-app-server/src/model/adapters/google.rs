//! Adapter for Google-family providers.

use crate::model::auth::google_vertex_bearer_token;
use crate::model::config::ResolvedProvider;
use crate::model::family::ModelProviderFamily;
use crate::model::gateway::{ModelError, ModelRunSummary, ModelTurnRequest};
use crate::model::http::{build_client, post_json};
use crate::model::invocation::{InvocationTransport, ProviderInvocationPlan};
use crate::model::normalized_stream::{NormalizedTurnEvent, UsageDelta};
use crate::model::{OutputBudget, PromptCachePolicy, ToolSurfaceSpec};
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
        tool_surface: ToolSurfaceSpec,
        simulate: bool,
        sink: &mut dyn FnMut(NormalizedTurnEvent),
    ) -> Result<ModelRunSummary, ModelError> {
        let instruction_prompt = request.instruction_prompt();
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
                "system_instruction": {"parts": [{"text": instruction_prompt}]},
                "contents": [{"role": "user", "parts": [{"text": request.user_prompt}]}],
            })
            .to_string(),
            notes: provider.spec.notes.iter().map(|note| (*note).to_owned()).collect(),
        };

        if simulate {
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

            return Ok(ModelRunSummary {
                assistant_message: Some(final_message),
                usage,
                tool_calls: Vec::new(),
            });
        }

        execute_live_http(provider, &plan, request, tool_surface, sink)
    }
}

fn execute_live_http(
    provider: &ResolvedProvider,
    plan: &ProviderInvocationPlan,
    request: ModelTurnRequest,
    _tool_surface: ToolSurfaceSpec,
    sink: &mut dyn FnMut(NormalizedTurnEvent),
) -> Result<ModelRunSummary, ModelError> {
    let instruction_prompt = request.instruction_prompt();
    let output_budget = OutputBudget::for_provider(provider);
    let prompt_cache = PromptCachePolicy::for_provider(provider);
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
            let mut body = json!({
                "system_instruction": {"parts": [{"text": instruction_prompt}]},
                "contents": [{"role": "user", "parts": [{"text": request.user_prompt}]}],
                "generationConfig": {"maxOutputTokens": output_budget.max_output_tokens},
            });
            if let Some(cached_content) = prompt_cache.cached_content.clone() {
                body["cachedContent"] = json!(cached_content);
            }
            (
                format!(
                    "{}/v1beta/models/{}:generateContent?key={}",
                    trim_trailing_slash(&base),
                    provider.model_id,
                    api_key
                ),
                body,
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
            let mut body = json!({
                "system_instruction": {"parts": [{"text": instruction_prompt}]},
                "contents": [{"role": "user", "parts": [{"text": request.user_prompt}]}],
                "generationConfig": {"maxOutputTokens": output_budget.max_output_tokens},
            });
            if let Some(cached_content) = prompt_cache.cached_content.clone() {
                body["cachedContent"] = json!(cached_content);
            }
            (
                format!(
                    "{host}/v1/projects/{project}/locations/{location}/publishers/google/models/{}:generateContent",
                    provider.model_id
                ),
                body,
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
                    "system": instruction_prompt,
                    "messages": [{"role": "user", "content": request.user_prompt}],
                    "max_tokens": output_budget.max_output_tokens,
                }),
                headers,
            )
        }
        _ => {
            return Err(ModelError::ProviderFailure(format!(
                "live Google transport {} is not implemented for {}",
                plan.family.transport_label(),
                plan.provider_id
            )))
        }
    };

    let response = post_json(&build_client()?, &url, &headers, &body)?;
    let assistant_message = extract_google_text(plan, &response);

    sink(NormalizedTurnEvent::AssistantDelta {
        chunk: format!("Live response from {}.", plan.display_name),
    });
    sink(NormalizedTurnEvent::AssistantMessage { message: assistant_message.clone() });

    Ok(ModelRunSummary {
        assistant_message: Some(assistant_message),
        usage: extract_google_usage(plan, &request, &response),
        tool_calls: Vec::new(),
    })
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

fn extract_google_usage(
    plan: &ProviderInvocationPlan,
    request: &ModelTurnRequest,
    response: &serde_json::Value,
) -> UsageDelta {
    match plan.family {
        ModelProviderFamily::GoogleVertexAnthropic => UsageDelta {
            input_tokens: estimate_tokens(&request.prompt),
            output_tokens: estimate_tokens(&plan.model_id),
            reasoning_tokens: 0,
            cache_hit_tokens: 0,
            cache_write_tokens: 0,
        },
        _ => {
            let prompt_tokens = response
                .get("usageMetadata")
                .and_then(|value| value.get("promptTokenCount"))
                .and_then(|value| value.as_u64())
                .and_then(|value| u32::try_from(value).ok())
                .unwrap_or_else(|| estimate_tokens(&request.prompt));
            let output_tokens = response
                .get("usageMetadata")
                .and_then(|value| value.get("candidatesTokenCount"))
                .and_then(|value| value.as_u64())
                .and_then(|value| u32::try_from(value).ok())
                .unwrap_or_else(|| estimate_tokens(&plan.model_id));
            let cache_hit_tokens = response
                .get("usageMetadata")
                .and_then(|value| value.get("cachedContentTokenCount"))
                .and_then(|value| value.as_u64())
                .and_then(|value| u32::try_from(value).ok())
                .unwrap_or(0);
            UsageDelta {
                input_tokens: prompt_tokens.saturating_sub(cache_hit_tokens),
                output_tokens,
                reasoning_tokens: 0,
                cache_hit_tokens,
                cache_write_tokens: 0,
            }
        }
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
