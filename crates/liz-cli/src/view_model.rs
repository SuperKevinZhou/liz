//! Event-driven CLI view model primitives.

use liz_protocol::events::{
    ApprovalRequestedEvent, ApprovalResolvedEvent, AssistantChunkEvent, AssistantCompletedEvent,
    DiffAvailableEvent, MemoryCompilationAppliedEvent, MemoryDreamingCompletedEvent,
    MemoryWakeupLoadedEvent, ThreadArchivedEvent, ThreadForkedEvent, ThreadInterruptedEvent,
    ThreadResumedEvent, ThreadStartedEvent, ThreadUpdatedEvent, ToolCallCommittedEvent,
    ToolCallStartedEvent, ToolCallUpdatedEvent, ToolCompletedEvent, ToolFailedEvent,
    TurnCancelledEvent, TurnCompletedEvent, TurnFailedEvent, TurnStartedEvent,
};
use liz_protocol::{
    ApprovalRequest, ArtifactKind, MemoryEvidenceView, MemorySearchHit, MemorySessionEntry,
    MemorySessionView, MemoryTopicSummary, MemoryWakeup, RecentConversationWakeupView,
    ResponsePayload, ResumeSummary, ServerEvent, ServerEventPayload, ServerResponseEnvelope,
    Thread, ThreadId, ThreadStatus,
};
use std::collections::BTreeMap;

/// Transcript entry categories surfaced by the chat shell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TranscriptEntryKind {
    /// User-authored input.
    User,
    /// Assistant output produced by the model runtime.
    Assistant,
    /// Tool execution progress or completion output.
    Tool,
    /// Approval request surfaced inline with the conversation.
    Approval,
    /// Runtime status, interruption, memory, or error notes.
    System,
}

impl TranscriptEntryKind {
    /// Returns the label shown in the transcript for this entry kind.
    pub fn label(self) -> &'static str {
        match self {
            Self::User => "you",
            Self::Assistant => "liz",
            Self::Tool => "tool",
            Self::Approval => "approval",
            Self::System => "system",
        }
    }
}

/// A single transcript item rendered in the primary conversation flow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscriptEntry {
    /// The rendering category for the transcript entry.
    pub kind: TranscriptEntryKind,
    /// The user-visible transcript body.
    pub body: String,
}

/// Overlay surfaces that can temporarily take focus without replacing the transcript.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayPanel {
    /// Command and keyboard help.
    Help,
    /// Memory search result overlay.
    Search,
    /// Wake-up, recall, evidence, and compiled-experience overlay.
    Memory,
}

/// Minimal event-projected view model for the CLI.
#[derive(Debug, Clone, Default)]
pub struct ViewModel {
    /// The currently known thread statuses.
    pub thread_statuses: BTreeMap<ThreadId, ThreadStatus>,
    /// The current thread list used by the thread picker.
    pub threads: Vec<Thread>,
    /// The selected thread index inside the picker.
    pub selected_thread_index: usize,
    /// Transcript entries shown in the primary chat flow.
    pub transcript_entries: Vec<TranscriptEntry>,
    /// Transcript entries keyed by their owning thread.
    pub thread_transcripts: BTreeMap<ThreadId, Vec<TranscriptEntry>>,
    /// Approvals currently waiting on user action.
    pub pending_approvals: Vec<ApprovalRequest>,
    /// The latest resume summary returned by the server.
    pub resume_summary: Option<ResumeSummary>,
    /// The currently loaded wake-up slice.
    pub wakeup: Option<MemoryWakeup>,
    /// The currently loaded recent conversation view.
    pub recent_conversation: Option<RecentConversationWakeupView>,
    /// Topics loaded for memory inspection.
    pub topics: Vec<MemoryTopicSummary>,
    /// Recall hits loaded from memory search.
    pub recall_hits: Vec<MemorySearchHit>,
    /// The most recent expanded session view.
    pub session_view: Option<MemorySessionView>,
    /// The most recent expanded evidence view.
    pub evidence_view: Option<MemoryEvidenceView>,
    /// The current diff preview when a diff artifact is expanded.
    pub diff_preview: Option<String>,
    /// Procedure candidates surfaced by compilation.
    pub candidate_procedures: Vec<String>,
    /// Dreaming or reflection summaries surfaced by the runtime.
    pub dreaming_summaries: Vec<String>,
    /// The one-line status bar message.
    pub status_line: String,
    /// The active input buffer.
    pub input_buffer: String,
    /// Whether the thread rail is visible.
    pub show_thread_rail: bool,
    /// The active overlay panel, if any.
    pub active_overlay: Option<OverlayPanel>,
    pending_thread_start_entries: Vec<TranscriptEntry>,
    assistant_streaming: Option<String>,
}

