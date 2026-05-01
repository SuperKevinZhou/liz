//! Interaction context resources that separate ingress, actor identity, audience, role, and policy.

use crate::memory::ChannelRef;
use crate::primitives::Timestamp;
use serde::{Deserialize, Serialize};

/// A transport or source that delivered an inbound event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IngressRef {
    /// The ingress family, such as `web`, `telegram`, `node`, or `webhook`.
    pub kind: String,
    /// The source-owned identifier for this ingress.
    pub source_id: String,
    /// The source-owned conversation or stream identifier.
    pub conversation_id: Option<String>,
}

/// The kind of actor currently interacting with liz.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActorKind {
    /// The owner of this liz instance.
    Owner,
    /// A known human contact.
    HumanContact,
    /// An unknown or untrusted human.
    Stranger,
    /// Another agent outside this liz core.
    ExternalAgent,
    /// A local runtime node.
    LocalNode,
    /// A remote runtime node.
    RemoteNode,
    /// A trusted system service.
    SystemService,
    /// A webhook source.
    WebhookSource,
    /// A scheduled or background automation.
    Automation,
}

/// The actor associated with an interaction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActorRef {
    /// A stable actor identifier in liz-owned policy space.
    pub actor_id: String,
    /// The actor kind.
    pub kind: ActorKind,
    /// Human-readable display name.
    pub display_name: Option<String>,
    /// Optional authentication or provenance proof label.
    pub proof: Option<String>,
}

/// How broad the output audience is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudienceVisibility {
    /// Private owner-facing interaction.
    Private,
    /// A direct interaction with a non-owner contact.
    Direct,
    /// A group or shared-channel interaction.
    Group,
    /// A public or broadly visible interaction.
    Public,
    /// A machine-to-machine protocol audience.
    Machine,
}

/// The audience that may see the output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Audience {
    /// Visibility scope.
    pub visibility: AudienceVisibility,
    /// Stable participant identifiers, if known.
    pub participants: Vec<String>,
}

/// The social or operational role liz is taking for this interaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InteractionRole {
    /// Private owner-facing companion and working partner.
    PrivateCompanion,
    /// Direct task execution for the owner.
    TaskExecutor,
    /// Authorized representation of the owner.
    OwnerDelegate,
    /// Public-facing representative with conservative disclosure.
    PublicRepresentative,
    /// Participant in a shared or group channel.
    GroupParticipant,
    /// Peer interaction with another agent.
    AgentPeer,
    /// Coordinator interaction with another agent.
    AgentCoordinator,
    /// Core-to-node controller interaction.
    NodeController,
    /// Observer of a webhook event.
    WebhookObserver,
    /// Background automation context.
    BackgroundAutomation,
}

/// Runtime authority granted for this interaction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthorityScope {
    /// Whether liz may speak on behalf of the owner.
    pub can_speak_for_owner: bool,
    /// Whether this interaction may start work.
    pub can_start_work: bool,
    /// Whether tools may be called.
    pub can_call_tools: bool,
    /// Whether durable memory may be written.
    pub can_write_memory: bool,
    /// Whether owner confirmation is required before continuing.
    pub requires_owner_confirmation: bool,
}

impl AuthorityScope {
    /// Returns the default private owner authority.
    pub const fn owner_default() -> Self {
        Self {
            can_speak_for_owner: false,
            can_start_work: true,
            can_call_tools: true,
            can_write_memory: true,
            requires_owner_confirmation: false,
        }
    }

    /// Returns a conservative non-owner authority.
    pub const fn restricted_default() -> Self {
        Self {
            can_speak_for_owner: false,
            can_start_work: false,
            can_call_tools: false,
            can_write_memory: false,
            requires_owner_confirmation: true,
        }
    }
}

/// Evidence disclosure policy for outputs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidencePolicy {
    /// Evidence can be summarized but raw citations stay hidden.
    SummaryOnly,
    /// Evidence can be expanded on request.
    Expandable,
    /// Evidence must not be disclosed.
    Hidden,
}

/// Information disclosure policy resolved before prompt assembly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DisclosurePolicy {
    /// Topics explicitly allowed for this interaction.
    pub allowed_topics: Vec<String>,
    /// Topics forbidden for this interaction.
    pub forbidden_topics: Vec<String>,
    /// Whether active work state can be shared.
    pub share_active_state: bool,
    /// Whether pending commitments can be shared.
    pub share_commitments: bool,
    /// Whether owner identity/preferences can be shared.
    pub share_identity: bool,
    /// Evidence disclosure behavior.
    pub evidence_policy: EvidencePolicy,
}

impl DisclosurePolicy {
    /// Returns the full owner-facing disclosure policy.
    pub const fn owner_default() -> Self {
        Self {
            allowed_topics: Vec::new(),
            forbidden_topics: Vec::new(),
            share_active_state: true,
            share_commitments: true,
            share_identity: true,
            evidence_policy: EvidencePolicy::Expandable,
        }
    }

    /// Returns the conservative disclosure policy for unknown actors.
    pub const fn stranger_default() -> Self {
        Self {
            allowed_topics: Vec::new(),
            forbidden_topics: Vec::new(),
            share_active_state: false,
            share_commitments: false,
            share_identity: false,
            evidence_policy: EvidencePolicy::Hidden,
        }
    }
}

/// Provenance metadata for an inbound interaction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provenance {
    /// The channel metadata, when the ingress is a conversational channel.
    pub channel: Option<ChannelRef>,
    /// When the event was received, if known.
    pub received_at: Option<Timestamp>,
    /// Authentication mechanism or verifier label.
    pub authenticated_by: Option<String>,
    /// Pointer to a raw event stored in diagnostics.
    pub raw_event_ref: Option<String>,
}

/// The resolved context that controls policy, prompt rendering, and writeback.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InteractionContext {
    /// The ingress that delivered the interaction.
    pub ingress: IngressRef,
    /// The actor interacting with liz.
    pub actor: ActorRef,
    /// The audience that may see liz's output.
    pub audience: Audience,
    /// The current interaction role.
    pub role: InteractionRole,
    /// The granted authority for the interaction.
    pub authority: AuthorityScope,
    /// The resolved disclosure policy.
    pub disclosure: DisclosurePolicy,
    /// Optional task mandate or delegated scope.
    pub task_mandate: Option<String>,
    /// Provenance metadata for diagnostics and policy.
    pub provenance: Provenance,
}
