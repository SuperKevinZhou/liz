//! Provider-aware prompt cache shaping.

use crate::model::config::ResolvedProvider;
use crate::model::family::ModelProviderFamily;
use serde_json::json;

/// Prompt-cache additions resolved for one outbound request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptCachePolicy {
    /// Optional provider-native cached content handle, primarily for Google-family APIs.
    pub cached_content: Option<String>,
    /// Whether Anthropic-style ephemeral cache markers should be attached.
    pub anthropic_ephemeral: bool,
    /// Optional OpenAI-style prompt cache retention mode.
    pub openai_cache_retention: Option<String>,
    /// Optional stable OpenAI-style prompt cache key.
    pub openai_prompt_cache_key: Option<String>,
}

impl PromptCachePolicy {
    /// Resolves prompt-cache shaping for a provider.
    pub fn for_provider(provider: &ResolvedProvider) -> Self {
        let cached_content = provider
            .metadata
            .get("google.cached_content")
            .cloned()
            .or_else(|| provider.metadata.get("google.cachedContent").cloned())
            .filter(|value| !value.trim().is_empty());
        let cache_retention = provider
            .metadata
            .get("prompt_cache.retention")
            .cloned()
            .or_else(|| provider.metadata.get("cache_retention").cloned())
            .unwrap_or_else(|| "ephemeral".to_owned());
        let prompt_cache_key = provider
            .metadata
            .get("prompt_cache.key")
            .cloned()
            .or_else(|| provider.metadata.get("prompt_cache_key").cloned())
            .filter(|value| !value.trim().is_empty());

        Self {
            cached_content,
            anthropic_ephemeral: provider.spec.capabilities.prompt_caching
                && matches!(
                    provider.spec.family,
                    ModelProviderFamily::AnthropicMessages
                        | ModelProviderFamily::GoogleVertexAnthropic
                ),
            openai_cache_retention: provider
                .spec
                .capabilities
                .prompt_caching
                .then_some(cache_retention)
                .filter(|_| {
                    matches!(
                        provider.spec.family,
                        ModelProviderFamily::OpenAiResponses
                            | ModelProviderFamily::OpenAiCompatible
                    )
                }),
            openai_prompt_cache_key: prompt_cache_key,
        }
    }
}

/// Wraps Anthropic-compatible system prompt text in a cache-aware block.
pub fn anthropic_system_blocks(system_prompt: &str, cache_ephemeral: bool) -> serde_json::Value {
    if system_prompt.trim().is_empty() {
        return json!([]);
    }

    if cache_ephemeral {
        json!([{
            "type": "text",
            "text": system_prompt,
            "cache_control": { "type": "ephemeral" }
        }])
    } else {
        json!([{
            "type": "text",
            "text": system_prompt
        }])
    }
}

/// Wraps an Anthropic-compatible user message in a cache-aware content block list.
pub fn anthropic_user_content(user_prompt: &str, cache_ephemeral: bool) -> serde_json::Value {
    if cache_ephemeral {
        json!([{
            "type": "text",
            "text": user_prompt,
            "cache_control": { "type": "ephemeral" }
        }])
    } else {
        json!([{
            "type": "text",
            "text": user_prompt
        }])
    }
}
