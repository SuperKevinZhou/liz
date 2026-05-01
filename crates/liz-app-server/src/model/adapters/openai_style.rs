//! Adapter for OpenAI-style, OpenAI-compatible, Codex, Copilot, and GitLab provider families.

use crate::model::auth::{
    resolve_bedrock_mantle_runtime_auth, resolve_copilot_runtime_auth,
    resolve_gitlab_oauth_runtime_auth, resolve_openai_codex_runtime_auth,
    resolve_sap_ai_core_runtime_auth, OpenAiCodexRuntimeAuthRequest,
};
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

/// Provider-family adapter for OpenAI-style runtimes.
#[derive(Debug, Clone, Default)]
pub struct OpenAiStyleAdapter;

impl OpenAiStyleAdapter {
    /// Streams one turn through the resolved OpenAI-style provider.
    pub fn stream_turn(
        &self,
        provider: &ResolvedProvider,
        request: ModelTurnRequest,
        tool_surface: ToolSurfaceSpec,
        simulate: bool,
        sink: &mut dyn FnMut(NormalizedTurnEvent),
    ) -> Result<ModelRunSummary, ModelError> {
        let plan = self.build_plan(provider, &request)?;
        if simulate {
            return simulate_stream(plan, request, tool_surface, sink);
        }

        execute_live_http(provider, &plan, request, tool_surface, sink)
    }

    fn build_plan(
        &self,
        provider: &ResolvedProvider,
        request: &ModelTurnRequest,
    ) -> Result<ProviderInvocationPlan, ModelError> {
        let instruction_prompt = request.instruction_prompt();
        let payload_preview = match provider.spec.family {
            ModelProviderFamily::OpenAiResponses => json!({
                "model": provider.model_id,
                "instructions": instruction_prompt,
                "input": request.user_prompt,
                "stream": true,
            })
            .to_string(),
            ModelProviderFamily::GitHubCopilot => json!({
                "model": provider.model_id,
                "instructions": instruction_prompt,
                "input": request.user_prompt,
                "mode": if copilot_uses_anthropic_messages_model(&provider.model_id) {
                    "messages"
                } else if should_use_copilot_responses_api(&provider.model_id) {
                    "responses"
                } else {
                    "chat"
                },
            })
            .to_string(),
            ModelProviderFamily::GitLabDuo => json!({
                "model": provider.model_id,
                "prompt": request.prompt,
                "workflow_discovery": true,
            })
            .to_string(),
            _ => json!({
                "model": provider.model_id,
                "messages": [
                    {"role": "system", "content": instruction_prompt},
                    {"role": "user", "content": request.user_prompt}
                ],
                "stream": true,
            })
            .to_string(),
        };

        let transport = match provider.spec.family {
            ModelProviderFamily::OpenAiResponses => {
                let (base_url, path) = if provider.spec.id == "openai-codex" {
                    (
                        provider
                            .base_url
                            .clone()
                            .unwrap_or_else(|| "https://chatgpt.com/backend-api".to_owned()),
                        "/codex/responses".to_owned(),
                    )
                } else {
                    (
                        provider
                            .base_url
                            .clone()
                            .unwrap_or_else(|| "https://api.openai.com".to_owned()),
                        openai_responses_path(
                            provider.base_url.as_deref().unwrap_or("https://api.openai.com"),
                        ),
                    )
                };
                InvocationTransport::HttpJson { method: "POST", base_url, path }
            }
            ModelProviderFamily::OpenAiCompatible => match provider.base_url.clone() {
                Some(base_url) => InvocationTransport::HttpJson {
                    method: "POST",
                    path: openai_compatible_chat_path(&base_url),
                    base_url,
                },
                None => InvocationTransport::ProviderOperation {
                    operation: "openai-compatible.chat-completions",
                    base_url: None,
                },
            },
            ModelProviderFamily::GitHubCopilot => {
                if copilot_uses_anthropic_messages_model(&provider.model_id) {
                    InvocationTransport::ProviderOperation {
                        operation: "github-copilot.messages",
                        base_url: provider.base_url.clone(),
                    }
                } else if should_use_copilot_responses_api(&provider.model_id) {
                    InvocationTransport::ProviderOperation {
                        operation: "github-copilot.responses",
                        base_url: provider.base_url.clone(),
                    }
                } else {
                    InvocationTransport::ProviderOperation {
                        operation: "github-copilot.chat",
                        base_url: provider.base_url.clone(),
                    }
                }
            }
            ModelProviderFamily::GitLabDuo => InvocationTransport::ProviderOperation {
                operation: "gitlab.duo.chat",
                base_url: provider.base_url.clone(),
            },
            _ => unreachable!("unexpected family for OpenAiStyleAdapter"),
        };

        Ok(ProviderInvocationPlan {
            provider_id: provider.spec.id.to_owned(),
            display_name: provider.spec.display_name.to_owned(),
            family: provider.spec.family,
            model_id: provider.model_id.clone(),
            auth_kind: provider.spec.auth_kind,
            transport,
            headers: provider.headers.clone(),
            payload_preview,
            notes: provider.spec.notes.iter().map(|note| (*note).to_owned()).collect(),
        })
    }
}

