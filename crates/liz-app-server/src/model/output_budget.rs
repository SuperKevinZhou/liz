//! Output-token budgeting for provider requests.

use crate::model::config::ResolvedProvider;

const MAX_OUTPUT_TOKENS_CAP: usize = 32_000;
const CONTEXT_WINDOW_DIVISOR: usize = 4;
const MIN_OUTPUT_TOKENS: usize = 1_024;

/// The resolved output-token budget for one provider request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutputBudget {
    /// The bounded output token target.
    pub max_output_tokens: usize,
}

impl OutputBudget {
    /// Resolves an output-token budget from the provider's capability envelope.
    pub fn for_provider(provider: &ResolvedProvider) -> Self {
        let context_window = provider.spec.capabilities.max_context_window;
        let context_derived = context_window
            .checked_div(CONTEXT_WINDOW_DIVISOR)
            .unwrap_or(0)
            .clamp(MIN_OUTPUT_TOKENS, MAX_OUTPUT_TOKENS_CAP);
        Self { max_output_tokens: context_derived }
    }
}
