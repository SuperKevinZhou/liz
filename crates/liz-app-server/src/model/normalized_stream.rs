//! Provider-agnostic normalized turn-stream events.

/// A normalized usage-accounting delta.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UsageDelta {
    /// Input tokens attributed to the provider call.
    pub input_tokens: u32,
    /// Output tokens attributed to the provider call.
    pub output_tokens: u32,
    /// Reasoning tokens, if the provider exposes them.
    pub reasoning_tokens: u32,
    /// Prompt-cache hits, when exposed by the provider.
    pub cache_hit_tokens: u32,
    /// Prompt-cache writes, when exposed by the provider.
    pub cache_write_tokens: u32,
}

impl UsageDelta {
    /// Adds another delta into this one.
    pub fn add_assign(&mut self, other: &UsageDelta) {
        self.input_tokens += other.input_tokens;
        self.output_tokens += other.output_tokens;
        self.reasoning_tokens += other.reasoning_tokens;
        self.cache_hit_tokens += other.cache_hit_tokens;
        self.cache_write_tokens += other.cache_write_tokens;
    }
}

/// A provider-agnostic normalized event emitted while a model turn is running.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NormalizedTurnEvent {
    /// Assistant text that is still streaming.
    AssistantDelta {
        /// The latest assistant-text delta chunk.
        chunk: String,
    },
    /// The final completed assistant message.
    AssistantMessage {
        /// The final assistant message after streaming ends.
        message: String,
    },
    /// A tool call started forming.
    ToolCallStarted {
        /// The provider-scoped tool-call identifier.
        call_id: String,
        /// The tool name chosen by the model.
        tool_name: String,
        /// A short preview of why the call is forming.
        summary: String,
    },
    /// A tool call received a patch-style argument delta.
    ToolCallDelta {
        /// The provider-scoped tool-call identifier.
        call_id: String,
        /// The tool name chosen by the model.
        tool_name: String,
        /// A summary of what changed in the arguments.
        delta_summary: String,
        /// The current preview of the in-progress arguments.
        preview: Option<String>,
    },
    /// A tool call became executable.
    ToolCallCommitted {
        /// The provider-scoped tool-call identifier.
        call_id: String,
        /// The tool name chosen by the model.
        tool_name: String,
        /// The final executable arguments payload.
        arguments: String,
    },
    /// A usage delta became available.
    UsageDelta(UsageDelta),
    /// The provider exposed a raw event that may help debugging.
    ProviderRawEvent {
        /// A short debug label describing the raw provider event.
        label: String,
    },
}