fn execute_live_http(
    provider: &ResolvedProvider,
    plan: &ProviderInvocationPlan,
    request: ModelTurnRequest,
    tool_surface: ToolSurfaceSpec,
    sink: &mut dyn FnMut(NormalizedTurnEvent),
) -> Result<ModelRunSummary, ModelError> {
    let mut instruction_prompt = request.instruction_prompt();
    let output_budget = OutputBudget::for_provider(provider);
    let prompt_cache = PromptCachePolicy::for_provider(provider);
    if matches!(tool_surface.protocol, ProviderToolProtocol::StructuredFallback) {
        instruction_prompt = format!(
            "{instruction_prompt}\n\nstructured_tool_protocol:\n{}",
            tool_surface.structured_fallback_instructions()
        );
    }

    let (url, body, headers) = match &plan.transport {
        InvocationTransport::HttpJson { base_url, path, .. } => {
            let mut body = match plan.family {
                ModelProviderFamily::OpenAiResponses => json!({
                    "model": provider.model_id,
                    "instructions": instruction_prompt,
                    "input": openai_responses_input_payload(&request, &tool_surface),
                    "max_output_tokens": output_budget.max_output_tokens,
                    "stream": false,
                }),
                _ => json!({
                    "model": provider.model_id,
                    "max_tokens": output_budget.max_output_tokens,
                    "messages": openai_chat_messages_payload(
                        &instruction_prompt,
                        &request,
                        &tool_surface,
                    ),
                    "stream": false,
                }),
            };
            if matches!(tool_surface.protocol, ProviderToolProtocol::Native)
                && !tool_surface.tools.is_empty()
            {
                body["tools"] = openai_native_tools_payload(&tool_surface);
            }
            if let Some(cache_retention) = prompt_cache.openai_cache_retention.clone() {
                body["prompt_cache_retention"] = json!(cache_retention);
            }
            if let Some(prompt_cache_key) = prompt_cache.openai_prompt_cache_key.clone() {
                body["prompt_cache_key"] = json!(prompt_cache_key);
            }
            (
                format!("{}{}", trim_trailing_slash(base_url), path),
                body,
                default_openai_style_headers(provider, plan)?,
            )
        }
        InvocationTransport::ProviderOperation { .. } => match plan.family {
            ModelProviderFamily::GitHubCopilot => {
                let github_token = provider.api_key.as_ref().ok_or_else(|| {
                    ModelError::ProviderFailure(
                        "github-copilot requires a GitHub token for live mode".to_owned(),
                    )
                })?;
                let runtime = resolve_copilot_runtime_auth(
                    github_token,
                    provider.metadata.get("copilot.token_url").map(String::as_str),
                    provider
                        .metadata
                        .get("copilot.api_base_url")
                        .map(String::as_str)
                        .or(provider.base_url.as_deref()),
                )?;
                let mut headers = provider.headers.clone();
                headers
                    .entry("Authorization".to_owned())
                    .or_insert_with(|| format!("Bearer {}", runtime.token));
                headers
                    .entry("Editor-Version".to_owned())
                    .or_insert_with(|| "vscode/1.96.2".to_owned());
                headers
                    .entry("User-Agent".to_owned())
                    .or_insert_with(|| "GitHubCopilotChat/0.26.7".to_owned());
                headers
                    .entry("Openai-Intent".to_owned())
                    .or_insert_with(|| "conversation-edits".to_owned());
                headers.entry("X-Initiator".to_owned()).or_insert_with(|| "user".to_owned());

                if copilot_uses_anthropic_messages_model(&provider.model_id) {
                    headers
                        .entry("anthropic-version".to_owned())
                        .or_insert_with(|| "2023-06-01".to_owned());
                    headers
                        .entry("anthropic-beta".to_owned())
                        .or_insert_with(|| "interleaved-thinking-2025-05-14".to_owned());
                    let mut body = json!({
                        "model": provider.model_id,
                        "system": instruction_prompt,
                        "max_tokens": output_budget.max_output_tokens,
                        "messages": [{
                            "role": "user",
                            "content": anthropic_like_user_prompt_with_results(&request, &tool_surface)
                        }],
                        "stream": false,
                    });
                    if matches!(tool_surface.protocol, ProviderToolProtocol::Native)
                        && !tool_surface.tools.is_empty()
                    {
                        body["tools"] = anthropic_native_tools_payload(&tool_surface);
                    }
                    (
                        format!("{}/v1/messages", trim_trailing_slash(&runtime.base_url)),
                        body,
                        headers,
                    )
                } else if should_use_copilot_responses_api(&provider.model_id) {
                    let mut body = json!({
                        "model": provider.model_id,
                        "instructions": instruction_prompt,
                        "input": openai_responses_input_payload(&request, &tool_surface),
                        "max_output_tokens": output_budget.max_output_tokens,
                        "stream": false,
                    });
                    if matches!(tool_surface.protocol, ProviderToolProtocol::Native)
                        && !tool_surface.tools.is_empty()
                    {
                        body["tools"] = openai_native_tools_payload(&tool_surface);
                    }
                    (
                        format!("{}/v1/responses", trim_trailing_slash(&runtime.base_url)),
                        body,
                        headers,
                    )
                } else {
                    let mut body = json!({
                        "model": provider.model_id,
                        "max_tokens": output_budget.max_output_tokens,
                        "messages": openai_chat_messages_payload(
                            &instruction_prompt,
                            &request,
                            &tool_surface,
                        ),
                        "stream": false,
                    });
                    if matches!(tool_surface.protocol, ProviderToolProtocol::Native)
                        && !tool_surface.tools.is_empty()
                    {
                        body["tools"] = openai_native_tools_payload(&tool_surface);
                    }
                    (
                        format!("{}/v1/chat/completions", trim_trailing_slash(&runtime.base_url)),
                        body,
                        headers,
                    )
                }
            }
            ModelProviderFamily::GitLabDuo => {
                let base_url = provider.base_url.as_ref().ok_or_else(|| {
                    ModelError::ProviderFailure(
                        "gitlab provider requires an instance or AI gateway base URL for live mode"
                            .to_owned(),
                    )
                })?;
                let mut api_key = provider.api_key.clone().ok_or_else(|| {
                    ModelError::ProviderFailure(
                        "gitlab provider requires a token for live mode".to_owned(),
                    )
                })?;
                if provider.metadata.get("gitlab.auth_mode").map(String::as_str) == Some("oauth") {
                    let expires_at_ms = provider
                        .metadata
                        .get("gitlab.oauth.expires_at_ms")
                        .and_then(|value| value.parse::<u64>().ok());
                    let runtime = resolve_gitlab_oauth_runtime_auth(
                        provider.api_key.as_deref(),
                        provider.metadata.get("gitlab.oauth.refresh_token").map(String::as_str),
                        expires_at_ms,
                        provider
                            .metadata
                            .get("gitlab.instance_url")
                            .map(String::as_str)
                            .unwrap_or(base_url),
                        provider.metadata.get("gitlab.oauth_client_id").map(String::as_str),
                        provider.metadata.get("gitlab.oauth_client_secret").map(String::as_str),
                        provider.metadata.get("gitlab.oauth_token_url").map(String::as_str),
                    )?;
                    api_key = runtime.access_token;
                }
                let mut headers = provider.headers.clone();
                if gitlab_uses_private_token(&api_key, provider) {
                    headers.entry("PRIVATE-TOKEN".to_owned()).or_insert_with(|| api_key.clone());
                } else {
                    headers
                        .entry("Authorization".to_owned())
                        .or_insert_with(|| format!("Bearer {api_key}"));
                }
                (
                    gitlab_chat_endpoint(base_url),
                    json!({
                        "content": gitlab_structured_prompt(&request, &tool_surface),
                    }),
                    headers,
                )
            }
            _ => {
                let operation = match &plan.transport {
                    InvocationTransport::ProviderOperation { operation, .. } => *operation,
                    InvocationTransport::HttpJson { .. } => "http-json",
                };
                return Err(ModelError::ProviderFailure(format!(
                    "live provider operation is not implemented for {} ({operation})",
                    plan.provider_id
                )));
            }
        },
    };

    let response = post_json(&build_client()?, &url, &headers, &body)?;
    let response_text = extract_openai_style_text(&response).unwrap_or_default();
    let tool_calls = parse_provider_tool_calls(plan, &response, &tool_surface, &response_text);
    let assistant_message = clean_structured_fallback_text(&response_text);

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

    if !assistant_message.trim().is_empty() && tool_calls.is_empty() {
        sink(NormalizedTurnEvent::AssistantDelta {
            chunk: format!("Live response from {}.", plan.display_name),
        });
        sink(NormalizedTurnEvent::AssistantMessage { message: assistant_message.clone() });
    }

    let final_message = if tool_calls.is_empty() {
        Some(if assistant_message.trim().is_empty() {
            format!("{} response received for {}.", plan.display_name, plan.model_id)
        } else {
            assistant_message
        })
    } else {
        None
    };

    Ok(ModelRunSummary {
        assistant_message: final_message,
        usage: extract_openai_style_usage(&request, plan, &response),
        tool_calls,
    })
}

