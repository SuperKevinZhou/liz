//! Adapter for Anthropic Messages-compatible providers.

use crate::model::auth::resolve_minimax_oauth_runtime_auth;
use crate::model::config::ResolvedProvider;
use crate::model::gateway::{ModelError, ModelRunSummary, ModelTurnRequest};
use crate::model::http::{build_client, post_json};
use crate::model::invocation::{InvocationTransport, ProviderInvocationPlan};
use crate::model::normalized_stream::{NormalizedTurnEvent, UsageDelta};
use crate::model::{
    anthropic_system_blocks, anthropic_user_content, OutputBudget, PromptCachePolicy,
    ProviderToolCall, ProviderToolProtocol, ToolSurfaceSpec,
};
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
        tool_surface: ToolSurfaceSpec,
        simulate: bool,
        sink: &mut dyn FnMut(NormalizedTurnEvent),
    ) -> Result<ModelRunSummary, ModelError> {
        let instruction_prompt = request.instruction_prompt();
        let output_budget = OutputBudget::for_provider(provider);
        let prompt_cache = PromptCachePolicy::for_provider(provider);
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
                "system": anthropic_system_blocks(&instruction_prompt, prompt_cache.anthropic_ephemeral),
                "max_tokens": output_budget.max_output_tokens,
                "messages": [{"role": "user", "content": anthropic_user_content(&request.user_prompt, prompt_cache.anthropic_ephemeral)}],
                "stream": true,
            })
            .to_string(),
            notes: provider.spec.notes.iter().map(|note| (*note).to_owned()).collect(),
        };

        if simulate {
            let mut tool_calls = Vec::new();
            sink(NormalizedTurnEvent::AssistantDelta {
                chunk: format!("Using {} messages API. ", plan.display_name),
            });
            sink(NormalizedTurnEvent::AssistantDelta {
                chunk: format!("Model {} is ready.", plan.model_id),
            });
            if request.tool_result_injections.is_empty() && needs_tool_call(&request.user_prompt) {
                let tool_name = infer_tool_name(&request.user_prompt);
                let provider_tool_name = tool_surface
                    .name_map
                    .provider_name(&tool_name)
                    .unwrap_or(tool_name.as_str())
                    .to_owned();
                let arguments = json!({ "path": request.user_prompt }).to_string();
                sink(NormalizedTurnEvent::ToolCallStarted {
                    call_id: "call_01".to_owned(),
                    tool_name: tool_name.clone(),
                    summary: format!("{} is preparing a tool call", plan.display_name),
                });
                sink(NormalizedTurnEvent::ToolCallCommitted {
                    call_id: "call_01".to_owned(),
                    tool_name: tool_name.clone(),
                    arguments: arguments.clone(),
                });
                tool_calls.push(ProviderToolCall {
                    call_id: "call_01".to_owned(),
                    tool_name,
                    provider_tool_name,
                    arguments: serde_json::from_str(&arguments).unwrap_or_else(|_| json!({})),
                });
            }
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
            let final_message = if tool_calls.is_empty() {
                let final_message = format!(
                    "{} request prepared for {} using anthropic-messages.",
                    plan.display_name, plan.model_id
                );
                sink(NormalizedTurnEvent::AssistantMessage { message: final_message.clone() });
                Some(final_message)
            } else {
                None
            };

            return Ok(ModelRunSummary { assistant_message: final_message, usage, tool_calls });
        }

        execute_live_http(provider, &plan, request, tool_surface, sink)
    }
}