impl ViewModel {
    /// Returns the primary view name surfaced by the CLI banner.
    pub fn primary_view() -> &'static str {
        "transcript"
    }

    /// Returns the selected thread, if one is available.
    pub fn selected_thread(&self) -> Option<&Thread> {
        self.threads.get(self.selected_thread_index)
    }

    /// Returns the selected thread identifier.
    pub fn selected_thread_id(&self) -> Option<ThreadId> {
        self.selected_thread().map(|thread| thread.id.clone())
    }

    /// Returns the in-progress assistant preview, if any.
    pub fn streaming_preview(&self) -> Option<&str> {
        self.assistant_streaming.as_deref()
    }

    /// Returns the number of pending approvals.
    pub fn pending_approval_count(&self) -> usize {
        self.pending_approvals.len()
    }

    /// Returns whether the current thread has wake-up context to surface.
    pub fn has_wakeup_context(&self) -> bool {
        self.wakeup.is_some() || self.recent_conversation.is_some() || self.resume_summary.is_some()
    }

    /// Moves the selection one thread upward.
    pub fn select_previous_thread(&mut self) {
        if self.threads.is_empty() {
            self.selected_thread_index = 0;
            return;
        }
        if self.selected_thread_index == 0 {
            self.selected_thread_index = self.threads.len() - 1;
        } else {
            self.selected_thread_index -= 1;
        }
    }

    /// Moves the selection one thread downward.
    pub fn select_next_thread(&mut self) {
        if self.threads.is_empty() {
            self.selected_thread_index = 0;
            return;
        }
        self.selected_thread_index = (self.selected_thread_index + 1) % self.threads.len();
    }

    /// Toggles the visibility of the thread rail.
    pub fn toggle_thread_rail(&mut self) {
        self.show_thread_rail = !self.show_thread_rail;
    }

    /// Opens an overlay panel.
    pub fn open_overlay(&mut self, panel: OverlayPanel) {
        self.active_overlay = Some(panel);
    }

    /// Closes the active overlay.
    pub fn close_overlay(&mut self) {
        self.active_overlay = None;
    }

    /// Applies one server response to the current CLI projection.
    pub fn apply_response(&mut self, response: &ServerResponseEnvelope) {
        match response {
            ServerResponseEnvelope::Success(success) => match &success.response {
                ResponsePayload::ThreadStart(response) => {
                    self.upsert_thread(response.thread.clone());
                    self.attach_pending_entries_to_thread(&response.thread.id);
                    self.status_line = format!("Started {}", response.thread.title);
                }
                ResponsePayload::ThreadResume(response) => {
                    self.upsert_thread(response.thread.clone());
                    self.resume_summary = response.resume_summary.clone();
                    self.sync_visible_transcript();
                    self.status_line = format!("Resumed {}", response.thread.title);
                }
                ResponsePayload::ThreadList(response) => {
                    let selected_thread_id = self.selected_thread_id();
                    self.threads = response.threads.clone();
                    self.thread_statuses = self
                        .threads
                        .iter()
                        .map(|thread| (thread.id.clone(), thread.status))
                        .collect();
                    self.selected_thread_index = selected_thread_id
                        .and_then(|selected_id| {
                            self.threads.iter().position(|thread| thread.id == selected_id)
                        })
                        .unwrap_or(0);
                    self.sync_visible_transcript();
                    self.status_line = match self.threads.len() {
                        0 => "No threads yet".to_owned(),
                        1 => "Loaded 1 thread".to_owned(),
                        count => format!("Loaded {count} threads"),
                    };
                }
                ResponsePayload::ThreadFork(response) => {
                    self.upsert_thread(response.thread.clone());
                    self.sync_visible_transcript();
                    self.status_line = format!("Forked {}", response.thread.title);
                }
                ResponsePayload::MemoryReadWakeup(response) => {
                    self.wakeup = Some(response.wakeup.clone());
                    self.recent_conversation = Some(response.recent_conversation.clone());
                    self.status_line = format!("Ready to continue {}", response.thread_id);
                }
                ResponsePayload::MemoryListTopics(response) => {
                    self.topics = response.topics.clone();
                    self.status_line = format!("Loaded {} memory topics", self.topics.len());
                }
                ResponsePayload::MemorySearch(response) => {
                    self.recall_hits = response.hits.clone();
                    self.active_overlay = Some(OverlayPanel::Search);
                    self.status_line = format!("Search found {} result(s)", self.recall_hits.len());
                }
                ResponsePayload::MemoryOpenSession(response) => {
                    self.session_view = Some(response.session.clone());
                    self.replace_thread_history(&response.session);
                    self.status_line = format!("Opened {}", response.session.title);
                }
                ResponsePayload::MemoryOpenEvidence(response) => {
                    self.diff_preview = response.evidence.artifact_body.clone().filter(|_body| {
                        response
                            .evidence
                            .artifact
                            .as_ref()
                            .map(|artifact| artifact.kind == ArtifactKind::Diff)
                            .unwrap_or(false)
                    });
                    self.evidence_view = Some(response.evidence.clone());
                    self.status_line =
                        format!("Opened evidence {}", response.evidence.citation.note);
                }
                ResponsePayload::MemoryCompileNow(response) => {
                    self.candidate_procedures = response.compilation.candidate_procedures.clone();
                    self.status_line = response.compilation.delta_summary.clone();
                }
                ResponsePayload::ApprovalRespond(response) => {
                    self.pending_approvals.retain(|approval| approval.id != response.approval.id);
                    self.status_line = format!("Resolved approval {}", response.approval.id);
                }
                ResponsePayload::TurnStart(_) => {
                    self.status_line = "Message sent".to_owned();
                }
                ResponsePayload::TurnCancel(_) => {
                    self.status_line = "Turn cancelled".to_owned();
                }
                ResponsePayload::ToolCall(response) => {
                    self.status_line = response.summary.clone();
                }
                _ => {}
            },
            ServerResponseEnvelope::Error(error) => {
                self.push_entry_for_selected_thread(
                    TranscriptEntryKind::System,
                    error.error.message.clone(),
                );
                self.status_line = error.error.message.clone();
            }
        }
    }

    /// Applies one server event to the current CLI projection.
    pub fn apply_event(&mut self, event: &ServerEvent) {
        match &event.payload {
            ServerEventPayload::ThreadStarted(ThreadStartedEvent { thread })
            | ServerEventPayload::ThreadResumed(ThreadResumedEvent { thread })
            | ServerEventPayload::ThreadUpdated(ThreadUpdatedEvent { thread })
            | ServerEventPayload::ThreadInterrupted(ThreadInterruptedEvent { thread })
            | ServerEventPayload::ThreadForked(ThreadForkedEvent { thread })
            | ServerEventPayload::ThreadArchived(ThreadArchivedEvent { thread }) => {
                self.upsert_thread(thread.clone());
            }
            ServerEventPayload::TurnStarted(TurnStartedEvent { turn }) => {
                if let Some(goal) = turn.goal.as_ref().filter(|goal| !goal.trim().is_empty()) {
                    self.push_thread_entry(
                        event.thread_id.clone(),
                        TranscriptEntryKind::User,
                        goal.clone(),
                    );
                }
                self.status_line = turn
                    .goal
                    .clone()
                    .map(|goal| format!("Working on {goal}"))
                    .unwrap_or_else(|| "liz is working".to_owned());
            }
            ServerEventPayload::TurnCompleted(TurnCompletedEvent { turn }) => {
                self.status_line =
                    turn.summary.clone().unwrap_or_else(|| "Turn completed".to_owned());
            }
            ServerEventPayload::TurnFailed(TurnFailedEvent { message, .. }) => {
                self.push_thread_entry(
                    event.thread_id.clone(),
                    TranscriptEntryKind::System,
                    format!("Turn failed: {message}"),
                );
                self.status_line = message.clone();
            }
            ServerEventPayload::TurnCancelled(TurnCancelledEvent { .. }) => {
                self.push_thread_entry(
                    event.thread_id.clone(),
                    TranscriptEntryKind::System,
                    "Turn interrupted".to_owned(),
                );
                self.status_line = "Turn interrupted".to_owned();
            }
            ServerEventPayload::AssistantChunk(AssistantChunkEvent { chunk, .. }) => {
                let preview = self.assistant_streaming.get_or_insert_with(String::new);
                preview.push_str(chunk);
                self.status_line = "liz is replying".to_owned();
            }
            ServerEventPayload::AssistantCompleted(AssistantCompletedEvent { message }) => {
                self.assistant_streaming = None;
                self.push_thread_entry(
                    event.thread_id.clone(),
                    TranscriptEntryKind::Assistant,
                    message.clone(),
                );
                self.status_line = "Reply finished".to_owned();
            }
            ServerEventPayload::ToolCallStarted(ToolCallStartedEvent {
                tool_name,
                summary,
                ..
            }) => {
                self.status_line = format!("{tool_name}: {summary}");
            }
            ServerEventPayload::ToolCallUpdated(ToolCallUpdatedEvent {
                tool_name,
                delta_summary,
                preview,
                ..
            }) => {
                self.status_line = format!(
                    "{tool_name}: {}{}",
                    delta_summary,
                    preview.as_ref().map(|value| format!(" ({value})")).unwrap_or_default()
                );
            }
            ServerEventPayload::ToolCallCommitted(ToolCallCommittedEvent {
                tool_name,
                arguments_summary,
                ..
            }) => {
                self.status_line = format!("{tool_name}: {arguments_summary}");
            }
            ServerEventPayload::ToolCompleted(ToolCompletedEvent {
                tool_name, summary, ..
            }) => {
                self.push_thread_entry(
                    event.thread_id.clone(),
                    TranscriptEntryKind::Tool,
                    format!("{tool_name}: {summary}"),
                );
                self.status_line = format!("{tool_name} finished");
            }
            ServerEventPayload::ToolFailed(ToolFailedEvent { tool_name, summary }) => {
                self.push_thread_entry(
                    event.thread_id.clone(),
                    TranscriptEntryKind::Tool,
                    format!("{tool_name}: {summary}"),
                );
                self.status_line = format!("{tool_name} failed");
            }
            ServerEventPayload::ApprovalRequested(ApprovalRequestedEvent { approval }) => {
                self.pending_approvals.push(approval.clone());
                self.push_thread_entry(
                    event.thread_id.clone(),
                    TranscriptEntryKind::Approval,
                    format!("{} needs approval: {}", approval.id, approval.reason),
                );
                self.status_line = "Approval needed".to_owned();
            }
            ServerEventPayload::ApprovalResolved(ApprovalResolvedEvent { approval, decision }) => {
                self.pending_approvals.retain(|pending| pending.id != approval.id);
                self.push_thread_entry(
                    event.thread_id.clone(),
                    TranscriptEntryKind::System,
                    format!("Approval {} resolved as {:?}", approval.id, decision),
                );
                self.status_line = format!("Approval {} resolved", approval.id);
            }
            ServerEventPayload::DiffAvailable(DiffAvailableEvent { artifact }) => {
                self.diff_preview = Some(artifact.summary.clone());
                self.status_line = "Diff preview ready".to_owned();
            }
            ServerEventPayload::MemoryWakeupLoaded(MemoryWakeupLoadedEvent { wakeup }) => {
                self.wakeup = Some(wakeup.clone());
                self.status_line = "Wake-up refreshed".to_owned();
            }
            ServerEventPayload::MemoryCompilationApplied(MemoryCompilationAppliedEvent {
                compilation,
            }) => {
                self.candidate_procedures = compilation.candidate_procedures.clone();
                self.push_thread_entry(
                    event.thread_id.clone(),
                    TranscriptEntryKind::System,
                    format!("Memory updated: {}", compilation.delta_summary),
                );
                self.status_line = compilation.delta_summary.clone();
            }
            ServerEventPayload::MemoryDreamingCompleted(MemoryDreamingCompletedEvent {
                summary,
            }) => {
                self.dreaming_summaries.push(summary.clone());
                self.status_line = "Reflection updated".to_owned();
            }
            _ => {}
        }
    }

    /// Adds a user message to the transcript.
    pub fn push_user_message(&mut self, message: String) {
        self.push_entry_for_selected_thread(TranscriptEntryKind::User, message);
    }

    /// Adds the first user message while a new thread request is still in flight.
    pub fn push_pending_thread_start_message(&mut self, message: String) {
        push_deduped_entry(
            &mut self.pending_thread_start_entries,
            TranscriptEntry { kind: TranscriptEntryKind::User, body: message },
        );
        self.transcript_entries = self.pending_thread_start_entries.clone();
    }

    fn push_entry_for_selected_thread(&mut self, kind: TranscriptEntryKind, body: String) {
        if let Some(thread_id) = self.selected_thread_id() {
            self.push_thread_entry(thread_id, kind, body);
        } else {
            push_deduped_entry(&mut self.transcript_entries, TranscriptEntry { kind, body });
        }
    }

    fn push_thread_entry(&mut self, thread_id: ThreadId, kind: TranscriptEntryKind, body: String) {
        let entry = TranscriptEntry { kind, body };
        push_deduped_entry(self.thread_transcripts.entry(thread_id.clone()).or_default(), entry);
        if self.selected_thread_id().as_ref() == Some(&thread_id) {
            self.sync_visible_transcript();
        }
    }

    fn upsert_thread(&mut self, thread: Thread) {
        let selected_thread_id = self.selected_thread_id();
        if let Some(existing) = self.threads.iter_mut().find(|existing| existing.id == thread.id) {
            *existing = thread.clone();
        } else {
            self.threads.push(thread.clone());
        }
        self.thread_statuses.insert(thread.id.clone(), thread.status);
        self.threads.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
        self.selected_thread_index = selected_thread_id
            .or(Some(thread.id))
            .and_then(|selected_id| self.threads.iter().position(|item| item.id == selected_id))
            .unwrap_or(0);
        self.sync_visible_transcript();
    }

    fn sync_visible_transcript(&mut self) {
        self.transcript_entries = self
            .selected_thread_id()
            .and_then(|thread_id| self.thread_transcripts.get(&thread_id).cloned())
            .unwrap_or_default();
    }

    fn attach_pending_entries_to_thread(&mut self, thread_id: &ThreadId) {
        if self.pending_thread_start_entries.is_empty() {
            self.sync_visible_transcript();
            return;
        }

        let transcript = self.thread_transcripts.entry(thread_id.clone()).or_default();
        for entry in self.pending_thread_start_entries.drain(..) {
            push_deduped_entry(transcript, entry);
        }
        self.sync_visible_transcript();
    }

    fn replace_thread_history(&mut self, session: &MemorySessionView) {
        let mut history =
            session.recent_entries.iter().filter_map(entry_from_session).collect::<Vec<_>>();

        if let Some(existing) = self.thread_transcripts.get(&session.thread_id) {
            for entry in existing {
                push_deduped_entry(&mut history, entry.clone());
            }
        }

        self.thread_transcripts.insert(session.thread_id.clone(), history);
        if self.selected_thread_id().as_ref() == Some(&session.thread_id) {
            self.sync_visible_transcript();
        }
    }
}

