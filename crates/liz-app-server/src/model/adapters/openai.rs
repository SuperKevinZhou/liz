//! A strong OpenAI-style adapter that preserves streaming and tool-call patching semantics.

use crate::model::adapters::ProviderAdapter;
use crate::model::capabilities::ModelCapabilities;
use crate::model::gateway::{ModelError, ModelRunSummary, ModelTurnRequest};
use crate::model::normalized_stream::{NormalizedTurnEvent, UsageDelta};

/// A provider-shaped delta chunk modeled after OpenAI's streaming semantics.
#[derive(Debug, Clone, PartialEq, Eq)]
enum OpenAiChunk {
    TextDelta(String),
    ToolCallStarted { call_id: String, tool_name: String },
    ToolCallArgumentsDelta { call_id: String, tool_name: String, delta: String },
    ToolCallArgumentsDone { call_id: String, tool_name: String, arguments: String },
    Usage(UsageDelta),
}

/// A strong OpenAI-style adapter used as the v0 primary provider path.
#[derive(Debug, Clone)]
pub struct OpenAiAdapter {
    capabilities: ModelCapabilities,
}

impl Default for OpenAiAdapter {
    fn default() -> Self {
        Self { capabilities: ModelCapabilities::openai_streaming() }
    }
}

impl ProviderAdapter for OpenAiAdapter {
    fn provider_name(&self) -> &'static str {
        "openai"
    }

    fn capabilities(&self) -> &ModelCapabilities {
        &self.capabilities
    }

    fn stream_turn(
        &self,
        request: ModelTurnRequest,
        sink: &mut dyn FnMut(NormalizedTurnEvent),
    ) -> Result<ModelRunSummary, ModelError> {
        let chunks = synthesize_chunks(&request);
        let mut usage = UsageDelta::default();
        let mut assistant = String::new();
        let mut saw_tool_commit = false;

        for chunk in chunks {
            match chunk {
                OpenAiChunk::TextDelta(delta) => {
                    assistant.push_str(&delta);
                    sink(NormalizedTurnEvent::AssistantDelta { chunk: delta });
                }
                OpenAiChunk::ToolCallStarted { call_id, tool_name } => {
                    sink(NormalizedTurnEvent::ToolCallStarted {
                        call_id,
                        tool_name,
                        summary: "tool call is being prepared".to_owned(),
                    });
                }
                OpenAiChunk::ToolCallArgumentsDelta { call_id, tool_name, delta } => {
                    sink(NormalizedTurnEvent::ToolCallDelta {
                        call_id,
                        tool_name,
                        delta_summary: "arguments patched".to_owned(),
                        preview: Some(delta),
                    });
                }
                OpenAiChunk::ToolCallArgumentsDone { call_id, tool_name, arguments } => {
                    saw_tool_commit = true;
                    sink(NormalizedTurnEvent::ToolCallCommitted {
                        call_id,
                        tool_name,
                        arguments,
                    });
                }
                OpenAiChunk::Usage(delta) => {
                    usage.add_assign(&delta);
                    sink(NormalizedTurnEvent::UsageDelta(delta));
                }
            }
        }

        if assistant.is_empty() && saw_tool_commit {
            assistant = "Committed tool plan for the current turn.".to_owned();
        }

        sink(NormalizedTurnEvent::AssistantMessage {
            message: assistant.clone(),
        });

        Ok(ModelRunSummary {
            assistant_message: Some(assistant),
            usage,
        })
    }
}

fn synthesize_chunks(request: &ModelTurnRequest) -> Vec<OpenAiChunk> {
    let prompt = request.prompt.trim();
    let mut chunks = vec![
        OpenAiChunk::TextDelta("Working on: ".to_owned()),
        OpenAiChunk::TextDelta(prompt.to_owned()),
    ];

    if needs_tool_call(prompt) {
        let tool_name = infer_tool_name(prompt);
        chunks.push(OpenAiChunk::ToolCallStarted {
            call_id: "call_01".to_owned(),
            tool_name: tool_name.to_owned(),
        });
        chunks.push(OpenAiChunk::ToolCallArgumentsDelta {
            call_id: "call_01".to_owned(),
            tool_name: tool_name.to_owned(),
            delta: format!("{{\"goal\":\"{}\"", truncate_preview(prompt)),
        });
        chunks.push(OpenAiChunk::ToolCallArgumentsDone {
            call_id: "call_01".to_owned(),
            tool_name,
            arguments: format!(
                "{{\"goal\":\"{}\",\"thread_id\":\"{}\"}}",
                prompt,
                request.thread.id
            ),
        });
    }

    chunks.push(OpenAiChunk::Usage(UsageDelta {
        input_tokens: estimate_tokens(prompt),
        output_tokens: estimate_tokens(prompt) + 8,
        reasoning_tokens: 4,
        cache_hit_tokens: 0,
        cache_write_tokens: 0,
    }));
    chunks
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
