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

#[cfg(test)]
mod tests {
    use super::{OutputPolicy, OutputPolicyDecision};
    use liz_protocol::{
        ActorKind, ActorRef, Audience, AudienceVisibility, AuthorityScope, DisclosurePolicy,
        EvidencePolicy, IngressRef, InteractionContext, InteractionRole, Provenance,
    };

    #[test]
    fn public_outputs_redact_private_commitments_when_not_shared() {
        let context = interaction_context(
            ActorKind::HumanContact,
            AudienceVisibility::Public,
            InteractionRole::PublicRepresentative,
            DisclosurePolicy::stranger_default(),
            AuthorityScope {
                requires_owner_confirmation: false,
                ..AuthorityScope::restricted_default()
            },
        );

        let decision = OutputPolicy::default()
            .check(&context, "Current commitment: finish the private project review.");

        assert_eq!(
            decision,
            OutputPolicyDecision::Redacted(
                "Current follow-up: finish the private project review.".to_owned()
            )
        );
    }

    #[test]
    fn node_outputs_cannot_receive_private_identity_memory() {
        let context = interaction_context(
            ActorKind::LocalNode,
            AudienceVisibility::Machine,
            InteractionRole::NodeController,
            DisclosurePolicy {
                evidence_policy: EvidencePolicy::Hidden,
                ..DisclosurePolicy::stranger_default()
            },
            AuthorityScope {
                requires_owner_confirmation: false,
                can_call_tools: true,
                ..AuthorityScope::restricted_default()
            },
        );

        let decision =
            OutputPolicy::default().check(&context, "About You: owner prefers concise updates.");

        assert_eq!(
            decision,
            OutputPolicyDecision::Denied(
                "Node interactions cannot receive owner identity memory.".to_owned()
            )
        );
    }

    #[test]
    fn required_owner_confirmation_blocks_delivery() {
        let context = interaction_context(
            ActorKind::ExternalAgent,
            AudienceVisibility::Direct,
            InteractionRole::AgentPeer,
            DisclosurePolicy::stranger_default(),
            AuthorityScope::restricted_default(),
        );

        let decision = OutputPolicy::default().check(&context, "Project status is green.");

        assert_eq!(
            decision,
            OutputPolicyDecision::NeedsOwnerConfirmation(
                "Owner confirmation is required for this interaction.".to_owned()
            )
        );
    }

    fn interaction_context(
        actor_kind: ActorKind,
        audience_visibility: AudienceVisibility,
        role: InteractionRole,
        disclosure: DisclosurePolicy,
        authority: AuthorityScope,
    ) -> InteractionContext {
        InteractionContext {
            ingress: IngressRef {
                kind: "test".to_owned(),
                source_id: "test-source".to_owned(),
                conversation_id: None,
            },
            actor: ActorRef {
                actor_id: "actor".to_owned(),
                kind: actor_kind,
                display_name: None,
                proof: None,
            },
            audience: Audience { visibility: audience_visibility, participants: Vec::new() },
            role,
            authority,
            disclosure,
            task_mandate: None,
            provenance: Provenance {
                channel: None,
                received_at: None,
                authenticated_by: None,
                raw_event_ref: None,
            },
        }
    }
}
