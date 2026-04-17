//! Foreground memory read, compile, and recall flows.

use crate::runtime::error::{RuntimeError, RuntimeResult};
use crate::runtime::ids::IdGenerator;
use crate::runtime::stores::RuntimeStores;
use crate::storage::{GlobalMemorySnapshot, StoredMemoryFact, StoredTopicRecord, TurnLogEntry};
use liz_protocol::{
    ArtifactId, ArtifactRef, MemoryCitationRef, MemoryCompilationSummary, MemoryEvidenceView,
    MemoryFactId, MemoryFactKind, MemorySearchHit, MemorySearchHitKind, MemorySearchMode,
    MemorySessionEntry, MemorySessionView, MemoryTopicStatus, MemoryTopicSummary, MemoryWakeup,
    RecentConversationWakeupView, Thread, ThreadId, ThreadStatus,
};
use std::collections::{BTreeMap, BTreeSet};

const DEFAULT_TOPIC_LIMIT: usize = 12;
const DEFAULT_SEARCH_LIMIT: usize = 8;
const RECENT_SUMMARY_LIMIT: usize = 4;
const RECENT_ENTRY_LIMIT: usize = 8;

/// Handles foreground wake-up, compilation, and recall work.
#[derive(Debug, Clone, Default)]
pub struct ForegroundMemoryEngine;

impl ForegroundMemoryEngine {
    /// Reads the current wake-up slice and recent-conversation view for a thread.
    pub fn read_wakeup(
        &self,
        stores: &RuntimeStores,
        thread_id: &ThreadId,
    ) -> RuntimeResult<(MemoryWakeup, RecentConversationWakeupView)> {
        let thread = stores
            .get_thread(thread_id)?
            .ok_or_else(|| RuntimeError::not_found("thread_not_found", "thread does not exist"))?;
        let snapshot = stores.read_global_memory()?;
        let entries = stores.read_turn_log(thread_id)?;
        let recent_conversation = build_recent_conversation(thread_id, &entries);
        let wakeup = build_memory_wakeup(&snapshot, &thread, &recent_conversation);
        Ok((wakeup, recent_conversation))
    }

