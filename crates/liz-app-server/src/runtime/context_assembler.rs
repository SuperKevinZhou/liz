//! Context assembly for one turn.

use crate::storage::GlobalMemorySnapshot;
use crate::storage::TurnLogEntry;
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

/// A compact recent-conversation slice assembled from the thread turn log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecentConversationWakeup {
    /// Recent turn-log summaries that help the model recover the active line of work.
    pub recent_summaries: Vec<String>,
    /// Lightweight topic keywords derived from recent conversation summaries.
    pub active_topics: Vec<String>,
    /// Lightweight lexical keywords derived from the same summaries.
    pub recent_keywords: Vec<String>,
}

/// Explicit metadata that keeps executor delegation inside `liz`'s boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutorBoundaryMetadata {
    /// The component that owns durable memory and relationship continuity.
    pub memory_owner: String,
    /// The component that owns approvals and high-risk decisions.
    pub approval_owner: String,
    /// The role external execution is allowed to play for this turn.
    pub executor_role: String,
    /// Whether relationship history is delegated alongside task execution.
    pub relationship_history_shared: bool,
}

/// The layered prompt sections produced for one turn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextLayers {
    /// Resident identity and active-world-model wake-up.
    pub resident: String,
    /// Recent conversation wake-up recovered from the turn log.
    pub recent_conversation: String,
    /// Thread-specific projection for the current task.
    pub thread_projection: String,
    /// Task-local retrieval and user input for this turn.
    pub task_local: String,
    /// Executor-boundary rules that constrain delegation.
    pub executor_boundary: String,
}

/// The assembled context envelope produced before model execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssembledContext {
    /// The resident wake-up slice loaded from memory.
    pub wakeup: MemoryWakeup,
    /// The recent conversation wake-up assembled from thread-local history.
    pub recent_conversation: RecentConversationWakeup,
    /// The projected thread state injected into the turn.
    pub thread_projection: String,
    /// The task-local retrieval plan for later workspace access.
    pub retrieval: TaskLocalRetrieval,
    /// The retrieval scope chosen by the minimal-diff gate.
    pub scope: RetrievalScope,
    /// The explicit context layers handed to the model gateway.
    pub layers: ContextLayers,
    /// Boundary metadata that keeps the executor subordinate to `liz`.
    pub executor_boundary: ExecutorBoundaryMetadata,
    /// The stable system prompt for `liz`'s single-self runtime.
    pub system_prompt: String,
    /// The dynamic runtime-owned operating context for the current turn.
    pub developer_prompt: String,
    /// The user-authored turn input.
    pub user_prompt: String,
    /// The flattened prompt string handed to fallback and diagnostics paths.
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
        recent_entries: &[TurnLogEntry],
        input: &str,
    ) -> AssembledContext {
        let scope = classify_scope(input);
        let retrieval = TaskLocalRetrieval {
            query_terms: derive_query_terms(input),
            requires_full_repo_scan: matches!(scope, RetrievalScope::Expanded),
        };
        let wakeup = MemoryWakeup {
            identity_summary: snapshot.identity_summary.clone(),
            active_state: snapshot
                .active_state_summary
                .clone()
                .or_else(|| thread.active_summary.clone()),
            relevant_facts: snapshot
                .facts
                .iter()
                .filter(|fact| fact.invalidated_at.is_none())
                .take(3)
                .map(|fact| format!("{}: {}", fact.subject, fact.value))
                .collect(),
            open_commitments: thread.pending_commitments.clone(),
            recent_topics: if snapshot.recent_topics.is_empty() {
                Vec::new()
            } else {
                snapshot.recent_topics.clone()
            },
            recent_keywords: if snapshot.recent_keywords.is_empty() {
                Vec::new()
            } else {
                snapshot.recent_keywords.clone()
            },
            citation_fact_ids: snapshot
                .facts
                .iter()
                .filter(|fact| fact.invalidated_at.is_none())
                .take(3)
                .map(|fact| fact.id.clone())
                .collect(),
            citations: snapshot
                .facts
                .iter()
                .filter(|fact| fact.invalidated_at.is_none())
                .flat_map(|fact| fact.citations.iter().cloned())
                .take(3)
                .collect(),
        };
        let recent_summaries = collect_recent_summaries(recent_entries);
        let recent_conversation = RecentConversationWakeup {
            active_topics: derive_recent_topics(&recent_summaries),
            recent_keywords: derive_recent_keywords(&recent_summaries),
            recent_summaries,
        };
        let thread_projection = format!(
            "goal: {}\nsummary: {}\ncommitments: {}",
            thread.active_goal.clone().unwrap_or_default(),
            thread.active_summary.clone().unwrap_or_default(),
            thread.pending_commitments.join(" | ")
        );
        let executor_boundary = ExecutorBoundaryMetadata {
            memory_owner: "liz".to_owned(),
            approval_owner: "liz".to_owned(),
            executor_role: "controlled task executor".to_owned(),
            relationship_history_shared: false,
        };
        let layers = ContextLayers {
            resident: format!(
                "identity_summary: {}\nactive_state: {}\nrelevant_facts: {}\nopen_commitments: {}",
                wakeup.identity_summary.clone().unwrap_or_default(),
                wakeup.active_state.clone().unwrap_or_default(),
                wakeup.relevant_facts.join(" | "),
                wakeup.open_commitments.join(" | ")
            ),
            recent_conversation: format!(
                "recent_summaries: {}\nactive_topics: {}\nrecent_keywords: {}",
                recent_conversation.recent_summaries.join(" | "),
                recent_conversation.active_topics.join(", "),
                recent_conversation.recent_keywords.join(", ")
            ),
            thread_projection: thread_projection.clone(),
            task_local: format!(
                "retrieval_scope: {:?}\nretrieval_terms: {}\nfull_repo_scan: {}\ninput: {}",
                scope,
                retrieval.query_terms.join(", "),
                retrieval.requires_full_repo_scan,
                input
            ),
            executor_boundary: format!(
                "memory_owner: {}\napproval_owner: {}\nexecutor_role: {}\nrelationship_history_shared: {}",
                executor_boundary.memory_owner,
                executor_boundary.approval_owner,
                executor_boundary.executor_role,
                executor_boundary.relationship_history_shared
            ),
        };
        let system_prompt = format!(
            "liz_identity:\n{}\n\nresident_wakeup:\n{}",
            liz_system_constitution(),
            layers.resident
        );
        let developer_prompt = format!(
            "turn_operating_contract:\n{}\n\nrecent_conversation_wakeup:\n{}\n\nthread_projection:\n{}\n\ntask_local:\n{}\n\nexecutor_boundary:\n{}\n\ntooling_surface:\n{}\n\nexecution_boundaries:\n{}",
            liz_turn_operating_contract(),
            layers.recent_conversation,
            layers.thread_projection,
            layers.task_local,
            layers.executor_boundary,
            liz_tool_surface_summary(),
            liz_execution_boundary_contract(),
        );
        let user_prompt = input.to_owned();
        let prompt = format!(
            "system:\n{}\n\ndeveloper:\n{}\n\nuser:\n{}",
            system_prompt, developer_prompt, user_prompt
        );

        AssembledContext {
            wakeup,
            recent_conversation,
            thread_projection,
            retrieval,
            scope,
            layers,
            executor_boundary,
            system_prompt,
            developer_prompt,
            user_prompt,
            prompt,
        }
    }
}

