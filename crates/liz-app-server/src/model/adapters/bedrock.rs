//! Adapter for AWS Bedrock providers.

use crate::model::config::ResolvedProvider;
use crate::model::gateway::{ModelError, ModelRunSummary, ModelTurnRequest};
use crate::model::invocation::{InvocationTransport, ProviderInvocationPlan};
use crate::model::normalized_stream::{NormalizedTurnEvent, UsageDelta};
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
        sink: &mut dyn FnMut(NormalizedTurnEvent),
    ) -> Result<ModelRunSummary, ModelError> {
        let resolved_model = prefix_bedrock_model(provider, &provider.model_id);
        let plan = ProviderInvocationPlan {
            provider_id: provider.spec.id.to_owned(),
            display_name: provider.spec.display_name.to_owned(),
            family: provider.spec.family,
            model_id: resolved_model.clone(),
            auth_kind: provider.spec.auth_kind,
            transport: InvocationTransport::ProviderOperation {
                operation: "aws.bedrock.converse_stream",
                base_url: provider.base_url.clone(),
            },
            headers: provider.headers.clone(),
            payload_preview: json!({
                "modelId": resolved_model,
                "messages": [{"role": "user", "content": [{"text": request.prompt}]}],
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
            chunk: format!("Using {}. ", plan.display_name),
        });
        sink(NormalizedTurnEvent::AssistantDelta {
            chunk: format!("Resolved Bedrock model {}.", plan.model_id),
        });
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
        let final_message = format!(
            "{} request prepared for {} using aws-bedrock-converse.",
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

fn prefix_bedrock_model(provider: &ResolvedProvider, model_id: &str) -> String {
    if ["global.", "us.", "eu.", "jp.", "apac.", "au."]
        .iter()
        .any(|prefix| model_id.starts_with(prefix))
    {
        return model_id.to_owned();
    }

    let region = provider
        .metadata
        .get("aws.region")
        .map(String::as_str)
        .unwrap_or("us-east-1");
    let region_prefix = region.split('-').next().unwrap_or("us");
    let lower = model_id.to_ascii_lowercase();

    if region_prefix == "us"
        && !region.starts_with("us-gov")
        && ["nova-micro", "nova-lite", "nova-pro", "nova-premier", "nova-2", "claude", "deepseek"]
            .iter()
            .any(|marker| lower.contains(marker))
    {
        return format!("us.{model_id}");
    }

    if region_prefix == "eu"
        && [
            "eu-west-1",
            "eu-west-2",
            "eu-west-3",
            "eu-north-1",
            "eu-central-1",
            "eu-south-1",
            "eu-south-2",
        ]
        .iter()
        .any(|candidate| region == *candidate)
        && ["claude", "nova-lite", "nova-micro", "llama3", "pixtral"]
            .iter()
            .any(|marker| lower.contains(marker))
    {
        return format!("eu.{model_id}");
    }

    if ["ap-southeast-2", "ap-southeast-4"].contains(&region)
        && [
            "anthropic.claude-sonnet-4-6",
            "anthropic.claude-haiku",
        ]
            .iter()
            .any(|marker| lower.contains(marker))
    {
        return format!("au.{model_id}");
    }

    if region == "ap-northeast-1"
        && ["claude", "nova-lite", "nova-micro", "nova-pro"]
            .iter()
            .any(|marker| lower.contains(marker))
    {
        return format!("jp.{model_id}");
    }

    if region.starts_with("ap-")
        && ["claude", "nova-lite", "nova-micro", "nova-pro"]
            .iter()
            .any(|marker| lower.contains(marker))
    {
        return format!("apac.{model_id}");
    }

    model_id.to_owned()
}

fn estimate_tokens(text: &str) -> u32 {
    let words = text.split_whitespace().count().max(1);
    u32::try_from(words.saturating_mul(3)).unwrap_or(u32::MAX)
}