    /// Runs a foreground compilation pass for a thread and persists the result.
    pub fn compile_thread(
        &self,
        stores: &RuntimeStores,
        ids: &IdGenerator,
        thread_id: &ThreadId,
    ) -> RuntimeResult<MemoryCompilationSummary> {
        let thread = stores
            .get_thread(thread_id)?
            .ok_or_else(|| RuntimeError::not_found("thread_not_found", "thread does not exist"))?;
        let entries = stores.read_turn_log(thread_id)?;
        let recent_entries = recent_entries(&entries);
        let citations = citations_from_entries(thread_id, &recent_entries);
        let recent_summaries = recent_entries
            .iter()
            .map(|entry| entry.summary.clone())
            .filter(|summary| !summary.trim().is_empty())
            .collect::<Vec<_>>();
        let recent_topics = derive_topics_from_text(
            std::iter::once(thread.title.as_str())
                .chain(thread.active_goal.iter().map(String::as_str))
                .chain(thread.active_summary.iter().map(String::as_str))
                .chain(recent_summaries.iter().map(String::as_str))
                .collect::<Vec<_>>()
                .as_slice(),
            6,
        );
        let recent_keywords = derive_keywords_from_text(
            std::iter::once(thread.title.as_str())
                .chain(thread.active_goal.iter().map(String::as_str))
                .chain(thread.active_summary.iter().map(String::as_str))
                .chain(recent_summaries.iter().map(String::as_str))
                .collect::<Vec<_>>()
                .as_slice(),
            8,
        );
        let candidate_procedures = derive_candidate_procedures(&recent_entries, &recent_topics);

        let mut snapshot = stores.read_global_memory()?;
        let now = ids.now_timestamp();
        snapshot.active_state_summary = thread.active_summary.clone();
        snapshot.recent_topics = recent_topics.clone();
        snapshot.recent_keywords = recent_keywords.clone();

        let mut updated_fact_ids = Vec::new();
        let mut invalidated_fact_ids = Vec::new();

        if let Some(active_summary) = thread.active_summary.as_ref() {
            upsert_fact(
                &mut snapshot,
                ids,
                &mut updated_fact_ids,
                &mut invalidated_fact_ids,
                FactSpec {
                    kind: MemoryFactKind::ActiveState,
                    subject: format!("thread:{}:active_state", thread.id),
                    value: active_summary.clone(),
                    keywords: recent_keywords.clone(),
                    related_thread_ids: vec![thread.id.clone()],
                    citations: citations.clone(),
                    updated_at: now.clone(),
                },
            );
        }

        if let Some(goal) = thread.active_goal.as_ref() {
            upsert_fact(
                &mut snapshot,
                ids,
                &mut updated_fact_ids,
                &mut invalidated_fact_ids,
                FactSpec {
                    kind: MemoryFactKind::Decision,
                    subject: format!("thread:{}:current_goal", thread.id),
                    value: goal.clone(),
                    keywords: recent_keywords.clone(),
                    related_thread_ids: vec![thread.id.clone()],
                    citations: citations.clone(),
                    updated_at: now.clone(),
                },
            );
        }

        sync_commitment_facts(
            &mut snapshot,
            ids,
            &thread,
            &recent_keywords,
            &citations,
            &now,
            &mut updated_fact_ids,
            &mut invalidated_fact_ids,
        );

        for procedure in candidate_procedures.iter().cloned() {
            upsert_fact(
                &mut snapshot,
                ids,
                &mut updated_fact_ids,
                &mut invalidated_fact_ids,
                FactSpec {
                    kind: MemoryFactKind::ProcedureCandidate,
                    subject: format!("thread:{}:procedure:{}", thread.id, slugify(&procedure)),
                    value: procedure,
                    keywords: recent_keywords.clone(),
                    related_thread_ids: vec![thread.id.clone()],
                    citations: citations.clone(),
                    updated_at: now.clone(),
                },
            );
        }

        sync_topic_index(
            &mut snapshot,
            &thread,
            &recent_entries,
            &recent_topics,
            &recent_keywords,
            &updated_fact_ids,
            &citations,
            &now,
        );

        stores.write_global_memory(&snapshot)?;

        Ok(MemoryCompilationSummary {
            delta_summary: format!(
                "Compiled {} topics, {} commitments, and {} procedure candidates",
                recent_topics.len(),
                thread.pending_commitments.len(),
                candidate_procedures.len()
            ),
            updated_fact_ids,
            invalidated_fact_ids,
            recent_topics,
            recent_keywords,
            candidate_procedures,
        })
    }

    /// Lists topic summaries from the topic index.
    pub fn list_topics(
        &self,
        stores: &RuntimeStores,
        status: Option<MemoryTopicStatus>,
        limit: Option<usize>,
    ) -> RuntimeResult<Vec<MemoryTopicSummary>> {
        let mut topics = stores
            .read_global_memory()?
            .topic_index
            .into_iter()
            .filter(|topic| status.map(|value| topic.status == value).unwrap_or(true))
            .map(topic_summary_from_record)
            .collect::<Vec<_>>();
        topics.sort_by(|left, right| right.last_active_at.cmp(&left.last_active_at));
        topics.truncate(limit.unwrap_or(DEFAULT_TOPIC_LIMIT));
        Ok(topics)
    }