fn simulate_stream(
    plan: ProviderInvocationPlan,
    request: ModelTurnRequest,
    tool_surface: ToolSurfaceSpec,
    sink: &mut dyn FnMut(NormalizedTurnEvent),
) -> Result<ModelRunSummary, ModelError> {
    let first_chunk = format!("Using {} via ", plan.display_name);
    let second_chunk = match &plan.transport {
        InvocationTransport::HttpJson { path, .. } => format!("{path} for {}.", plan.model_id),
        InvocationTransport::ProviderOperation { operation, .. } => {
            format!("{operation} for {}.", plan.model_id)
        }
    };

    sink(NormalizedTurnEvent::AssistantDelta { chunk: first_chunk });
    sink(NormalizedTurnEvent::AssistantDelta { chunk: second_chunk });

    let mut tool_calls = Vec::new();
    if !tool_surface.tools.is_empty()
        && request.tool_result_injections.is_empty()
        && needs_tool_call(&request.user_prompt)
    {
        let tool_name = infer_tool_name(&request.user_prompt);
        let provider_tool_name = tool_surface
            .name_map
            .provider_name(&tool_name)
            .unwrap_or(tool_name.as_str())
            .to_owned();
        let arguments = synthesize_tool_arguments(
            &request.user_prompt,
            request.thread.id.as_str(),
            &plan.provider_id,
        );
        let parsed_arguments =
            serde_json::from_str::<serde_json::Value>(&arguments).unwrap_or_else(|_| json!({}));
        sink(NormalizedTurnEvent::ToolCallStarted {
            call_id: "call_01".to_owned(),
            tool_name: tool_name.clone(),
            summary: format!("{} is preparing a tool call", plan.display_name),
        });
        if provider_supports_patching(&plan.family) {
            sink(NormalizedTurnEvent::ToolCallDelta {
                call_id: "call_01".to_owned(),
                tool_name: tool_name.clone(),
                delta_summary: "arguments patched".to_owned(),
                preview: Some(format!(
                    "{{\"goal\":\"{}\",\"provider\":\"{}\"",
                    truncate_preview(&request.user_prompt),
                    plan.provider_id
                )),
            });
        }
        sink(NormalizedTurnEvent::ToolCallCommitted {
            call_id: "call_01".to_owned(),
            tool_name: tool_name.clone(),
            arguments,
        });
        tool_calls.push(ProviderToolCall {
            call_id: "call_01".to_owned(),
            tool_name,
            provider_tool_name,
            arguments: parsed_arguments,
        });
    }

    sink(NormalizedTurnEvent::ProviderRawEvent {
        label: format!("request-plan {}", plan.payload_preview),
    });

    let usage = UsageDelta {
        input_tokens: estimate_tokens(&request.prompt),
        output_tokens: estimate_tokens(&request.user_prompt) + 12,
        reasoning_tokens: if supports_reasoning_accounting(&plan.family) { 6 } else { 0 },
        cache_hit_tokens: 0,
        cache_write_tokens: 0,
    };
    sink(NormalizedTurnEvent::UsageDelta(usage.clone()));

    let final_message = if tool_calls.is_empty() {
        let message = format!(
            "{} request prepared for {} using {}.",
            plan.display_name,
            plan.model_id,
            plan.family.transport_label()
        );
        sink(NormalizedTurnEvent::AssistantMessage { message: message.clone() });
        Some(message)
    } else {
        None
    };

    Ok(ModelRunSummary { assistant_message: final_message, usage, tool_calls })
}

