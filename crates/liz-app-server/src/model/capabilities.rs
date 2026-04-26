//! Model capability descriptions used by the runtime.

/// The capability matrix advertised by a provider adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelCapabilities {
    /// Whether the provider supports native tool-call request/response shapes.
    pub native_tool_calls: bool,
    /// Whether liz can fall back to a structured tool-call protocol when native tools are absent.
    pub structured_tool_protocol: bool,
    /// Whether tool results can be injected back for continuation turns.
    pub tool_result_continuation: bool,
    /// Whether incremental tool-call deltas are streamed before tool-call commit.
    pub streaming_tool_call_delta: bool,
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
    /// Returns a capability matrix for a strong OpenAI-style streaming adapter.
    pub fn openai_streaming() -> Self {
        Self {
            native_tool_calls: true,
            structured_tool_protocol: true,
            tool_result_continuation: true,
            streaming_tool_call_delta: true,
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
            native_tool_calls: true,
            structured_tool_protocol: true,
            tool_result_continuation: true,
            streaming_tool_call_delta: false,
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
            max_context_window: 32_000,
        }
    }

    /// Returns a capability matrix for Anthropic Messages-style providers.
    pub fn anthropic_messages() -> Self {
        Self {
            native_tool_calls: true,
            structured_tool_protocol: true,
            tool_result_continuation: true,
            streaming_tool_call_delta: false,
            assistant_text_streaming: true,
            tool_call_streaming: true,
            tool_call_patching: false,
            interleaved_text_and_tool_calls: true,
            parallel_tool_calls: false,
            strict_tool_schema: true,
            prompt_caching: true,
            server_side_conversation_state: false,
            reasoning_token_accounting: true,
            image_input: true,
            max_context_window: 200_000,
        }
    }

    /// Returns a capability matrix for Google-family providers.
    pub fn google_family() -> Self {
        Self {
            native_tool_calls: true,
            structured_tool_protocol: true,
            tool_result_continuation: true,
            streaming_tool_call_delta: false,
            assistant_text_streaming: true,
            tool_call_streaming: true,
            tool_call_patching: false,
            interleaved_text_and_tool_calls: true,
            parallel_tool_calls: true,
            strict_tool_schema: false,
            prompt_caching: true,
            server_side_conversation_state: false,
            reasoning_token_accounting: false,
            image_input: true,
            max_context_window: 1_000_000,
        }
    }

    /// Returns a capability matrix for AWS Bedrock converse providers.
    pub fn bedrock_converse() -> Self {
        Self {
            native_tool_calls: true,
            structured_tool_protocol: true,
            tool_result_continuation: true,
            streaming_tool_call_delta: false,
            assistant_text_streaming: true,
            tool_call_streaming: true,
            tool_call_patching: false,
            interleaved_text_and_tool_calls: true,
            parallel_tool_calls: false,
            strict_tool_schema: true,
            prompt_caching: false,
            server_side_conversation_state: false,
            reasoning_token_accounting: false,
            image_input: true,
            max_context_window: 200_000,
        }
    }

    /// Returns a conservative capability matrix for a local gateway.
    pub fn local_gateway() -> Self {
        Self {
            native_tool_calls: false,
            structured_tool_protocol: true,
            tool_result_continuation: true,
            streaming_tool_call_delta: false,
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

    /// Overrides the maximum context window.
    pub fn with_max_context_window(mut self, max_context_window: usize) -> Self {
        self.max_context_window = max_context_window;
        self
    }

    /// Overrides strict schema support.
    pub fn with_strict_tool_schema(mut self, strict_tool_schema: bool) -> Self {
        self.strict_tool_schema = strict_tool_schema;
        self
    }

    /// Overrides prompt caching support.
    pub fn with_prompt_caching(mut self, prompt_caching: bool) -> Self {
        self.prompt_caching = prompt_caching;
        self
    }

    /// Overrides tool-call streaming support.
    pub fn with_tool_call_streaming(mut self, tool_call_streaming: bool) -> Self {
        self.tool_call_streaming = tool_call_streaming;
        self
    }

    /// Overrides native tool-call support.
    pub fn with_native_tool_calls(mut self, native_tool_calls: bool) -> Self {
        self.native_tool_calls = native_tool_calls;
        self
    }

    /// Overrides structured-tool-protocol fallback support.
    pub fn with_structured_tool_protocol(mut self, structured_tool_protocol: bool) -> Self {
        self.structured_tool_protocol = structured_tool_protocol;
        self
    }

    /// Overrides tool-result continuation support.
    pub fn with_tool_result_continuation(mut self, tool_result_continuation: bool) -> Self {
        self.tool_result_continuation = tool_result_continuation;
        self
    }

    /// Overrides streaming tool-call delta support.
    pub fn with_streaming_tool_call_delta(mut self, streaming_tool_call_delta: bool) -> Self {
        self.streaming_tool_call_delta = streaming_tool_call_delta;
        self
    }

    /// Overrides image-input support.
    pub fn with_image_input(mut self, image_input: bool) -> Self {
        self.image_input = image_input;
        self
    }

    /// Overrides parallel tool-call support.
    pub fn with_parallel_tool_calls(mut self, parallel_tool_calls: bool) -> Self {
        self.parallel_tool_calls = parallel_tool_calls;
        self
    }

    /// Overrides server-side conversation support.
    pub fn with_server_side_conversation_state(
        mut self,
        server_side_conversation_state: bool,
    ) -> Self {
        self.server_side_conversation_state = server_side_conversation_state;
        self
    }

    /// Overrides reasoning token accounting support.
    pub fn with_reasoning_token_accounting(mut self, reasoning_token_accounting: bool) -> Self {
        self.reasoning_token_accounting = reasoning_token_accounting;
        self
    }
}