    /// Searches topic, fact, and session memory using a recall mode.
    pub fn search(
        &self,
        stores: &RuntimeStores,
        query: &str,
        mode: MemorySearchMode,
        limit: Option<usize>,
    ) -> RuntimeResult<Vec<MemorySearchHit>> {
        let snapshot = stores.read_global_memory()?;
        let query_tokens = normalize_tokens(query);
        if query_tokens.is_empty() {
            return Ok(Vec::new());
        }

        let mut hits = Vec::new();
        for topic in snapshot.topic_index.iter() {
            let haystack = format!(
                "{} {} {} {}",
                topic.name,
                topic.aliases.join(" "),
                topic.summary,
                topic.recent_keywords.join(" ")
            );
            let score = score_match(&query_tokens, &haystack, mode);
            if score == 0 {
                continue;
            }
            hits.push(MemorySearchHit {
                kind: MemorySearchHitKind::Topic,
                title: topic.name.clone(),
                summary: topic.summary.clone(),
                score,
                thread_id: topic.related_thread_ids.first().cloned(),
                turn_id: topic.citations.first().and_then(|citation| citation.turn_id.clone()),
                artifact_id: topic.related_artifact_ids.first().cloned(),
                fact_id: topic.citation_fact_ids.first().cloned(),
            });
        }

        for fact in snapshot
            .facts
            .iter()
            .filter(|fact| fact.invalidated_at.is_none() && fact.invalidated_by.is_none())
        {
            let haystack = format!("{} {} {}", fact.subject, fact.value, fact.keywords.join(" "));
            let score = score_match(&query_tokens, &haystack, mode);
            if score == 0 {
                continue;
            }
            hits.push(MemorySearchHit {
                kind: MemorySearchHitKind::Fact,
                title: fact.subject.clone(),
                summary: fact.value.clone(),
                score,
                thread_id: fact.related_thread_ids.first().cloned(),
                turn_id: fact.citations.first().and_then(|citation| citation.turn_id.clone()),
                artifact_id: fact.citations.first().and_then(|citation| citation.artifact_id.clone()),
                fact_id: Some(fact.id.clone()),
            });
        }

        for thread in stores.list_threads()? {
            let entries = stores.read_turn_log(&thread.id)?;
            let recent = recent_entries(&entries);
            let summaries = recent
                .iter()
                .map(|entry| entry.summary.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            let haystack = format!(
                "{} {} {} {}",
                thread.title,
                thread.active_goal.clone().unwrap_or_default(),
                thread.active_summary.clone().unwrap_or_default(),
                summaries
            );
            let score = score_match(&query_tokens, &haystack, mode);
            if score == 0 {
                continue;
            }
            hits.push(MemorySearchHit {
                kind: MemorySearchHitKind::Session,
                title: thread.title.clone(),
                summary: thread.active_summary.clone().unwrap_or_else(|| {
                    recent
                        .last()
                        .map(|entry| entry.summary.clone())
                        .unwrap_or_else(|| "No session summary available".to_owned())
                }),
                score,
                thread_id: Some(thread.id),
                turn_id: recent.last().and_then(|entry| entry.turn_id.clone()),
                artifact_id: recent.last().and_then(|entry| entry.artifact_ids.first().cloned()),
                fact_id: None,
            });
        }

        hits.sort_by(|left, right| {
            right
                .score
                .cmp(&left.score)
                .then_with(|| left.title.cmp(&right.title))
        });
        hits.truncate(limit.unwrap_or(DEFAULT_SEARCH_LIMIT));
        Ok(hits)
    }

    /// Expands one thread session into recent entries and artifacts.
    pub fn open_session(
        &self,
        stores: &RuntimeStores,
        thread_id: &ThreadId,
    ) -> RuntimeResult<MemorySessionView> {
        let thread = stores
            .get_thread(thread_id)?
            .ok_or_else(|| RuntimeError::not_found("thread_not_found", "thread does not exist"))?;
        let entries = recent_entries(&stores.read_turn_log(thread_id)?);
        let artifacts = artifacts_for_entries(stores, &entries)?;

        Ok(MemorySessionView {
            thread_id: thread.id,
            title: thread.title,
            status: thread.status,
            active_summary: thread.active_summary,
            pending_commitments: thread.pending_commitments,
            recent_entries: entries
                .into_iter()
                .map(|entry| MemorySessionEntry {
                    recorded_at: entry.recorded_at,
                    event: entry.event,
                    summary: entry.summary,
                    turn_id: entry.turn_id,
                    artifact_ids: entry.artifact_ids,
                })
                .collect(),
            artifacts,
        })
    }

    /// Expands one piece of memory evidence into a raw view.
    pub fn open_evidence(
        &self,
        stores: &RuntimeStores,
        thread_id: &ThreadId,
        turn_id: Option<&liz_protocol::TurnId>,
        artifact_id: Option<&ArtifactId>,
        fact_id: Option<&MemoryFactId>,
    ) -> RuntimeResult<MemoryEvidenceView> {
        if turn_id.is_none() && artifact_id.is_none() && fact_id.is_none() {
            return Err(RuntimeError::invalid_state(
                "memory_evidence_target_required",
                "memory/open_evidence requires a turn, artifact, or fact target",
            ));
        }

        let thread = stores
            .get_thread(thread_id)?
            .ok_or_else(|| RuntimeError::not_found("thread_not_found", "thread does not exist"))?;
        let snapshot = stores.read_global_memory()?;
        let entries = stores.read_turn_log(thread_id)?;
        let turn_summary = turn_id.and_then(|needle| {
            entries
                .iter()
                .find(|entry| entry.turn_id.as_ref() == Some(needle))
                .map(|entry| entry.summary.clone())
        });
        let fact = fact_id.and_then(|needle| snapshot.facts.iter().find(|fact| &fact.id == needle));
        let artifact = match artifact_id {
            Some(needle) => stores.get_artifact(needle)?,
            None => None,
        };
        let citation = if let Some(artifact) = artifact.as_ref() {
            MemoryCitationRef {
                thread_id: thread.id.clone(),
                turn_id: Some(artifact.reference.turn_id.clone()),
                artifact_id: Some(artifact.reference.id.clone()),
                note: artifact.reference.summary.clone(),
            }
        } else if let Some(fact) = fact {
            fact.citations.first().cloned().unwrap_or(MemoryCitationRef {
                thread_id: thread.id.clone(),
                turn_id: turn_id.cloned(),
                artifact_id: None,
                note: fact.subject.clone(),
            })
        } else {
            MemoryCitationRef {
                thread_id: thread.id.clone(),
                turn_id: turn_id.cloned(),
                artifact_id: artifact_id.cloned(),
                note: turn_summary.clone().unwrap_or_else(|| "Session evidence".to_owned()),
            }
        };

        Ok(MemoryEvidenceView {
            citation,
            thread_title: Some(thread.title),
            turn_summary,
            fact_id: fact.map(|fact| fact.id.clone()),
            fact_kind: fact.map(|fact| fact.kind),
            fact_value: fact.map(|fact| fact.value.clone()),
            artifact: artifact.as_ref().map(|artifact| artifact.reference.clone()),
            artifact_body: artifact.map(|artifact| artifact.body),
        })
    }
}

#[derive(Debug, Clone)]
struct FactSpec {
    kind: MemoryFactKind,
    subject: String,
    value: String,
    keywords: Vec<String>,
    related_thread_ids: Vec<ThreadId>,
    citations: Vec<MemoryCitationRef>,
    updated_at: liz_protocol::Timestamp,
}

fn build_memory_wakeup(
    snapshot: &GlobalMemorySnapshot,
    thread: &Thread,
    recent_conversation: &RecentConversationWakeupView,
) -> MemoryWakeup {
    let relevant_facts = snapshot
        .facts
        .iter()
        .filter(|fact| fact.invalidated_at.is_none() && fact.invalidated_by.is_none())
        .filter(|fact| fact.related_thread_ids.is_empty() || fact.related_thread_ids.contains(&thread.id))
        .take(6)
        .map(|fact| format!("{}: {}", fact.subject, fact.value))
        .collect::<Vec<_>>();
    let citation_fact_ids = snapshot
        .facts
        .iter()
        .filter(|fact| fact.invalidated_at.is_none() && fact.invalidated_by.is_none())
        .filter(|fact| fact.related_thread_ids.is_empty() || fact.related_thread_ids.contains(&thread.id))
        .take(6)
        .map(|fact| fact.id.clone())
        .collect::<Vec<_>>();

    MemoryWakeup {
        identity_summary: snapshot.identity_summary.clone(),
        active_state: snapshot
            .active_state_summary
            .clone()
            .or_else(|| thread.active_summary.clone()),
        relevant_facts,
        open_commitments: thread.pending_commitments.clone(),
        recent_topics: if snapshot.recent_topics.is_empty() {
            recent_conversation.active_topics.clone()
        } else {
            snapshot.recent_topics.clone()
        },
        recent_keywords: if snapshot.recent_keywords.is_empty() {
            recent_conversation.recent_keywords.clone()
        } else {
            snapshot.recent_keywords.clone()
        },
        citation_fact_ids,
        citations: recent_conversation.citations.clone(),
    }
}

fn build_recent_conversation(
    thread_id: &ThreadId,
    entries: &[TurnLogEntry],
) -> RecentConversationWakeupView {
    let recent_entries = recent_entries(entries);
    let recent_summaries = recent_entries
        .iter()
        .map(|entry| entry.summary.clone())
        .filter(|summary| !summary.trim().is_empty())
        .collect::<Vec<_>>();
    let active_topics =
        derive_topics_from_text(&recent_summaries.iter().map(String::as_str).collect::<Vec<_>>(), 6);
    let recent_keywords = derive_keywords_from_text(
        &recent_summaries.iter().map(String::as_str).collect::<Vec<_>>(),
        8,
    );

    RecentConversationWakeupView {
        recent_summaries,
        active_topics,
        recent_keywords,
        citations: citations_from_entries(thread_id, &recent_entries),
    }
}

fn sync_commitment_facts(
    snapshot: &mut GlobalMemorySnapshot,
    ids: &IdGenerator,
    thread: &Thread,
    recent_keywords: &[String],
    citations: &[MemoryCitationRef],
    now: &liz_protocol::Timestamp,
    updated_fact_ids: &mut Vec<MemoryFactId>,
    invalidated_fact_ids: &mut Vec<MemoryFactId>,
) {
    let active_subjects = thread
        .pending_commitments
        .iter()
        .map(|commitment| format!("thread:{}:commitment:{}", thread.id, slugify(commitment)))
        .collect::<BTreeSet<_>>();

    for fact in snapshot.facts.iter_mut() {
        if fact.kind != MemoryFactKind::Commitment {
            continue;
        }
        if !fact.related_thread_ids.contains(&thread.id) {
            continue;
        }
        if active_subjects.contains(&fact.subject) || fact.invalidated_at.is_some() {
            continue;
        }
        fact.invalidated_at = Some(now.clone());
        fact.invalidated_by = Some(ids.next_memory_fact_id());
        invalidated_fact_ids.push(fact.id.clone());
    }

    for commitment in thread.pending_commitments.iter().cloned() {
        upsert_fact(
            snapshot,
            ids,
            updated_fact_ids,
            invalidated_fact_ids,
            FactSpec {
                kind: MemoryFactKind::Commitment,
                subject: format!("thread:{}:commitment:{}", thread.id, slugify(&commitment)),
                value: commitment,
                keywords: recent_keywords.to_vec(),
                related_thread_ids: vec![thread.id.clone()],
                citations: citations.to_vec(),
                updated_at: now.clone(),
            },
        );
    }
}

fn sync_topic_index(
    snapshot: &mut GlobalMemorySnapshot,
    thread: &Thread,
    recent_entries: &[TurnLogEntry],
    recent_topics: &[String],
    recent_keywords: &[String],
    updated_fact_ids: &[MemoryFactId],
    citations: &[MemoryCitationRef],
    now: &liz_protocol::Timestamp,
) {
    let related_artifact_ids = recent_entries
        .iter()
        .flat_map(|entry| entry.artifact_ids.iter().cloned())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let status = match thread.status {
        ThreadStatus::Completed => MemoryTopicStatus::Resolved,
        ThreadStatus::Failed | ThreadStatus::Archived => MemoryTopicStatus::Stale,
        _ => MemoryTopicStatus::Active,
    };

    for topic_name in recent_topics.iter().cloned() {
        let summary = thread
            .active_summary
            .clone()
            .unwrap_or_else(|| format!("Recent topic around {topic_name}"));
        if let Some(topic) = snapshot
            .topic_index
            .iter_mut()
            .find(|topic| topic.name == topic_name || topic.aliases.iter().any(|alias| alias == &topic_name))
        {
            topic.summary = summary.clone();
            topic.status = status;
            topic.last_active_at = now.clone();
            merge_into_vec(&mut topic.related_thread_ids, std::iter::once(thread.id.clone()));
            merge_into_vec(&mut topic.related_artifact_ids, related_artifact_ids.iter().cloned());
            merge_into_vec(&mut topic.citation_fact_ids, updated_fact_ids.iter().cloned());
            topic.recent_keywords = recent_keywords.to_vec();
            merge_into_vec(&mut topic.citations, citations.iter().cloned());
            continue;
        }

        snapshot.topic_index.push(StoredTopicRecord {
            name: topic_name,
            aliases: Vec::new(),
            summary,
            status,
            last_active_at: now.clone(),
            related_thread_ids: vec![thread.id.clone()],
            related_artifact_ids: related_artifact_ids.clone(),
            citation_fact_ids: updated_fact_ids.to_vec(),
            recent_keywords: recent_keywords.to_vec(),
            citations: citations.to_vec(),
        });
    }
}

fn upsert_fact(
    snapshot: &mut GlobalMemorySnapshot,
    ids: &IdGenerator,
    updated_fact_ids: &mut Vec<MemoryFactId>,
    invalidated_fact_ids: &mut Vec<MemoryFactId>,
    spec: FactSpec,
) {
    if let Some(existing) = snapshot
        .facts
        .iter_mut()
        .find(|fact| fact.subject == spec.subject && fact.invalidated_at.is_none())
    {
        if existing.value == spec.value && existing.kind == spec.kind {
            existing.keywords = spec.keywords;
            existing.related_thread_ids = spec.related_thread_ids;
            existing.citations = spec.citations;
            existing.updated_at = spec.updated_at;
            updated_fact_ids.push(existing.id.clone());
            return;
        }

        let replacement_id = ids.next_memory_fact_id();
        existing.invalidated_at = Some(spec.updated_at.clone());
        existing.invalidated_by = Some(replacement_id.clone());
        invalidated_fact_ids.push(existing.id.clone());

        snapshot.facts.push(StoredMemoryFact {
            id: replacement_id.clone(),
            kind: spec.kind,
            subject: spec.subject,
            value: spec.value,
            keywords: spec.keywords,
            related_thread_ids: spec.related_thread_ids,
            citations: spec.citations,
            updated_at: spec.updated_at,
            invalidated_at: None,
            invalidated_by: None,
        });
        updated_fact_ids.push(replacement_id);
        return;
    }

    let id = ids.next_memory_fact_id();
    snapshot.facts.push(StoredMemoryFact {
        id: id.clone(),
        kind: spec.kind,
        subject: spec.subject,
        value: spec.value,
        keywords: spec.keywords,
        related_thread_ids: spec.related_thread_ids,
        citations: spec.citations,
        updated_at: spec.updated_at,
        invalidated_at: None,
        invalidated_by: None,
    });
    updated_fact_ids.push(id);
}

fn recent_entries(entries: &[TurnLogEntry]) -> Vec<TurnLogEntry> {
    let mut slice = entries
        .iter()
        .rev()
        .filter(|entry| !entry.summary.trim().is_empty())
        .take(RECENT_ENTRY_LIMIT)
        .cloned()
        .collect::<Vec<_>>();
    slice.reverse();
    slice
}

fn citations_from_entries(thread_id: &ThreadId, entries: &[TurnLogEntry]) -> Vec<MemoryCitationRef> {
    entries
        .iter()
        .take(RECENT_SUMMARY_LIMIT)
        .map(|entry| MemoryCitationRef {
            thread_id: thread_id.clone(),
            turn_id: entry.turn_id.clone(),
            artifact_id: entry.artifact_ids.first().cloned(),
            note: entry.summary.clone(),
        })
        .collect()
}

fn artifacts_for_entries(
    stores: &RuntimeStores,
    entries: &[TurnLogEntry],
) -> RuntimeResult<Vec<ArtifactRef>> {
    let mut artifacts = Vec::new();
    for artifact_id in entries.iter().flat_map(|entry| entry.artifact_ids.iter()) {
        if let Some(artifact) = stores.get_artifact(artifact_id)? {
            artifacts.push(artifact.reference);
        }
    }
    Ok(artifacts)
}

fn topic_summary_from_record(topic: StoredTopicRecord) -> MemoryTopicSummary {
    MemoryTopicSummary {
        name: topic.name,
        aliases: topic.aliases,
        summary: topic.summary,
        status: topic.status,
        last_active_at: Some(topic.last_active_at),
        related_thread_ids: topic.related_thread_ids,
        related_artifact_ids: topic.related_artifact_ids,
        citation_fact_ids: topic.citation_fact_ids,
        recent_keywords: topic.recent_keywords,
    }
}

fn derive_candidate_procedures(entries: &[TurnLogEntry], topics: &[String]) -> Vec<String> {
    let tool_steps = entries
        .iter()
        .filter(|entry| entry.event == "tool_completed")
        .map(|entry| entry.summary.clone())
        .collect::<Vec<_>>();
    if tool_steps.len() < 2 {
        return Vec::new();
    }

    let topic_prefix = topics.first().map(|topic| format!("For {topic}, ")).unwrap_or_default();
    vec![format!("{topic_prefix}repeat the proven flow: {}", tool_steps.join(" -> "))]
}

fn derive_topics_from_text(texts: &[&str], limit: usize) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut topics = Vec::new();
    for token in texts.iter().flat_map(|text| normalize_tokens(text)) {
        if token.len() < 4 || !seen.insert(token.clone()) {
            continue;
        }
        topics.push(token);
        if topics.len() == limit {
            break;
        }
    }
    topics
}