fn liz_system_constitution() -> &'static str {
    concat!(
        "You are liz, a coding-first general-purpose personal agent.",
        "\n",
        "Stay one continuous self across planning, execution, and supportive conversation.",
        "\n",
        "Advance the user's real work while preserving continuity, commitments, and trust.",
        "\n",
        "Use the smallest reliable action, prefer minimal diffs, and never invent work you did not complete or observe.",
        "\n",
        "Tools and external executors are subordinate runtimes. liz keeps memory, approvals, and final responsibility."
    )
}

fn liz_turn_operating_contract() -> &'static str {
    concat!(
        "Treat resident wake-up, recent conversation, thread state, and task-local retrieval as runtime-owned context.",
        "\n",
        "Use them to preserve continuity without turning the turn into a mode switch or a second persona.",
        "\n",
        "Keep exploration proportional to the retrieval scope and stay narrow when the request is small or specific.",
        "\n",
        "Maintain clear executor boundaries, surface uncertainty briefly, and preserve pending commitments when they matter."
    )
}

fn liz_tool_surface_summary() -> String {
    let workspace_root = std::env::current_dir()
        .ok()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| ".".to_owned());
    format!(
        concat!(
            "workspace_root: {workspace_root}\n",
            "provider_tool_aliases: workspace_list, workspace_search, workspace_read, workspace_write_text, workspace_apply_patch, shell_exec, shell_spawn, shell_wait, shell_read_output, shell_terminate\n",
            "canonical_runtime_tools: workspace.list, workspace.search, workspace.read, workspace.write_text, workspace.apply_patch, shell.exec, shell.spawn, shell.wait, shell.read_output, shell.terminate"
        ),
        workspace_root = workspace_root,
    )
}

fn liz_execution_boundary_contract() -> &'static str {
    concat!(
        "Use runtime tools when reading, searching, editing, patching files, or running commands.",
        "\n",
        "Do not claim you cannot modify files: capability is available through runtime tools; approval and sandbox are execution boundaries, not capability absence.",
        "\n",
        "Never claim completion before observing corresponding tool results.",
        "\n",
        "Keep edits minimal, read only the files you need, and run narrow verification after mutations.",
        "\n",
        "Structured tool results are fed back as data; reason from stdout/stderr, exit code, diffs, and changed file signals."
    )
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

fn collect_recent_summaries(entries: &[TurnLogEntry]) -> Vec<String> {
    let mut summaries = entries
        .iter()
        .rev()
        .filter_map(|entry| {
            let trimmed = entry.summary.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_owned())
        })
        .take(4)
        .collect::<Vec<_>>();
    summaries.reverse();
    summaries
}

fn derive_recent_topics(summaries: &[String]) -> Vec<String> {
    use std::collections::BTreeSet;

    let mut seen = BTreeSet::new();
    let mut topics = Vec::new();
    for summary in summaries {
        for token in
            summary.split(|character: char| !character.is_alphanumeric() && character != '_')
        {
            let token = token.to_ascii_lowercase();
            if token.len() < 4 || !seen.insert(token.clone()) {
                continue;
            }
            topics.push(token);
            if topics.len() == 6 {
                return topics;
            }
        }
    }
    topics
}

fn derive_recent_keywords(summaries: &[String]) -> Vec<String> {
    use std::collections::BTreeMap;

    let mut counts = BTreeMap::new();
    for summary in summaries {
        for token in
            summary.split(|character: char| !character.is_alphanumeric() && character != '_')
        {
            let token = token.to_ascii_lowercase();
            if token.len() < 3 {
                continue;
            }
            *counts.entry(token).or_insert(0_u32) += 1;
        }
    }

    let mut ranked = counts.into_iter().collect::<Vec<_>>();
    ranked.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
    ranked.into_iter().take(8).map(|(token, _)| token).collect()
}
