//! Memory-related protocol resources shared by requests, responses, and events.

use crate::artifact::ArtifactRef;
use crate::ids::{ArtifactId, MemoryFactId, ThreadId, TurnId};
use crate::primitives::Timestamp;
use crate::thread::ThreadStatus;
use serde::{Deserialize, Serialize};

/// A concise summary returned when a thread is resumed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResumeSummary {
    /// A one-line resume headline for the thread.
    pub headline: String,
    /// A short active summary for the thread.
    pub active_summary: Option<String>,
    /// The commitments that still need attention.
    pub pending_commitments: Vec<String>,
    /// The most recent interruption note.
    pub last_interruption: Option<String>,
}

/// The stable kind assigned to a compiled memory fact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryFactKind {
    /// Stable identity-level preference or relationship memory.
    Identity,
    /// Active world-model state for the current line of work.
    ActiveState,
    /// A pending commitment that still requires follow-through.
    Commitment,
    /// A decision recorded from prior work.
    Decision,
    /// A topic-oriented summary fact.
    Topic,
    /// A reusable procedure candidate extracted from repeated behavior.
    ProcedureCandidate,
    /// A keyword or lexical recall hint.
    Keyword,
}

/// A minimal evidence or citation pointer attached to memory output.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct MemoryCitationRef {
    /// The thread the citation came from.
    pub thread_id: ThreadId,
    /// The turn associated with the citation, if any.
    pub turn_id: Option<TurnId>,
    /// The artifact associated with the citation, if any.
    pub artifact_id: Option<ArtifactId>,
    /// A concise label describing what this citation points to.
    pub note: String,
}

/// The recent-conversation wake-up view surfaced to clients.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecentConversationWakeupView {
    /// Recent summaries that restore the active line of work.
    pub recent_summaries: Vec<String>,
    /// Topics that remained active across the recent conversation window.
    pub active_topics: Vec<String>,
    /// Lightweight keywords extracted from the same window.
    pub recent_keywords: Vec<String>,
    /// Evidence pointers backing the wake-up window.
    pub citations: Vec<MemoryCitationRef>,
}

/// The wake-up slice needed to restore minimal thread continuity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryWakeup {
    /// A concise identity summary for the user.
    pub identity_summary: Option<String>,
    /// The currently active world-state summary.
    pub active_state: Option<String>,
    /// Relevant recalled facts for the next turn.
    pub relevant_facts: Vec<String>,
    /// Commitments that remain open after wake-up.
    pub open_commitments: Vec<String>,
    /// Recent topics carried into foreground memory.
    pub recent_topics: Vec<String>,
    /// Recent keywords carried into foreground memory.
    pub recent_keywords: Vec<String>,
    /// Fact identifiers cited by the wake-up payload.
    pub citation_fact_ids: Vec<MemoryFactId>,
    /// Minimal evidence pointers that back the wake-up payload.
    pub citations: Vec<MemoryCitationRef>,
}

/// A summary of the facts changed by a compilation pass.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryCompilationSummary {
    /// A short user-visible summary of the compilation delta.
    pub delta_summary: String,
    /// Fact identifiers that were updated or created.
    pub updated_fact_ids: Vec<MemoryFactId>,
    /// Fact identifiers that were invalidated or superseded.
    pub invalidated_fact_ids: Vec<MemoryFactId>,
    /// Recent topics written back by the compilation pass.
    pub recent_topics: Vec<String>,
    /// Recent keywords written back by the compilation pass.
    pub recent_keywords: Vec<String>,
    /// Reusable procedure candidates surfaced by the compilation pass.
    pub candidate_procedures: Vec<String>,
}

/// The lifecycle state of a topic in semantic recall.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryTopicStatus {
    /// The topic is currently active in the user's working set.
    Active,
    /// The topic appears completed or settled.
    Resolved,
    /// The topic has become outdated or explicitly invalidated.
    Stale,
}