fn openai_native_tools_payload(tool_surface: &ToolSurfaceSpec) -> serde_json::Value {
    serde_json::Value::Array(
        tool_surface
            .tools
            .iter()
            .map(|tool| {
                json!({
                    "type":"function",
                    "function":{
                        "name":tool.provider_name,
                        "description":tool.description,
                        "parameters":tool.input_json_schema,
                    }
                })
            })
            .collect(),
    )
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

fn openai_responses_input_payload(
    request: &ModelTurnRequest,
    tool_surface: &ToolSurfaceSpec,
) -> serde_json::Value {
    if request.tool_result_injections.is_empty() {
        return json!(request.user_prompt);
    }

    let mut items = vec![json!({
        "type":"message",
        "role":"user",
        "content":[{"type":"input_text","text":request.user_prompt}]
    })];
    for injection in &request.tool_result_injections {
        let provider_tool_name = tool_surface
            .name_map
            .provider_name(&injection.tool_name)
            .unwrap_or(injection.provider_tool_name.as_str())
            .to_owned();
        items.push(json!({
            "type":"function_call_output",
            "call_id": injection.call_id,
            "name": provider_tool_name,
            "output": injection.result.to_string(),
            "is_error": injection.is_error,
        }));
    }
    serde_json::Value::Array(items)
}

fn openai_chat_messages_payload(
    instruction_prompt: &str,
    request: &ModelTurnRequest,
    tool_surface: &ToolSurfaceSpec,
) -> serde_json::Value {
    let mut messages = vec![
        json!({"role":"system","content":instruction_prompt}),
        json!({"role":"user","content":request.user_prompt}),
    ];
    for injection in &request.tool_result_injections {
        let provider_tool_name = tool_surface
            .name_map
            .provider_name(&injection.tool_name)
            .unwrap_or(injection.provider_tool_name.as_str())
            .to_owned();
        messages.push(json!({
            "role":"tool",
            "tool_call_id": injection.call_id,
            "name": provider_tool_name,
            "content": injection.result.to_string(),
        }));
    }
    serde_json::Value::Array(messages)
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
                "call_id":injection.call_id,
                "tool_name":provider_tool_name,
                "result":injection.result,
                "is_error":injection.is_error,
                "summary":injection.summary,
            })
            .to_string(),
        );
        prompt.push_str("</liz_tool_result>");
    }
    prompt
}

