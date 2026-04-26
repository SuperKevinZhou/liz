//! Adapter for AWS Bedrock providers.

use crate::model::auth::sign_bedrock_request;
use crate::model::config::ResolvedProvider;
use crate::model::gateway::{ModelError, ModelRunSummary, ModelTurnRequest};
use crate::model::http::{build_client, post_json};
use crate::model::invocation::{InvocationTransport, ProviderInvocationPlan};
use crate::model::normalized_stream::{NormalizedTurnEvent, UsageDelta};
use crate::model::{
    OutputBudget, ProviderToolCall, ProviderToolProtocol, ToolSurfaceSpec,
};
use reqwest::Url;
use serde_json::json;

/// AWS Bedrock family adapter.
#[derive(Debug, Clone, Default)]
pub struct AwsBedrockAdapter;

impl AwsBedrockAdapter {
    /// Streams one turn through a Bedrock provider.
    pub fn stream_turn(
        &self,
        provider: &ResolvedProvider,
        request: ModelTurnRequest,
        tool_surface: ToolSurfaceSpec,
        simulate: bool,
        sink: &mut dyn FnMut(NormalizedTurnEvent),
    ) -> Result<ModelRunSummary, ModelError> {
        let instruction_prompt = request.instruction_prompt();
        let resolved_model = normalize_bedrock_model_id(&provider.model_id);
        let region = resolve_bedrock_region(provider);
        let base_url = resolve_bedrock_base_url(provider, &region);
        let plan = ProviderInvocationPlan {
            provider_id: provider.spec.id.to_owned(),
            display_name: provider.spec.display_name.to_owned(),
            family: provider.spec.family,
            model_id: resolved_model.clone(),
            auth_kind: provider.spec.auth_kind,
            transport: InvocationTransport::HttpJson {
                method: "POST",
                base_url,
                path: format!("/model/{resolved_model}/converse"),
            },
            headers: provider.headers.clone(),
            payload_preview: json!({
                "modelId": resolved_model,
                "system": [{"text": instruction_prompt}],
                "messages": [{"role": "user", "content": [{"text": request.user_prompt}]}],
            })
            .to_string(),
            notes: provider.spec.notes.iter().map(|note| (*note).to_owned()).collect(),
        };

        if simulate {
            let mut tool_calls = Vec::new();
            sink(NormalizedTurnEvent::AssistantDelta {
                chunk: format!("Using {}. ", plan.display_name),
            });
            sink(NormalizedTurnEvent::AssistantDelta {
                chunk: format!("Resolved Bedrock model {} in {}.", plan.model_id, region),
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
                output_tokens: estimate_tokens(&request.prompt) + 10,
                reasoning_tokens: 0,
                cache_hit_tokens: 0,
                cache_write_tokens: 0,
            };
            sink(NormalizedTurnEvent::UsageDelta(usage.clone()));
            let final_message = if tool_calls.is_empty() {
                let final_message = format!(
                    "{} request prepared for {} using aws-bedrock-converse.",
                    plan.display_name, plan.model_id
                );
                sink(NormalizedTurnEvent::AssistantMessage {
                    message: final_message.clone(),
                });
                Some(final_message)
            } else {
                None
            };

            return Ok(ModelRunSummary { assistant_message: final_message, usage, tool_calls });
        }

        execute_live_http(provider, &plan, &region, request, tool_surface, sink)
    }
}

fn execute_live_http(
    provider: &ResolvedProvider,
    plan: &ProviderInvocationPlan,
    region: &str,
    request: ModelTurnRequest,
    tool_surface: ToolSurfaceSpec,
    sink: &mut dyn FnMut(NormalizedTurnEvent),
) -> Result<ModelRunSummary, ModelError> {
    let InvocationTransport::HttpJson { base_url, path, .. } = &plan.transport else {
        return Err(ModelError::ProviderFailure(
            "amazon-bedrock transport must be HTTP JSON".to_owned(),
        ));
    };

    let url = format!("{}{}", trim_trailing_slash(base_url), path);
    let instruction_prompt = request.instruction_prompt();
    let output_budget = OutputBudget::for_provider(provider);
    let mut messages = vec![json!({
        "role":"user",
        "content":[{"text": request.user_prompt}]
    })];
    for injection in &request.tool_result_injections {
        let provider_tool_name = tool_surface
            .name_map
            .provider_name(&injection.tool_name)
            .unwrap_or(injection.provider_tool_name.as_str());
        messages.push(json!({
            "role":"user",
            "content":[{
                "toolResult":{
                    "toolUseId": injection.call_id,
                    "content":[{"json": injection.result}],
                    "status": if injection.is_error { "error" } else { "success" },
                    "name": provider_tool_name,
                }
            }]
        }));
    }

    let mut body = json!({
        "system": [{"text": instruction_prompt}],
        "messages": messages,
        "inferenceConfig": {
            "maxTokens": output_budget.max_output_tokens,
        },
    });
    if matches!(tool_surface.protocol, ProviderToolProtocol::Native) {
        body["toolConfig"] = json!({
            "tools": tool_surface
                .tools
                .iter()
                .map(|tool| {
                    json!({
                        "toolSpec": {
                            "name": tool.provider_name,
                            "description": tool.description,
                            "inputSchema": { "json": tool.input_json_schema }
                        }
                    })
                })
                .collect::<Vec<_>>()
        });
    }
    let body_text = serde_json::to_vec(&body).map_err(|error| {
        ModelError::ProviderFailure(format!(
            "failed to serialize Amazon Bedrock request body: {error}"
        ))
    })?;

    let mut headers = provider.headers.clone();
    headers.entry("content-type".to_owned()).or_insert_with(|| "application/json".to_owned());
    headers.entry("accept".to_owned()).or_insert_with(|| "application/json".to_owned());

    let final_headers = if let Some(token) = provider.api_key.as_ref() {
        headers.entry("Authorization".to_owned()).or_insert_with(|| format!("Bearer {token}"));
        headers
    } else {
        sign_bedrock_request("POST", &url, &headers, &body_text, region)?
    };

    let response = post_json(&build_client()?, &url, &final_headers, &body)?;
    let mut assistant_parts = Vec::new();
    let mut tool_calls = Vec::new();
    for (index, item) in response
        .get("output")
        .and_then(|value| value.get("message"))
        .and_then(|value| value.get("content"))
        .and_then(|value| value.as_array())
        .into_iter()
        .flatten()
        .enumerate()
    {
        if let Some(text) = item.get("text").and_then(|value| value.as_str()) {
            assistant_parts.push(text.to_owned());
        }
        if let Some(tool_use) = item.get("toolUse") {
            let provider_tool_name = tool_use
                .get("name")
                .and_then(|value| value.as_str())
                .unwrap_or_default()
                .to_owned();
            if let Some(canonical_name) = tool_surface.name_map.canonical_name(&provider_tool_name) {
                let call_id = tool_use
                    .get("toolUseId")
                    .and_then(|value| value.as_str())
                    .map(str::to_owned)
                    .unwrap_or_else(|| format!("call_{}", index + 1));
                let arguments = tool_use.get("input").cloned().unwrap_or_else(|| json!({}));
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
                    provider_tool_name,
                    arguments,
                });
            }
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
        sink(NormalizedTurnEvent::AssistantMessage {
            message: assistant_message.clone(),
        });
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

fn normalize_bedrock_model_id(model_id: &str) -> String {
    model_id.trim().to_owned()
}

fn resolve_bedrock_base_url(provider: &ResolvedProvider, region: &str) -> String {
    provider
        .base_url
        .clone()
        .unwrap_or_else(|| format!("https://bedrock-runtime.{region}.amazonaws.com"))
}

fn resolve_bedrock_region(provider: &ResolvedProvider) -> String {
    provider
        .metadata
        .get("aws.region")
        .cloned()
        .or_else(|| {
            provider.base_url.as_ref().and_then(|value| extract_bedrock_region_from_url(value))
        })
        .or_else(|| first_env(&["AWS_REGION", "AWS_DEFAULT_REGION"]))
        .unwrap_or_else(|| "us-east-1".to_owned())
}

fn extract_bedrock_region_from_url(url: &str) -> Option<String> {
    let host = Url::parse(url).ok()?.host_str()?.to_owned();
    if let Some(rest) = host.strip_prefix("bedrock-runtime.") {
        return rest.split('.').next().map(str::to_owned);
    }
    None
}

fn first_env(keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| std::env::var(key).ok().filter(|value| !value.trim().is_empty()))
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
