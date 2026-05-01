//! User-facing memory surfaces that productize identity, active work, and decisions.

use crate::ids::{MemoryFactId, ThreadId};
use crate::memory::{InfoBoundary, MemoryCitationRef, TrustLevel};
use crate::primitives::Timestamp;
use crate::thread::ThreadStatus;
use serde::{Deserialize, Serialize};

/// A user-editable profile field in the About You surface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AboutYouItem {
    /// Stable field key.
    pub key: String,
    /// Owner-facing label.
    pub label: String,
    /// Field value.
    pub value: String,
    /// Whether the owner has explicitly confirmed this item.
    pub confirmed: bool,
    /// Source fact when the item came from compiled memory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_fact_id: Option<MemoryFactId>,
}

/// The owner-facing L0 memory surface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AboutYouSurface {
    /// Current identity summary.
    pub identity_summary: Option<String>,
    /// Editable profile items.
    pub items: Vec<AboutYouItem>,
}

/// Updates the About You surface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AboutYouUpdate {
    /// Replacement identity summary, when provided.
    pub identity_summary: Option<String>,
    /// Full replacement list for editable items.
    pub items: Vec<AboutYouItem>,
}

/// A thread or commitment carried forward by liz.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CarryingItem {
    /// Related thread.
    pub thread_id: ThreadId,
    /// Owner-facing title.
    pub title: String,
    /// Current thread status.
    pub status: ThreadStatus,
    /// Current summary.
    pub summary: Option<String>,
    /// Pending commitments.
    pub pending_commitments: Vec<String>,
    /// Suggested next step.
    pub suggested_next_step: Option<String>,
    /// Last update time.
    pub updated_at: Timestamp,
}

/// The owner-facing L1 active-world surface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CarryingSurface {
    /// Active or interrupted work liz is carrying.
    pub active: Vec<CarryingItem>,
    /// Work that appears completed or archived.
    pub completed: Vec<CarryingItem>,
}

/// A user-facing decision or knowledge item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnowledgeItem {
    /// Backing fact identifier.
    pub fact_id: MemoryFactId,
    /// Kind label such as identity, decision, topic, or procedure.
    pub kind: String,
    /// Subject of the knowledge.
    pub subject: String,
    /// User-facing summary.
    pub summary: String,
    /// Whether the knowledge is stale.
    pub stale: bool,
    /// Last update time.
    pub updated_at: Timestamp,
    /// Evidence pointers available on demand.
    pub citations: Vec<MemoryCitationRef>,
}

/// The owner-facing L2 knowledge and decisions surface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnowledgeSurface {
    /// Knowledge items ordered for review.
    pub items: Vec<KnowledgeItem>,
}

/// A correction to an existing knowledge item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnowledgeCorrection {
    /// The fact being corrected.
    pub fact_id: MemoryFactId,
    /// Replacement user-facing value.
    pub corrected_value: String,
}

/// User-facing relationship and disclosure policy for one actor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersonBoundary {
    /// Stable actor or participant identifier.
    pub person_id: String,
    /// Human-readable display name.
    pub display_name: String,
    /// Whether the entry describes a human contact or an external agent.
    pub actor_kind: String,
    /// Owner-defined trust level.
    pub trust_level: TrustLevel,
    /// Topics explicitly allowed for this actor.
    pub shared_topics: Vec<String>,
    /// Topics that must not be disclosed.
    pub forbidden_topics: Vec<String>,
    /// Whether active work state can be shared.
    pub share_active_state: bool,
    /// Whether pending commitments can be shared.
    pub share_commitments: bool,
    /// Short stance label used by prompt rendering.
    pub interaction_stance: String,
    /// Optional owner-authored notes.
    pub notes: Option<String>,
    /// Whether owner confirmation is expected before sharing task status.
    pub requires_owner_confirmation: bool,
}

/// The owner-facing people and disclosure surface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeopleSurface {
    /// Known human contacts.
    pub humans: Vec<PersonBoundary>,
    /// Known external agents or machine actors.
    pub external_agents: Vec<PersonBoundary>,
    /// Default policy for unknown actors.
    pub default_stranger_boundary: InfoBoundary,
}