fn gitlab_structured_prompt(request: &ModelTurnRequest, tool_surface: &ToolSurfaceSpec) -> String {
    if matches!(tool_surface.protocol, ProviderToolProtocol::Native) {
        return request.prompt.clone();
    }
    let mut prompt = request.prompt.clone();
    prompt.push_str("\n\nstructured_tool_protocol:\n");
    prompt.push_str(&tool_surface.structured_fallback_instructions());
    for injection in &request.tool_result_injections {
        let provider_tool_name = tool_surface
            .name_map
            .provider_name(&injection.tool_name)
            .unwrap_or(injection.provider_tool_name.as_str());
        prompt.push_str("\n<liz_tool_result>");
        prompt.push_str(
            &json!({
                "call_id":injection.call_id,
                "tool_name":provider_tool_name,
                "result":injection.result,
                "is_error":injection.is_error,
                "summary":injection.summary,
            })
            .to_string(),
        );
        prompt.push_str("</liz_tool_result>");
    }
    prompt
}

fn parse_provider_tool_calls(
    plan: &ProviderInvocationPlan,
    response: &serde_json::Value,
    tool_surface: &ToolSurfaceSpec,
    response_text: &str,
) -> Vec<ProviderToolCall> {
    if matches!(tool_surface.protocol, ProviderToolProtocol::StructuredFallback) {
        return parse_structured_fallback_tool_calls(response_text, tool_surface);
    }

    let mut calls = parse_openai_native_tool_calls_from_response(response, tool_surface);
    if calls.is_empty() {
        calls = parse_chat_tool_calls_from_response(response, tool_surface);
    }
    if calls.is_empty() && copilot_uses_anthropic_messages_model(&plan.model_id) {
        calls = parse_anthropic_tool_calls_from_response(response, tool_surface);
    }
    calls
}

fn parse_openai_native_tool_calls_from_response(
    response: &serde_json::Value,
    tool_surface: &ToolSurfaceSpec,
) -> Vec<ProviderToolCall> {
    response
        .get("output")
        .and_then(|value| value.as_array())
        .into_iter()
        .flatten()
        .filter_map(|item| {
            let item_type = item.get("type").and_then(|value| value.as_str()).unwrap_or_default();
            if item_type != "function_call" {
                return None;
            }
            let call_id = item
                .get("call_id")
                .or_else(|| item.get("id"))
                .and_then(|value| value.as_str())
                .unwrap_or("call_auto")
                .to_owned();
            let provider_tool_name = item.get("name").and_then(|value| value.as_str())?.to_owned();
            let canonical_name = normalize_tool_name(tool_surface, &provider_tool_name)?;
            let arguments = parse_tool_call_arguments(item.get("arguments"))?;
            Some(ProviderToolCall {
                call_id,
                tool_name: canonical_name,
                provider_tool_name,
                arguments,
            })
        })
        .collect()
}