fn entry_from_session(entry: &MemorySessionEntry) -> Option<TranscriptEntry> {
    let body = entry.summary.trim();
    if body.is_empty() {
        return None;
    }

    let (kind, body) = match entry.event.as_str() {
        "turn_started" => (
            TranscriptEntryKind::User,
            body.strip_prefix("Started turn for: ").unwrap_or(body).to_owned(),
        ),
        "turn_completed" => (TranscriptEntryKind::Assistant, body.to_owned()),
        "tool_completed" => (TranscriptEntryKind::Tool, body.to_owned()),
        "approval_wait" => (TranscriptEntryKind::Approval, body.to_owned()),
        "approval_resolved" => (TranscriptEntryKind::System, body.to_owned()),
        "turn_cancelled" => (TranscriptEntryKind::System, format!("Interrupted: {body}")),
        "turn_failed" => (TranscriptEntryKind::System, format!("Turn failed: {body}")),
        _ => (TranscriptEntryKind::System, body.to_owned()),
    };

    Some(TranscriptEntry { kind, body })
}

fn push_deduped_entry(entries: &mut Vec<TranscriptEntry>, entry: TranscriptEntry) {
    if entry.body.trim().is_empty() {
        return;
    }
    if entries.last() == Some(&entry) {
        return;
    }
    if entries.iter().rev().take(3).any(|existing| existing == &entry) {
        return;
    }
    entries.push(entry);
}