/// A compact topic index entry surfaced to clients.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryTopicSummary {
    /// The primary topic name.
    pub name: String,
    /// Alternate names or aliases that can also recall the topic.
    pub aliases: Vec<String>,
    /// A one-line summary of the topic.
    pub summary: String,
    /// The current lifecycle state of the topic.
    pub status: MemoryTopicStatus,
    /// The last time the topic was touched by foreground compilation.
    pub last_active_at: Option<Timestamp>,
    /// Threads most strongly associated with the topic.
    pub related_thread_ids: Vec<ThreadId>,
    /// Artifact identifiers associated with the topic.
    pub related_artifact_ids: Vec<ArtifactId>,
    /// Fact identifiers associated with the topic.
    pub citation_fact_ids: Vec<MemoryFactId>,
    /// Recent keywords kept with the topic index entry.
    pub recent_keywords: Vec<String>,
}

/// The search mode used for history recall.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemorySearchMode {
    /// Exact-term and keyword-oriented matching.
    Keyword,
    /// Lightweight semantic recall using token-overlap similarity.
    Semantic,
}

/// The stable kind of a memory search hit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemorySearchHitKind {
    /// A topic-index hit.
    Topic,
    /// A session or thread-history hit.
    Session,
    /// A compiled memory-fact hit.
    Fact,
    /// An artifact-backed hit.
    Artifact,
}

/// One hit returned from history recall.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemorySearchHit {
    /// The kind of result that matched.
    pub kind: MemorySearchHitKind,
    /// A concise hit title.
    pub title: String,
    /// A short summary of why the hit matters.
    pub summary: String,
    /// A simple integer score used for ordering results.
    pub score: u32,
    /// The related thread, if any.
    pub thread_id: Option<ThreadId>,
    /// The related turn, if any.
    pub turn_id: Option<TurnId>,
    /// The related artifact, if any.
    pub artifact_id: Option<ArtifactId>,
    /// The related compiled fact, if any.
    pub fact_id: Option<MemoryFactId>,
}

/// A compact turn-log entry surfaced inside a session expansion view.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemorySessionEntry {
    /// The timestamp when the entry was recorded.
    pub recorded_at: Timestamp,
    /// The stable event label recorded in the turn log.
    pub event: String,
    /// The user-visible summary stored for the entry.
    pub summary: String,
    /// The associated turn, if any.
    pub turn_id: Option<TurnId>,
    /// Artifact identifiers referenced by the entry.
    pub artifact_ids: Vec<ArtifactId>,
}

/// A session view expanded from thread history and evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemorySessionView {
    /// The thread that owns the session.
    pub thread_id: ThreadId,
    /// The thread title for the session.
    pub title: String,
    /// The current thread status.
    pub status: ThreadStatus,
    /// The active summary stored on the thread.
    pub active_summary: Option<String>,
    /// Pending commitments still associated with the session.
    pub pending_commitments: Vec<String>,
    /// The recent turn-log entries used to reconstruct the session.
    pub recent_entries: Vec<MemorySessionEntry>,
    /// Artifacts attached to those entries.
    pub artifacts: Vec<ArtifactRef>,
}

/// A minimal evidence expansion view resolved from a citation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryEvidenceView {
    /// The citation that was expanded.
    pub citation: MemoryCitationRef,
    /// The thread title resolved for the citation, if available.
    pub thread_title: Option<String>,
    /// The turn or log summary resolved for the citation, if available.
    pub turn_summary: Option<String>,
    /// The compiled fact identifier resolved for the citation, if any.
    pub fact_id: Option<MemoryFactId>,
    /// The fact kind resolved for the citation, if any.
    pub fact_kind: Option<MemoryFactKind>,
    /// The fact value resolved for the citation, if any.
    pub fact_value: Option<String>,
    /// The artifact resolved for the citation, if any.
    pub artifact: Option<ArtifactRef>,
    /// The artifact body resolved for the citation, if any.
    pub artifact_body: Option<String>,
}
