//! Adapter for Google-family providers.

use crate::model::auth::google_vertex_bearer_token;
use crate::model::config::ResolvedProvider;
use crate::model::family::ModelProviderFamily;
use crate::model::gateway::{ModelError, ModelRunSummary, ModelTurnRequest};
use crate::model::http::{build_client, post_json};
use crate::model::invocation::{InvocationTransport, ProviderInvocationPlan};
use crate::model::normalized_stream::{NormalizedTurnEvent, UsageDelta};
use crate::model::{
    OutputBudget, PromptCachePolicy, ProviderToolCall, ProviderToolProtocol, ToolSurfaceSpec,
};
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
            let mut tool_calls = Vec::new();

            sink(NormalizedTurnEvent::AssistantDelta {
                chunk: format!("Using {}. ", plan.display_name),
            });
            sink(NormalizedTurnEvent::AssistantDelta {
                chunk: format!("Routing model {} in {}.", plan.model_id, location_hint),
            });
            if request.tool_result_injections.is_empty() && needs_tool_call(&request.user_prompt) {
                let tool_name = infer_tool_name(&request.user_prompt);
                let provider_tool_name = tool_surface
                    .name_map
                    .provider_name(&tool_name)
                    .unwrap_or(tool_name.as_str())
                    .to_owned();
                let arguments = json!({ "path": request.user_prompt });
                sink(NormalizedTurnEvent::ToolCallStarted {
                    call_id: "call_01".to_owned(),
                    tool_name: tool_name.clone(),
                    summary: format!("{} is preparing a tool call", plan.display_name),
                });
                sink(NormalizedTurnEvent::ToolCallCommitted {
                    call_id: "call_01".to_owned(),
                    tool_name: tool_name.clone(),
                    arguments: arguments.to_string(),
                });
                tool_calls.push(ProviderToolCall {
                    call_id: "call_01".to_owned(),
                    tool_name,
                    provider_tool_name,
                    arguments,
                });
            }
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
            let final_message = if tool_calls.is_empty() {
                let final_message = format!(
                    "{} request prepared for {} using {}.",
                    plan.display_name,
                    plan.model_id,
                    plan.family.transport_label()
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
                "contents": google_contents_with_tool_results(&request, &tool_surface),
                "generationConfig": {"maxOutputTokens": output_budget.max_output_tokens},
            });
            if matches!(tool_surface.protocol, ProviderToolProtocol::Native) {
                body["tools"] = google_native_tools_payload(&tool_surface);
            }
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
                "contents": google_contents_with_tool_results(&request, &tool_surface),
                "generationConfig": {"maxOutputTokens": output_budget.max_output_tokens},
            });
            if matches!(tool_surface.protocol, ProviderToolProtocol::Native) {
                body["tools"] = google_native_tools_payload(&tool_surface);
            }
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
                    "messages": [{
                        "role": "user",
                        "content": anthropic_like_user_prompt_with_results(&request, &tool_surface),
                    }],
                    "max_tokens": output_budget.max_output_tokens,
                    "tools": anthropic_native_tools_payload(&tool_surface),
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
    let mut tool_calls = parse_google_tool_calls(&response, &tool_surface);
    if tool_calls.is_empty() && matches!(plan.family, ModelProviderFamily::GoogleVertexAnthropic) {
        tool_calls = parse_google_anthropic_tool_calls(&response, &tool_surface);
    }
    for call in &tool_calls {
        sink(NormalizedTurnEvent::ToolCallStarted {
            call_id: call.call_id.clone(),
            tool_name: call.tool_name.clone(),
            summary: format!("{} requested {}", plan.display_name, call.tool_name),
        });
        sink(NormalizedTurnEvent::ToolCallCommitted {
            call_id: call.call_id.clone(),
            tool_name: call.tool_name.clone(),
            arguments: call.arguments.to_string(),
        });
    }
    let assistant_message = extract_google_text(plan, &response);

    if tool_calls.is_empty() {
        sink(NormalizedTurnEvent::AssistantDelta {
            chunk: format!("Live response from {}.", plan.display_name),
        });
        sink(NormalizedTurnEvent::AssistantMessage { message: assistant_message.clone() });
    }

    Ok(ModelRunSummary {
        assistant_message: tool_calls.is_empty().then_some(assistant_message),
        usage: extract_google_usage(plan, &request, &response),
        tool_calls,
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

fn google_native_tools_payload(tool_surface: &ToolSurfaceSpec) -> serde_json::Value {
    serde_json::Value::Array(vec![json!({
        "functionDeclarations": tool_surface
            .tools
            .iter()
            .map(|tool| {
                json!({
                    "name": tool.provider_name,
                    "description": tool.description,
                    "parameters": tool.input_json_schema,
                })
            })
            .collect::<Vec<_>>()
    })])
}

fn anthropic_native_tools_payload(tool_surface: &ToolSurfaceSpec) -> serde_json::Value {
    serde_json::Value::Array(
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
    )
}

fn google_contents_with_tool_results(
    request: &ModelTurnRequest,
    tool_surface: &ToolSurfaceSpec,
) -> serde_json::Value {
    let mut contents = vec![json!({
        "role":"user",
        "parts":[{"text": request.user_prompt}]
    })];
    for injection in &request.tool_result_injections {
        let provider_tool_name = tool_surface
            .name_map
            .provider_name(&injection.tool_name)
            .unwrap_or(injection.provider_tool_name.as_str());
        contents.push(json!({
            "role":"user",
            "parts":[{
                "functionResponse":{
                    "name": provider_tool_name,
                    "response": injection.result,
                }
            }]
        }));
    }
    serde_json::Value::Array(contents)
}

fn anthropic_like_user_prompt_with_results(
    request: &ModelTurnRequest,
    tool_surface: &ToolSurfaceSpec,
) -> String {
    if request.tool_result_injections.is_empty() {
        return request.user_prompt.clone();
    }
    let mut prompt = request.user_prompt.clone();
    for injection in &request.tool_result_injections {
        let provider_tool_name = tool_surface
            .name_map
            .provider_name(&injection.tool_name)
            .unwrap_or(injection.provider_tool_name.as_str());
        prompt.push_str("\n<liz_tool_result>");
        prompt.push_str(
            &json!({
                "call_id": injection.call_id,
                "tool_name": provider_tool_name,
                "result": injection.result,
                "is_error": injection.is_error,
            })
            .to_string(),
        );
        prompt.push_str("</liz_tool_result>");
    }
    prompt
}

fn parse_google_tool_calls(
    response: &serde_json::Value,
    tool_surface: &ToolSurfaceSpec,
) -> Vec<ProviderToolCall> {
    response
        .get("candidates")
        .and_then(|value| value.as_array())
        .and_then(|items| items.first())
        .and_then(|candidate| candidate.get("content"))
        .and_then(|content| content.get("parts"))
        .and_then(|parts| parts.as_array())
        .into_iter()
        .flatten()
        .enumerate()
        .filter_map(|(index, part)| {
            let function_call = part.get("functionCall")?;
            let provider_tool_name =
                function_call.get("name").and_then(|value| value.as_str())?.to_owned();
            let canonical_name = tool_surface.name_map.canonical_name(&provider_tool_name)?;
            let arguments = function_call.get("args").cloned().unwrap_or_else(|| json!({}));
            Some(ProviderToolCall {
                call_id: format!("call_{}", index + 1),
                tool_name: canonical_name.to_owned(),
                provider_tool_name,
                arguments,
            })
        })
        .collect()
}

fn parse_google_anthropic_tool_calls(
    response: &serde_json::Value,
    tool_surface: &ToolSurfaceSpec,
) -> Vec<ProviderToolCall> {
    response
        .get("content")
        .and_then(|value| value.as_array())
        .into_iter()
        .flatten()
        .enumerate()
        .filter_map(|(index, item)| {
            if item.get("type").and_then(|value| value.as_str()) != Some("tool_use") {
                return None;
            }
            let provider_tool_name = item.get("name").and_then(|value| value.as_str())?.to_owned();
            let canonical_name = tool_surface.name_map.canonical_name(&provider_tool_name)?;
            Some(ProviderToolCall {
                call_id: item
                    .get("id")
                    .and_then(|value| value.as_str())
                    .map(str::to_owned)
                    .unwrap_or_else(|| format!("call_{}", index + 1)),
                tool_name: canonical_name.to_owned(),
                provider_tool_name,
                arguments: item.get("input").cloned().unwrap_or_else(|| json!({})),
            })
        })
        .collect()
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
