//! Output policy checks that enforce disclosure boundaries after model generation.

use liz_protocol::{ActorKind, AudienceVisibility, InteractionContext};

/// The result of checking an outbound message against interaction policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutputPolicyDecision {
    /// The output can be delivered as-is.
    Allowed,
    /// The output was redacted before delivery.
    Redacted(String),
    /// Owner confirmation is required before delivery.
    NeedsOwnerConfirmation(String),
    /// The output must not be delivered.
    Denied(String),
}

/// Enforces post-generation disclosure boundaries.
#[derive(Debug, Clone, Default)]
pub struct OutputPolicy;

impl OutputPolicy {
    /// Checks an outbound message for the resolved interaction context.
    pub fn check(&self, context: &InteractionContext, output: &str) -> OutputPolicyDecision {
        if context.authority.requires_owner_confirmation {
            return OutputPolicyDecision::NeedsOwnerConfirmation(
                "Owner confirmation is required for this interaction.".to_owned(),
            );
        }

        if matches!(context.actor.kind, ActorKind::RemoteNode | ActorKind::LocalNode)
            && !context.disclosure.share_identity
            && mentions_private_identity(output)
        {
            return OutputPolicyDecision::Denied(
                "Node interactions cannot receive owner identity memory.".to_owned(),
            );
        }

        if matches!(
            context.audience.visibility,
            AudienceVisibility::Public | AudienceVisibility::Group
        ) && !context.disclosure.share_commitments
            && output.to_ascii_lowercase().contains("commitment")
        {
            return OutputPolicyDecision::Redacted(redact_commitment_language(output));
        }

        OutputPolicyDecision::Allowed
    }
}

fn mentions_private_identity(output: &str) -> bool {
    let lower = output.to_ascii_lowercase();
    ["owner prefers", "identity summary", "about you"].iter().any(|needle| lower.contains(needle))
}

fn redact_commitment_language(output: &str) -> String {
    output.replace("commitment", "follow-up").replace("Commitment", "Follow-up")
}