fn derive_keywords_from_text(texts: &[&str], limit: usize) -> Vec<String> {
    let mut counts = BTreeMap::<String, u32>::new();
    for token in texts.iter().flat_map(|text| normalize_tokens(text)) {
        *counts.entry(token).or_default() += 1;
    }

    let mut ranked = counts.into_iter().collect::<Vec<_>>();
    ranked.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
    ranked.into_iter().take(limit).map(|(token, _)| token).collect()
}

fn normalize_tokens(text: &str) -> Vec<String> {
    const STOP_WORDS: &[&str] = &[
        "the", "and", "that", "with", "from", "this", "have", "into", "your", "were", "while",
        "after", "before", "only", "just", "there", "their", "about", "would", "could", "should",
        "thread", "turn", "work", "task", "then", "when", "what",
    ];

    text
        .split(|character: char| !character.is_alphanumeric() && character != '_')
        .filter_map(|token| {
            let normalized = token.to_ascii_lowercase();
            (!normalized.is_empty()
                && normalized.len() >= 3
                && !STOP_WORDS.contains(&normalized.as_str()))
            .then_some(normalized)
        })
        .collect()
}

fn score_match(query_tokens: &[String], haystack: &str, mode: MemorySearchMode) -> u32 {
    let haystack_tokens = normalize_tokens(haystack);
    if haystack_tokens.is_empty() {
        return 0;
    }
    let haystack_set = haystack_tokens.into_iter().collect::<BTreeSet<_>>();
    let shared = query_tokens.iter().filter(|token| haystack_set.contains(*token)).count() as u32;
    if shared == 0 {
        return 0;
    }

    match mode {
        MemorySearchMode::Keyword => shared * 100,
        MemorySearchMode::Semantic => {
            let total = query_tokens.len().max(haystack_set.len()) as u32;
            shared * 100 + (shared * 100 / total.max(1))
        }
    }
}

fn merge_into_vec<T, I>(target: &mut Vec<T>, values: I)
where
    T: Clone + Ord,
    I: IntoIterator<Item = T>,
{
    let mut merged = target.iter().cloned().collect::<BTreeSet<_>>();
    for value in values {
        merged.insert(value);
    }
    *target = merged.into_iter().collect();
}

fn slugify(text: &str) -> String {
    let tokens = normalize_tokens(text);
    if tokens.is_empty() {
        return "item".to_owned();
    }
    tokens.into_iter().take(6).collect::<Vec<_>>().join("_")
}