fn parse_chat_tool_calls_from_response(
    response: &serde_json::Value,
    tool_surface: &ToolSurfaceSpec,
) -> Vec<ProviderToolCall> {
    response
        .get("choices")
        .and_then(|value| value.as_array())
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("tool_calls"))
        .and_then(|calls| calls.as_array())
        .into_iter()
        .flatten()
        .filter_map(|call| {
            let call_id =
                call.get("id").and_then(|value| value.as_str()).unwrap_or("call_auto").to_owned();
            let provider_tool_name = call
                .get("function")
                .and_then(|function| function.get("name"))
                .and_then(|value| value.as_str())?
                .to_owned();
            let canonical_name = normalize_tool_name(tool_surface, &provider_tool_name)?;
            let arguments = parse_tool_call_arguments(
                call.get("function").and_then(|function| function.get("arguments")),
            )?;
            Some(ProviderToolCall {
                call_id,
                tool_name: canonical_name,
                provider_tool_name,
                arguments,
            })
        })
        .collect()
}

fn parse_anthropic_tool_calls_from_response(
    response: &serde_json::Value,
    tool_surface: &ToolSurfaceSpec,
) -> Vec<ProviderToolCall> {
    response
        .get("content")
        .and_then(|value| value.as_array())
        .into_iter()
        .flatten()
        .filter_map(|item| {
            if item.get("type").and_then(|value| value.as_str()) != Some("tool_use") {
                return None;
            }
            let call_id =
                item.get("id").and_then(|value| value.as_str()).unwrap_or("call_auto").to_owned();
            let provider_tool_name = item.get("name").and_then(|value| value.as_str())?.to_owned();
            let canonical_name = normalize_tool_name(tool_surface, &provider_tool_name)?;
            let arguments = item.get("input").cloned().unwrap_or_else(|| json!({}));
            Some(ProviderToolCall {
                call_id,
                tool_name: canonical_name,
                provider_tool_name,
                arguments,
            })
        })
        .collect()
}

fn parse_structured_fallback_tool_calls(
    response_text: &str,
    tool_surface: &ToolSurfaceSpec,
) -> Vec<ProviderToolCall> {
    let mut calls = Vec::new();
    let mut cursor = response_text;
    while let Some(start) = cursor.find("<liz_tool_call>") {
        let after_start = &cursor[start + "<liz_tool_call>".len()..];
        let Some(end) = after_start.find("</liz_tool_call>") else {
            break;
        };
        let payload = after_start[..end].trim();
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(payload) {
            if let (Some(provider_tool_name), Some(arguments)) =
                (value.get("tool_name").and_then(|value| value.as_str()), value.get("arguments"))
            {
                if let Some(canonical_name) = normalize_tool_name(tool_surface, provider_tool_name)
                {
                    let call_id = value
                        .get("call_id")
                        .and_then(|value| value.as_str())
                        .unwrap_or("call_structured")
                        .to_owned();
                    calls.push(ProviderToolCall {
                        call_id,
                        tool_name: canonical_name,
                        provider_tool_name: provider_tool_name.to_owned(),
                        arguments: arguments.clone(),
                    });
                }
            }
        }
        cursor = &after_start[end + "</liz_tool_call>".len()..];
    }
    calls
}

fn parse_tool_call_arguments(value: Option<&serde_json::Value>) -> Option<serde_json::Value> {
    let value = value?;
    if let Some(raw) = value.as_str() {
        return serde_json::from_str::<serde_json::Value>(raw).ok();
    }
    Some(value.clone())
}

fn normalize_tool_name(tool_surface: &ToolSurfaceSpec, provider_tool_name: &str) -> Option<String> {
    tool_surface.name_map.canonical_name(provider_tool_name).map(str::to_owned).or_else(|| {
        tool_surface
            .name_map
            .provider_name(provider_tool_name)
            .map(|_| provider_tool_name.to_owned())
    })
}

fn clean_structured_fallback_text(value: &str) -> String {
    let mut cleaned = value.to_owned();
    loop {
        let Some(start) = cleaned.find("<liz_tool_call>") else {
            break;
        };
        let Some(end) = cleaned[start..].find("</liz_tool_call>") else {
            break;
        };
        let end_index = start + end + "</liz_tool_call>".len();
        cleaned.replace_range(start..end_index, "");
    }
    cleaned.trim().to_owned()
}

fn provider_supports_patching(family: &ModelProviderFamily) -> bool {
    matches!(family, ModelProviderFamily::OpenAiResponses)
}

fn supports_reasoning_accounting(family: &ModelProviderFamily) -> bool {
    matches!(family, ModelProviderFamily::OpenAiResponses | ModelProviderFamily::GitLabDuo)
}

