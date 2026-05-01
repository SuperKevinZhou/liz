//! Inbound event classification before an event becomes a model turn.

use liz_protocol::InteractionContext;

/// An inbound event received by the runtime boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InboundEvent {
    /// Resolved interaction context.
    pub interaction_context: InteractionContext,
    /// Optional text payload.
    pub text: Option<String>,
    /// Source-owned event kind.
    pub event_kind: String,
}

/// Classification result for inbound events.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InboundEventAction {
    /// Start or continue a model turn.
    RunTurn,
    /// Store/update runtime state without a model turn.
    StoreOnly,
    /// Notify the owner without running a model turn.
    NotifyOwner,
    /// Ignore the event after diagnostics capture.
    Ignore,
}

/// Classifies inbound events before turn creation.
#[derive(Debug, Clone, Default)]
pub struct InboundEventRouter;

impl InboundEventRouter {
    /// Returns the runtime action for an inbound event.
    pub fn classify(&self, event: &InboundEvent) -> InboundEventAction {
        match event.interaction_context.role {
            liz_protocol::InteractionRole::PrivateCompanion
            | liz_protocol::InteractionRole::TaskExecutor
            | liz_protocol::InteractionRole::OwnerDelegate => {
                if event.text.as_ref().is_some_and(|text| !text.trim().is_empty()) {
                    InboundEventAction::RunTurn
                } else {
                    InboundEventAction::StoreOnly
                }
            }
            liz_protocol::InteractionRole::WebhookObserver
            | liz_protocol::InteractionRole::BackgroundAutomation => {
                InboundEventAction::NotifyOwner
            }
            liz_protocol::InteractionRole::NodeController => InboundEventAction::StoreOnly,
            liz_protocol::InteractionRole::AgentPeer
            | liz_protocol::InteractionRole::AgentCoordinator
            | liz_protocol::InteractionRole::PublicRepresentative
            | liz_protocol::InteractionRole::GroupParticipant => InboundEventAction::RunTurn,
        }
    }
}
