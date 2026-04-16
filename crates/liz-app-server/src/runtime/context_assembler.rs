//! Context assembly for one turn.

use crate::storage::GlobalMemorySnapshot;
use liz_protocol::{MemoryWakeup, Thread};

/// The retrieval scope chosen for the current turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetrievalScope {
    /// The task should stay tightly scoped to the smallest likely surface.
    Focused,
    /// The task likely needs broader project exploration.
    Expanded,
}

/// A task-local retrieval plan derived from the current input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskLocalRetrieval {
    /// The normalized query terms to use for future workspace lookup.
    pub query_terms: Vec<String>,
    /// Whether the request likely needs a full-repo scan.
    pub requires_full_repo_scan: bool,
}

/// The assembled context envelope produced before model execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssembledContext {
    /// The resident wake-up slice loaded from memory.
    pub wakeup: MemoryWakeup,
    /// The projected thread state injected into the turn.
    pub thread_projection: String,
    /// The task-local retrieval plan for later workspace access.
    pub retrieval: TaskLocalRetrieval,
    /// The retrieval scope chosen by the minimal-diff gate.
    pub scope: RetrievalScope,
    /// The final prompt string handed to the model gateway.
    pub prompt: String,
}

/// Assembles resident, thread, and task-local context for a turn.
#[derive(Debug, Clone, Default)]
pub struct ContextAssembler;

impl ContextAssembler {
    /// Builds an assembled context envelope from memory, thread state, and input.
    pub fn assemble(
        &self,
        snapshot: &GlobalMemorySnapshot,
        thread: &Thread,
        input: &str,
    ) -> AssembledContext {
        let scope = classify_scope(input);
        let retrieval = TaskLocalRetrieval {
            query_terms: derive_query_terms(input),
            requires_full_repo_scan: matches!(scope, RetrievalScope::Expanded),
        };
        let wakeup = MemoryWakeup {
            identity_summary: snapshot.identity_summary.clone(),
            active_state: thread.active_summary.clone(),
            relevant_facts: snapshot
                .facts
                .iter()
                .take(3)
                .map(|fact| format!("{}: {}", fact.subject, fact.value))
                .collect(),
            open_commitments: thread.pending_commitments.clone(),
            citation_fact_ids: snapshot.facts.iter().take(3).map(|fact| fact.id.clone()).collect(),
        };
        let thread_projection = format!(
            "goal: {}\nsummary: {}\ncommitments: {}",
            thread.active_goal.clone().unwrap_or_default(),
            thread.active_summary.clone().unwrap_or_default(),
            thread.pending_commitments.join(" | ")
        );
        let prompt = format!(
            "identity: {}\nthread: {}\nretrieval_scope: {:?}\nretrieval_terms: {}\ninput: {}",
            wakeup.identity_summary.clone().unwrap_or_default(),
            thread_projection,
            scope,
            retrieval.query_terms.join(", "),
            input
        );

        AssembledContext { wakeup, thread_projection, retrieval, scope, prompt }
    }
}

fn classify_scope(input: &str) -> RetrievalScope {
    let lower = input.to_ascii_lowercase();
    let focused_hints = ["only", "just", "small", "single", "one line", "minor"];
    let expanded_hints = ["refactor", "rewrite", "whole repo", "entire", "across files"];

    if expanded_hints.iter().any(|hint| lower.contains(hint)) {
        RetrievalScope::Expanded
    } else if focused_hints.iter().any(|hint| lower.contains(hint)) {
        RetrievalScope::Focused
    } else {
        RetrievalScope::Focused
    }
}

fn derive_query_terms(input: &str) -> Vec<String> {
    input
        .split(|character: char| {
            !character.is_alphanumeric() && character != '_' && character != '.'
        })
        .filter(|term| term.len() >= 3)
        .take(6)
        .map(|term| term.to_ascii_lowercase())
        .collect()
}