fn default_openai_style_headers(
    provider: &ResolvedProvider,
    plan: &ProviderInvocationPlan,
) -> Result<std::collections::BTreeMap<String, String>, ModelError> {
    let mut headers = provider.headers.clone();
    if provider.spec.id == "amazon-bedrock-mantle" {
        let region = provider.metadata.get("aws.region").map(String::as_str).unwrap_or("us-east-1");
        let token = resolve_bedrock_mantle_runtime_auth(provider.api_key.as_deref(), region)?;
        headers.insert("Authorization".to_owned(), format!("Bearer {token}"));
        return Ok(headers);
    }

    if provider.spec.id == "openai-codex" {
        let expires_at_ms = provider
            .metadata
            .get("openai_codex.expires_at_ms")
            .and_then(|value| value.parse::<u64>().ok());
        let runtime = resolve_openai_codex_runtime_auth(OpenAiCodexRuntimeAuthRequest {
            access_token: provider.api_key.as_deref(),
            refresh_token: provider.metadata.get("openai_codex.refresh_token").map(String::as_str),
            expires_at_ms,
            account_id: provider.metadata.get("openai_codex.account_id").map(String::as_str),
            token_url_override: provider.metadata.get("openai_codex.token_url").map(String::as_str),
        })?;
        headers.insert("Authorization".to_owned(), format!("Bearer {}", runtime.access_token));
        if let Some(account_id) = runtime.account_id {
            headers.insert("ChatGPT-Account-Id".to_owned(), account_id);
        }
        return Ok(headers);
    }

    if provider.spec.id == "sap-ai-core" {
        let bearer = resolve_sap_ai_core_runtime_auth(
            provider.api_key.as_deref(),
            provider.metadata.get("sap_ai_core.client_id").map(String::as_str),
            provider.metadata.get("sap_ai_core.client_secret").map(String::as_str),
            provider.metadata.get("sap_ai_core.oauth_base_url").map(String::as_str),
        )?;
        headers.insert("Authorization".to_owned(), format!("Bearer {bearer}"));
        if let Some(resource_group) = provider
            .metadata
            .get("sap_ai_core.resource_group")
            .map(String::as_str)
            .filter(|value| !value.trim().is_empty())
        {
            headers.insert("AI-Resource-Group".to_owned(), resource_group.to_owned());
        }
        return Ok(headers);
    }

    if matches!(provider.spec.id, "azure" | "azure-cognitive-services" | "microsoft-foundry") {
        let api_key = provider.api_key.as_ref().ok_or_else(|| {
            ModelError::ProviderFailure(format!(
                "{} requires an API key for live mode",
                provider.spec.id
            ))
        })?;
        headers.insert("api-key".to_owned(), api_key.clone());
        headers.remove("Authorization");
        return Ok(headers);
    }

    if let Some(api_key) = provider.api_key.as_ref() {
        headers.entry("Authorization".to_owned()).or_insert_with(|| format!("Bearer {api_key}"));
    }
    if matches!(plan.family, ModelProviderFamily::GitLabDuo) {
        headers
            .entry("Authorization".to_owned())
            .or_insert_with(|| format!("Bearer {}", provider.api_key.clone().unwrap_or_default()));
    }
    Ok(headers)
}

fn extract_openai_style_text(response: &serde_json::Value) -> Option<String> {
    response
        .get("output")
        .and_then(|value| value.as_array())
        .and_then(|items| {
            items.iter().find_map(|item| {
                item.get("content").and_then(|value| value.as_array()).and_then(|parts| {
                    parts.iter().find_map(|part| {
                        part.get("text").and_then(|value| value.as_str()).map(str::to_owned)
                    })
                })
            })
        })
        .or_else(|| {
            response
                .get("content")
                .and_then(|value| value.as_array())
                .and_then(|items| items.first())
                .and_then(|item| item.get("text"))
                .and_then(|value| value.as_str())
                .map(str::to_owned)
        })
        .or_else(|| {
            response
                .get("choices")
                .and_then(|value| value.as_array())
                .and_then(|items| items.first())
                .and_then(|choice| {
                    choice
                        .get("message")
                        .and_then(|message| message.get("content"))
                        .and_then(|value| value.as_str())
                        .map(str::to_owned)
                })
        })
        .or_else(|| response.as_str().map(str::to_owned))
}

