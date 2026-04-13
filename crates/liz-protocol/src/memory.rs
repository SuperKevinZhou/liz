//! Memory-related protocol resources shared by requests, responses, and events.

use crate::ids::MemoryFactId;
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
    /// Fact identifiers cited by the wake-up payload.
    pub citation_fact_ids: Vec<MemoryFactId>,
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
}
