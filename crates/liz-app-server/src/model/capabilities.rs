//! Model capability descriptions used by the runtime.

/// The capability matrix advertised by a provider adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelCapabilities {
    /// Whether assistant text can stream incrementally.
    pub assistant_text_streaming: bool,
    /// Whether tool calls can stream incrementally.
    pub tool_call_streaming: bool,
    /// Whether tool calls support patch-style argument deltas before commit.
    pub tool_call_patching: bool,
    /// Whether text and tool calls may interleave in a single turn.
    pub interleaved_text_and_tool_calls: bool,
    /// Whether the model can emit more than one tool call in parallel.
    pub parallel_tool_calls: bool,
    /// Whether the adapter enforces strict tool schemas.
    pub strict_tool_schema: bool,
    /// Whether the provider supports prompt caching hints.
    pub prompt_caching: bool,
    /// Whether the provider keeps server-side conversation state.
    pub server_side_conversation_state: bool,
    /// Whether reasoning-token accounting is surfaced separately.
    pub reasoning_token_accounting: bool,
    /// Whether image input is accepted.
    pub image_input: bool,
    /// The advertised maximum context window.
    pub max_context_window: usize,
}

impl ModelCapabilities {
    /// Returns a capability matrix that mirrors a strong OpenAI-style streaming adapter.
    pub fn openai_streaming() -> Self {
        Self {
            assistant_text_streaming: true,
            tool_call_streaming: true,
            tool_call_patching: true,
            interleaved_text_and_tool_calls: true,
            parallel_tool_calls: false,
            strict_tool_schema: true,
            prompt_caching: false,
            server_side_conversation_state: false,
            reasoning_token_accounting: true,
            image_input: true,
            max_context_window: 128_000,
        }
    }

    /// Returns a conservative capability matrix for future OpenAI-compatible providers.
    pub fn openai_compatible() -> Self {
        Self {
            assistant_text_streaming: true,
            tool_call_streaming: true,
            tool_call_patching: false,
            interleaved_text_and_tool_calls: false,
            parallel_tool_calls: false,
            strict_tool_schema: false,
            prompt_caching: false,
            server_side_conversation_state: false,
            reasoning_token_accounting: false,
            image_input: false,
            max_context_window: 32_000,
        }
    }

    /// Returns a conservative capability matrix for a local gateway.
    pub fn local_gateway() -> Self {
        Self {
            assistant_text_streaming: true,
            tool_call_streaming: false,
            tool_call_patching: false,
            interleaved_text_and_tool_calls: false,
            parallel_tool_calls: false,
            strict_tool_schema: false,
            prompt_caching: false,
            server_side_conversation_state: false,
            reasoning_token_accounting: false,
            image_input: false,
            max_context_window: 16_000,
        }
    }
}