fn execute_live_http(
    provider: &ResolvedProvider,
    plan: &ProviderInvocationPlan,
    request: ModelTurnRequest,
    tool_surface: ToolSurfaceSpec,
    sink: &mut dyn FnMut(NormalizedTurnEvent),
) -> Result<ModelRunSummary, ModelError> {
    let instruction_prompt = request.instruction_prompt();
    let output_budget = OutputBudget::for_provider(provider);
    let prompt_cache = PromptCachePolicy::for_provider(provider);
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

    let mut user_content =
        anthropic_user_content(&request.user_prompt, prompt_cache.anthropic_ephemeral)
            .as_array()
            .cloned()
            .unwrap_or_default();
    for injection in &request.tool_result_injections {
        user_content.push(json!({
            "type":"tool_result",
            "tool_use_id": injection.call_id,
            "content": injection.result.to_string(),
            "is_error": injection.is_error,
        }));
    }

    let mut body = json!({
        "model": provider.model_id,
        "system": anthropic_system_blocks(&instruction_prompt, prompt_cache.anthropic_ephemeral),
        "max_tokens": output_budget.max_output_tokens,
        "messages": [{"role": "user", "content": user_content}],
        "stream": false,
    });
    if matches!(tool_surface.protocol, ProviderToolProtocol::Native)
        && !tool_surface.tools.is_empty()
    {
        body["tools"] = serde_json::Value::Array(
            tool_surface
                .tools
                .iter()
                .map(|tool| {
                    json!({
                        "name": tool.provider_name,
                        "description": tool.description,
                        "input_schema": tool.input_json_schema,
                    })
                })
                .collect(),
        );
    }
    if provider.spec.id == "minimax" || provider.spec.id == "minimax-portal" {
        body["thinking"] = json!({ "type": "disabled" });
    }
    let response = post_json(
        &build_client()?,
        &format!("{}{}", trim_trailing_slash(&resolved_base_url), path),
        &headers,
        &body,
    )?;

    let mut assistant_parts = Vec::new();
    let mut tool_calls = Vec::new();
    for (index, item) in
        response.get("content").and_then(|value| value.as_array()).into_iter().flatten().enumerate()
    {
        match item.get("type").and_then(|value| value.as_str()) {
            Some("text") | None => {
                if let Some(text) = item.get("text").and_then(|value| value.as_str()) {
                    assistant_parts.push(text.to_owned());
                }
            }
            Some("tool_use") => {
                let provider_tool_name =
                    item.get("name").and_then(|value| value.as_str()).unwrap_or_default();
                if let Some(canonical_name) =
                    tool_surface.name_map.canonical_name(provider_tool_name)
                {
                    let call_id = item
                        .get("id")
                        .and_then(|value| value.as_str())
                        .map(str::to_owned)
                        .unwrap_or_else(|| format!("call_{}", index + 1));
                    let arguments = item.get("input").cloned().unwrap_or_else(|| json!({}));
                    sink(NormalizedTurnEvent::ToolCallStarted {
                        call_id: call_id.clone(),
                        tool_name: canonical_name.to_owned(),
                        summary: format!("{} requested {}", plan.display_name, canonical_name),
                    });
                    sink(NormalizedTurnEvent::ToolCallCommitted {
                        call_id: call_id.clone(),
                        tool_name: canonical_name.to_owned(),
                        arguments: arguments.to_string(),
                    });
                    tool_calls.push(ProviderToolCall {
                        call_id,
                        tool_name: canonical_name.to_owned(),
                        provider_tool_name: provider_tool_name.to_owned(),
                        arguments,
                    });
                }
            }
            _ => {}
        }
    }
    let assistant_message = if assistant_parts.is_empty() {
        format!("{} response received.", plan.display_name)
    } else {
        assistant_parts.join("\n")
    };

    if tool_calls.is_empty() {
        sink(NormalizedTurnEvent::AssistantDelta {
            chunk: format!("Live response from {}.", plan.display_name),
        });
        sink(NormalizedTurnEvent::AssistantMessage { message: assistant_message.clone() });
    }

    Ok(ModelRunSummary {
        assistant_message: tool_calls.is_empty().then_some(assistant_message),
        usage: UsageDelta {
            input_tokens: estimate_tokens(&request.prompt),
            output_tokens: estimate_tokens(&plan.model_id),
            reasoning_tokens: 0,
            cache_hit_tokens: 0,
            cache_write_tokens: 0,
        },
        tool_calls,
    })
}

fn trim_trailing_slash(value: &str) -> &str {
    value.trim_end_matches('/')
}

fn estimate_tokens(text: &str) -> u32 {
    let words = text.split_whitespace().count().max(1);
    u32::try_from(words.saturating_mul(3)).unwrap_or(u32::MAX)
}

fn needs_tool_call(prompt: &str) -> bool {
    let lower = prompt.to_ascii_lowercase();
    lower.contains("tool")
        || lower.contains("patch")
        || lower.contains("command")
        || lower.contains("run ")
}

fn infer_tool_name(prompt: &str) -> String {
    let lower = prompt.to_ascii_lowercase();
    if lower.contains("patch") || lower.contains("write") {
        "workspace.apply_patch".to_owned()
    } else if lower.contains("command") || lower.contains("run ") {
        "shell.exec".to_owned()
    } else {
        "workspace.read".to_owned()
    }
}
