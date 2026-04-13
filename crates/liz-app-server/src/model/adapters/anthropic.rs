//! Adapter for Anthropic Messages-compatible providers.

use crate::model::config::ResolvedProvider;
use crate::model::gateway::{ModelError, ModelRunSummary, ModelTurnRequest};
use crate::model::invocation::{InvocationTransport, ProviderInvocationPlan};
use crate::model::normalized_stream::{NormalizedTurnEvent, UsageDelta};
use serde_json::json;

/// Anthropic family adapter.
#[derive(Debug, Clone, Default)]
pub struct AnthropicAdapter;

impl AnthropicAdapter {
    /// Streams one turn through an Anthropic-compatible provider.
    pub fn stream_turn(
        &self,
        provider: &ResolvedProvider,
        request: ModelTurnRequest,
        sink: &mut dyn FnMut(NormalizedTurnEvent),
    ) -> Result<ModelRunSummary, ModelError> {
        let base_url = provider
            .base_url
            .clone()
            .unwrap_or_else(|| "https://api.anthropic.com".to_owned());
        let plan = ProviderInvocationPlan {
            provider_id: provider.spec.id.to_owned(),
            display_name: provider.spec.display_name.to_owned(),
            family: provider.spec.family,
            model_id: provider.model_id.clone(),
            auth_kind: provider.spec.auth_kind,
            transport: InvocationTransport::HttpJson {
                method: "POST",
                base_url,
                path: "/v1/messages".to_owned(),
            },
            headers: provider.headers.clone(),
            payload_preview: json!({
                "model": provider.model_id,
                "max_tokens": 4096,
                "messages": [{"role": "user", "content": request.prompt}],
                "stream": true,
            })
            .to_string(),
            notes: provider
                .spec
                .notes
                .iter()
                .map(|note| (*note).to_owned())
                .collect(),
        };

        sink(NormalizedTurnEvent::AssistantDelta {
            chunk: format!("Using {} messages API. ", plan.display_name),
        });
        sink(NormalizedTurnEvent::AssistantDelta {
            chunk: format!("Model {} is ready.", plan.model_id),
        });
        sink(NormalizedTurnEvent::ProviderRawEvent {
            label: format!("request-plan {}", plan.payload_preview),
        });
        let usage = UsageDelta {
            input_tokens: estimate_tokens(&request.prompt),
            output_tokens: estimate_tokens(&request.prompt) + 10,
            reasoning_tokens: 4,
            cache_hit_tokens: 0,
            cache_write_tokens: 0,
        };
        sink(NormalizedTurnEvent::UsageDelta(usage.clone()));
        let final_message = format!(
            "{} request prepared for {} using anthropic-messages.",
            plan.display_name, plan.model_id
        );
        sink(NormalizedTurnEvent::AssistantMessage {
            message: final_message.clone(),
        });

        Ok(ModelRunSummary {
            assistant_message: Some(final_message),
            usage,
        })
    }
}

fn estimate_tokens(text: &str) -> u32 {
    let words = text.split_whitespace().count().max(1);
    u32::try_from(words.saturating_mul(3)).unwrap_or(u32::MAX)
}
