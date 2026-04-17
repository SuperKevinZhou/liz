//! Adapter for Anthropic Messages-compatible providers.

use crate::model::auth::resolve_minimax_oauth_runtime_auth;
use crate::model::config::ResolvedProvider;
use crate::model::gateway::{ModelError, ModelRunSummary, ModelTurnRequest};
use crate::model::http::{build_client, post_json};
use crate::model::invocation::{InvocationTransport, ProviderInvocationPlan};
use crate::model::normalized_stream::{NormalizedTurnEvent, UsageDelta};
use serde_json::json;

/// Anthropic family adapter.
#[derive(Debug, Clone, Default)]
pub struct AnthropicAdapter;

impl AnthropicAdapter {
    /// Streams one turn through an Anthropic-compatible provider.
    pub fn stream_turn(
        &self,
        provider: &ResolvedProvider,
        request: ModelTurnRequest,
        simulate: bool,
        sink: &mut dyn FnMut(NormalizedTurnEvent),
    ) -> Result<ModelRunSummary, ModelError> {
        let instruction_prompt = request.instruction_prompt();
        let base_url =
            provider.base_url.clone().unwrap_or_else(|| "https://api.anthropic.com".to_owned());
        let plan = ProviderInvocationPlan {
            provider_id: provider.spec.id.to_owned(),
            display_name: provider.spec.display_name.to_owned(),
            family: provider.spec.family,
            model_id: provider.model_id.clone(),
            auth_kind: provider.spec.auth_kind,
            transport: InvocationTransport::HttpJson {
                method: "POST",
                base_url,
                path: "/v1/messages".to_owned(),
            },
            headers: provider.headers.clone(),
            payload_preview: json!({
                "model": provider.model_id,
                "system": instruction_prompt,
                "max_tokens": 4096,
                "messages": [{"role": "user", "content": request.user_prompt}],
                "stream": true,
            })
            .to_string(),
            notes: provider.spec.notes.iter().map(|note| (*note).to_owned()).collect(),
        };

        if simulate {
            sink(NormalizedTurnEvent::AssistantDelta {
                chunk: format!("Using {} messages API. ", plan.display_name),
            });
            sink(NormalizedTurnEvent::AssistantDelta {
                chunk: format!("Model {} is ready.", plan.model_id),
            });
            sink(NormalizedTurnEvent::ProviderRawEvent {
                label: format!("request-plan {}", plan.payload_preview),
            });
            let usage = UsageDelta {
                input_tokens: estimate_tokens(&request.prompt),
                output_tokens: estimate_tokens(&request.prompt) + 10,
                reasoning_tokens: 4,
                cache_hit_tokens: 0,
                cache_write_tokens: 0,
            };
            sink(NormalizedTurnEvent::UsageDelta(usage.clone()));
            let final_message = format!(
                "{} request prepared for {} using anthropic-messages.",
                plan.display_name, plan.model_id
            );
            sink(NormalizedTurnEvent::AssistantMessage { message: final_message.clone() });

            return Ok(ModelRunSummary { assistant_message: Some(final_message), usage });
        }

        execute_live_http(provider, &plan, request, sink)
    }
}

fn execute_live_http(
    provider: &ResolvedProvider,
    plan: &ProviderInvocationPlan,
    request: ModelTurnRequest,
    sink: &mut dyn FnMut(NormalizedTurnEvent),
) -> Result<ModelRunSummary, ModelError> {
    let instruction_prompt = request.instruction_prompt();
    let InvocationTransport::HttpJson { base_url, path, .. } = &plan.transport else {
        return Err(ModelError::ProviderFailure(
            "anthropic transport must be HTTP JSON".to_owned(),
        ));
    };

    let mut headers = provider.headers.clone();
    let mut resolved_base_url = base_url.to_owned();
    let mut api_key = provider.api_key.clone();
    if provider.spec.id == "minimax-portal" {
        let expires_at_ms = provider
            .metadata
            .get("minimax.oauth.expires_at_ms")
            .and_then(|value| value.parse::<u64>().ok());
        let runtime = resolve_minimax_oauth_runtime_auth(
            provider.api_key.as_deref(),
            provider.metadata.get("minimax.oauth.refresh_token").map(String::as_str),
            expires_at_ms,
            provider.metadata.get("minimax.region").map(String::as_str).unwrap_or("global"),
            provider.metadata.get("minimax.resource_url").map(String::as_str),
        )?;
        api_key = Some(runtime.access_token);
        resolved_base_url = runtime.resource_url;
        headers
            .entry("Authorization".to_owned())
            .or_insert_with(|| format!("Bearer {}", api_key.clone().unwrap_or_default()));
    } else if provider.spec.id == "minimax" {
        if let Some(api_key) = api_key.as_ref() {
            headers
                .entry("Authorization".to_owned())
                .or_insert_with(|| format!("Bearer {api_key}"));
        }
    } else if let Some(api_key) = api_key.as_ref() {
        headers.entry("x-api-key".to_owned()).or_insert_with(|| api_key.clone());
    }
    headers.entry("anthropic-version".to_owned()).or_insert_with(|| "2023-06-01".to_owned());

    let mut body = json!({
        "model": provider.model_id,
        "system": instruction_prompt,
        "max_tokens": 4096,
        "messages": [{"role": "user", "content": request.user_prompt}],
        "stream": false,
    });
    if provider.spec.id == "minimax" || provider.spec.id == "minimax-portal" {
        body["thinking"] = json!({ "type": "disabled" });
    }
    let response = post_json(
        &build_client()?,
        &format!("{}{}", trim_trailing_slash(&resolved_base_url), path),
        &headers,
        &body,
    )?;

    let assistant_message = response
        .get("content")
        .and_then(|value| value.as_array())
        .and_then(|items| items.first())
        .and_then(|item| item.get("text"))
        .and_then(|value| value.as_str())
        .map(str::to_owned)
        .unwrap_or_else(|| format!("{} response received.", plan.display_name));

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

fn trim_trailing_slash(value: &str) -> &str {
    value.trim_end_matches('/')
}

fn estimate_tokens(text: &str) -> u32 {
    let words = text.split_whitespace().count().max(1);
    u32::try_from(words.saturating_mul(3)).unwrap_or(u32::MAX)
}
