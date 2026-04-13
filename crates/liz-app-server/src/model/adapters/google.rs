//! Adapter for Google-family providers.

use crate::model::config::ResolvedProvider;
use crate::model::family::ModelProviderFamily;
use crate::model::gateway::{ModelError, ModelRunSummary, ModelTurnRequest};
use crate::model::invocation::{InvocationTransport, ProviderInvocationPlan};
use crate::model::normalized_stream::{NormalizedTurnEvent, UsageDelta};
use serde_json::json;

/// Google-family adapter for direct Gemini and Vertex providers.
#[derive(Debug, Clone, Default)]
pub struct GoogleAdapter;

impl GoogleAdapter {
    /// Streams one turn through a Google-family provider.
    pub fn stream_turn(
        &self,
        provider: &ResolvedProvider,
        request: ModelTurnRequest,
        sink: &mut dyn FnMut(NormalizedTurnEvent),
    ) -> Result<ModelRunSummary, ModelError> {
        let transport = match provider.spec.family {
            ModelProviderFamily::GoogleGenerativeAi => InvocationTransport::ProviderOperation {
                operation: if provider.spec.id == "google-gemini-cli" {
                    "google-gemini-cli.session"
                } else {
                    "google.generate_content"
                },
                base_url: provider.base_url.clone(),
            },
            ModelProviderFamily::GoogleVertex => InvocationTransport::ProviderOperation {
                operation: "google-vertex.generate_content",
                base_url: provider.base_url.clone(),
            },
            ModelProviderFamily::GoogleVertexAnthropic => InvocationTransport::ProviderOperation {
                operation: "google-vertex.anthropic.messages",
                base_url: provider.base_url.clone(),
            },
            _ => {
                return Err(ModelError::ProviderFailure(format!(
                    "provider {} is not a Google-family provider",
                    provider.spec.id
                )))
            }
        };

        let plan = ProviderInvocationPlan {
            provider_id: provider.spec.id.to_owned(),
            display_name: provider.spec.display_name.to_owned(),
            family: provider.spec.family,
            model_id: provider.model_id.clone(),
            auth_kind: provider.spec.auth_kind,
            transport,
            headers: provider.headers.clone(),
            payload_preview: json!({
                "model": provider.model_id,
                "contents": [{"role": "user", "parts": [{"text": request.prompt}]}],
            })
            .to_string(),
            notes: provider
                .spec
                .notes
                .iter()
                .map(|note| (*note).to_owned())
                .collect(),
        };

        let location_hint = provider
            .metadata
            .get("google.location")
            .cloned()
            .unwrap_or_else(|| "default-location".to_owned());

        sink(NormalizedTurnEvent::AssistantDelta {
            chunk: format!("Using {}. ", plan.display_name),
        });
        sink(NormalizedTurnEvent::AssistantDelta {
            chunk: format!("Routing model {} in {}.", plan.model_id, location_hint),
        });
        sink(NormalizedTurnEvent::ProviderRawEvent {
            label: format!("request-plan {}", plan.payload_preview),
        });
        let usage = UsageDelta {
            input_tokens: estimate_tokens(&request.prompt),
            output_tokens: estimate_tokens(&request.prompt) + 9,
            reasoning_tokens: 0,
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
}

fn estimate_tokens(text: &str) -> u32 {
    let words = text.split_whitespace().count().max(1);
    u32::try_from(words.saturating_mul(3)).unwrap_or(u32::MAX)
}