fn extract_openai_style_usage(
    request: &ModelTurnRequest,
    plan: &ProviderInvocationPlan,
    response: &serde_json::Value,
) -> UsageDelta {
    let usage = response.get("usage");
    let input_tokens = usage
        .and_then(|value| value.get("input_tokens").or_else(|| value.get("prompt_tokens")))
        .and_then(|value| value.as_u64())
        .and_then(|value| u32::try_from(value).ok())
        .unwrap_or_else(|| estimate_tokens(&request.prompt));
    let output_tokens = usage
        .and_then(|value| value.get("output_tokens").or_else(|| value.get("completion_tokens")))
        .and_then(|value| value.as_u64())
        .and_then(|value| u32::try_from(value).ok())
        .unwrap_or_else(|| estimate_tokens(&plan.model_id));
    let reasoning_tokens = usage
        .and_then(|value| value.get("output_tokens_details"))
        .and_then(|value| value.get("reasoning_tokens"))
        .and_then(|value| value.as_u64())
        .and_then(|value| u32::try_from(value).ok())
        .unwrap_or(0);
    let cache_hit_tokens = usage
        .and_then(|value| value.get("input_tokens_details"))
        .and_then(|value| {
            value.get("cached_tokens").or_else(|| value.get("cache_read_input_tokens"))
        })
        .and_then(|value| value.as_u64())
        .and_then(|value| u32::try_from(value).ok())
        .unwrap_or(0);
    let cache_write_tokens = usage
        .and_then(|value| value.get("input_tokens_details"))
        .and_then(|value| value.get("cache_creation_input_tokens"))
        .and_then(|value| value.as_u64())
        .and_then(|value| u32::try_from(value).ok())
        .unwrap_or(0);

    UsageDelta {
        input_tokens,
        output_tokens,
        reasoning_tokens,
        cache_hit_tokens,
        cache_write_tokens,
    }
}

fn trim_trailing_slash(value: &str) -> &str {
    value.trim_end_matches('/')
}

fn should_use_copilot_responses_api(model_id: &str) -> bool {
    let Some(rest) = model_id.strip_prefix("gpt-") else {
        return false;
    };

    let version = rest
        .split(|character: char| !character.is_ascii_digit())
        .next()
        .and_then(|segment| segment.parse::<u32>().ok())
        .unwrap_or_default();

    version >= 5 && !model_id.starts_with("gpt-5-mini")
}

fn copilot_uses_anthropic_messages_model(model_id: &str) -> bool {
    model_id.to_ascii_lowercase().contains("claude")
}

fn gitlab_uses_private_token(api_key: &str, provider: &ResolvedProvider) -> bool {
    provider
        .metadata
        .get("gitlab.auth_mode")
        .map(|value| value == "pat")
        .unwrap_or_else(|| api_key.starts_with("glpat-"))
}

fn gitlab_chat_endpoint(base_url: &str) -> String {
    let trimmed = trim_trailing_slash(base_url);
    if trimmed.ends_with("/api/v4/chat/completions") {
        trimmed.to_owned()
    } else if trimmed.ends_with("/api/v4") {
        format!("{trimmed}/chat/completions")
    } else {
        format!("{trimmed}/api/v4/chat/completions")
    }
}

fn openai_compatible_chat_path(base_url: &str) -> String {
    let path = reqwest::Url::parse(base_url)
        .ok()
        .map(|url| url.path().trim_end_matches('/').to_owned())
        .unwrap_or_default();
    if path.is_empty() {
        "/v1/chat/completions".to_owned()
    } else {
        "/chat/completions".to_owned()
    }
}

fn openai_responses_path(base_url: &str) -> String {
    let path = reqwest::Url::parse(base_url)
        .ok()
        .map(|url| url.path().trim_end_matches('/').to_owned())
        .unwrap_or_default();
    if path.is_empty() {
        "/v1/responses".to_owned()
    } else {
        "/responses".to_owned()
    }
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

fn synthesize_tool_arguments(prompt: &str, thread_id: &str, provider_id: &str) -> String {
    let tool_name = infer_tool_name(prompt);
    if tool_name == "shell.exec" {
        let command = extract_shell_command(prompt).unwrap_or_else(|| prompt.to_owned());
        json!({
            "command": command,
            "working_dir": serde_json::Value::Null,
            "thread_id": thread_id,
            "provider": provider_id,
        })
        .to_string()
    } else {
        json!({
            "goal": prompt,
            "thread_id": thread_id,
            "provider": provider_id,
        })
        .to_string()
    }
}

fn extract_shell_command(prompt: &str) -> Option<String> {
    let lower = prompt.to_ascii_lowercase();
    lower.find("run command:").map(|index| {
        prompt[index + "run command:".len()..].lines().next().unwrap_or_default().trim().to_owned()
    })
}

fn truncate_preview(prompt: &str) -> String {
    let mut preview = prompt.chars().take(24).collect::<String>();
    if prompt.chars().count() > 24 {
        preview.push_str("...");
    }
    preview
}

fn estimate_tokens(text: &str) -> u32 {
    let words = text.split_whitespace().count().max(1);
    u32::try_from(words.saturating_mul(3)).unwrap_or(u32::MAX)
}
