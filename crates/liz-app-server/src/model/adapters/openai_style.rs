//! Adapter for OpenAI-style, OpenAI-compatible, Copilot, and GitLab provider families.

use crate::model::config::ResolvedProvider;
use crate::model::family::ModelProviderFamily;
use crate::model::gateway::{ModelError, ModelRunSummary, ModelTurnRequest};
use crate::model::http::{build_client, post_json};
use crate::model::invocation::{InvocationTransport, ProviderInvocationPlan};
use crate::model::normalized_stream::{NormalizedTurnEvent, UsageDelta};
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
        sink: &mut dyn FnMut(NormalizedTurnEvent),
    ) -> Result<ModelRunSummary, ModelError> {
        let plan = self.build_plan(provider, &request)?;
        if should_attempt_live_http(provider, &plan) {
            return execute_live_http(provider, &plan, request, sink);
        }

        simulate_stream(plan, request, sink)
    }

    fn build_plan(
        &self,
        provider: &ResolvedProvider,
        request: &ModelTurnRequest,
    ) -> Result<ProviderInvocationPlan, ModelError> {
        let payload_preview = match provider.spec.family {
            ModelProviderFamily::OpenAiResponses => json!({
                "model": provider.model_id,
                "input": request.prompt,
                "stream": true,
            })
            .to_string(),
            ModelProviderFamily::GitHubCopilot => json!({
                "model": provider.model_id,
                "input": request.prompt,
                "mode": if should_use_copilot_responses_api(&provider.model_id) { "responses" } else { "chat" },
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
                    {"role": "user", "content": request.prompt}
                ],
                "stream": true,
            })
            .to_string(),
        };

        let transport = match provider.spec.family {
            ModelProviderFamily::OpenAiResponses => InvocationTransport::HttpJson {
                method: "POST",
                base_url: provider
                    .base_url
                    .clone()
                    .unwrap_or_else(|| "https://api.openai.com".to_owned()),
                path: "/v1/responses".to_owned(),
            },
            ModelProviderFamily::OpenAiCompatible => match provider.base_url.clone() {
                Some(base_url) => InvocationTransport::HttpJson {
                    method: "POST",
                    base_url,
                    path: "/v1/chat/completions".to_owned(),
                },
                None => InvocationTransport::ProviderOperation {
                    operation: "openai-compatible.chat-completions",
                    base_url: None,
                },
            },
            ModelProviderFamily::GitHubCopilot => {
                if should_use_copilot_responses_api(&provider.model_id) {
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
            notes: provider
                .spec
                .notes
                .iter()
                .map(|note| (*note).to_owned())
                .collect(),
        })
    }
}

fn execute_live_http(
    provider: &ResolvedProvider,
    plan: &ProviderInvocationPlan,
    request: ModelTurnRequest,
    sink: &mut dyn FnMut(NormalizedTurnEvent),
) -> Result<ModelRunSummary, ModelError> {
    let (url, body) = match &plan.transport {
        InvocationTransport::HttpJson {
            base_url, path, ..
        } => {
            let body = match plan.family {
                ModelProviderFamily::OpenAiResponses => json!({
                    "model": provider.model_id,
                    "input": request.prompt,
                    "stream": false,
                }),
                _ => json!({
                    "model": provider.model_id,
                    "messages": [{"role": "user", "content": request.prompt}],
                    "stream": false,
                }),
            };
            (format!("{}{}", trim_trailing_slash(base_url), path), body)
        }
        InvocationTransport::ProviderOperation { .. } => {
            return simulate_stream(plan.clone(), request, sink);
        }
    };

    let mut headers = provider.headers.clone();
    if let Some(api_key) = provider.api_key.as_ref() {
        headers
            .entry("Authorization".to_owned())
            .or_insert_with(|| format!("Bearer {api_key}"));
    }
    if matches!(plan.family, ModelProviderFamily::GitLabDuo) {
        headers
            .entry("Authorization".to_owned())
            .or_insert_with(|| format!("Bearer {}", provider.api_key.clone().unwrap_or_default()));
    }

    let response = post_json(&build_client()?, &url, &headers, &body)?;
    let assistant_message = extract_openai_style_text(&response).unwrap_or_else(|| {
        format!(
            "{} response received for {}.",
            plan.display_name, plan.model_id
        )
    });

    sink(NormalizedTurnEvent::AssistantDelta {
        chunk: format!("Live response from {}.", plan.display_name),
    });
    sink(NormalizedTurnEvent::AssistantMessage {
        message: assistant_message.clone(),
    });

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

fn simulate_stream(
    plan: ProviderInvocationPlan,
    request: ModelTurnRequest,
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

    if needs_tool_call(&request.prompt) {
        let tool_name = infer_tool_name(&request.prompt);
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
                    truncate_preview(&request.prompt),
                    plan.provider_id
                )),
            });
        }
        sink(NormalizedTurnEvent::ToolCallCommitted {
            call_id: "call_01".to_owned(),
            tool_name,
            arguments: format!(
                "{{\"goal\":\"{}\",\"thread_id\":\"{}\",\"provider\":\"{}\"}}",
                request.prompt, request.thread.id, plan.provider_id
            ),
        });
    }

    sink(NormalizedTurnEvent::ProviderRawEvent {
        label: format!("request-plan {}", plan.payload_preview),
    });

    let usage = UsageDelta {
        input_tokens: estimate_tokens(&request.prompt),
        output_tokens: estimate_tokens(&request.prompt) + 12,
        reasoning_tokens: if supports_reasoning_accounting(&plan.family) {
            6
        } else {
            0
        },
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
    sink(NormalizedTurnEvent::AssistantMessage {
        message: final_message.clone(),
    });

    Ok(ModelRunSummary {
        assistant_message: Some(final_message),
        usage,
    })
}

fn provider_supports_patching(family: &ModelProviderFamily) -> bool {
    matches!(family, ModelProviderFamily::OpenAiResponses)
}

fn supports_reasoning_accounting(family: &ModelProviderFamily) -> bool {
    matches!(family, ModelProviderFamily::OpenAiResponses | ModelProviderFamily::GitLabDuo)
}

fn should_attempt_live_http(provider: &ResolvedProvider, plan: &ProviderInvocationPlan) -> bool {
    if std::env::var("LIZ_PROVIDER_ENABLE_LIVE").ok().as_deref() != Some("1") {
        return false;
    }

    match plan.family {
        ModelProviderFamily::OpenAiResponses => provider.api_key.is_some(),
        ModelProviderFamily::OpenAiCompatible => provider.base_url.is_some() || provider.api_key.is_some(),
        ModelProviderFamily::GitHubCopilot | ModelProviderFamily::GitLabDuo => {
            provider.api_key.is_some() && provider.base_url.is_some()
        }
        _ => false,
    }
}

fn extract_openai_style_text(response: &serde_json::Value) -> Option<String> {
    response
        .get("output")
        .and_then(|value| value.as_array())
        .and_then(|items| {
            items.iter().find_map(|item| {
                item.get("content")
                    .and_then(|value| value.as_array())
                    .and_then(|parts| {
                        parts.iter().find_map(|part| {
                            part.get("text")
                                .and_then(|value| value.as_str())
                                .map(str::to_owned)
                        })
                    })
            })
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
