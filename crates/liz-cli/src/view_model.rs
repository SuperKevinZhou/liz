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
    ApprovalRequest, ArtifactKind, MemoryEvidenceView, MemorySearchHit, MemorySessionView,
    MemoryTopicSummary, MemoryWakeup, RecentConversationWakeupView, ResponsePayload,
    ResumeSummary, ServerEvent, ServerEventPayload, ServerResponseEnvelope, Thread, ThreadId,
    ThreadStatus,
};
use std::collections::BTreeMap;

/// The command family currently bound to the input box.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ComposerMode {
    /// Send a normal turn to the selected thread.
    #[default]
    Turn,
    /// Create a new thread from the typed input.
    NewThread,
    /// Search memory with keyword ranking.
    SearchKeyword,
    /// Search memory with semantic ranking.
    SearchSemantic,
}

impl ComposerMode {
    /// Returns the short label shown in the input title.
    pub fn label(self) -> &'static str {
        match self {
            Self::Turn => "turn",
            Self::NewThread => "new-thread",
            Self::SearchKeyword => "search-keyword",
            Self::SearchSemantic => "search-semantic",
        }
    }

    /// Returns a short hint for the active input mode.
    pub fn description(self) -> &'static str {
        match self {
            Self::Turn => "send work to the selected thread",
            Self::NewThread => "create a new thread from this input",
            Self::SearchKeyword => "search memory by exact terms",
            Self::SearchSemantic => "search memory by lightweight semantics",
        }
    }

    /// Rotates to the next composer mode.
    pub fn next(self) -> Self {
        match self {
            Self::Turn => Self::NewThread,
            Self::NewThread => Self::SearchKeyword,
            Self::SearchKeyword => Self::SearchSemantic,
            Self::SearchSemantic => Self::Turn,
        }
    }
}

/// Minimal event-projected view model for the reference CLI.
#[derive(Debug, Clone, Default)]
pub struct ViewModel {
    /// The currently known thread statuses.
    pub thread_statuses: BTreeMap<ThreadId, ThreadStatus>,
    /// The current thread list used by the thread picker.
    pub threads: Vec<Thread>,
    /// The selected thread index inside the picker.
    pub selected_thread_index: usize,
    /// Human-readable transcript lines derived from the event stream.
    pub transcript_lines: Vec<String>,
    /// Approvals currently waiting on user action.
    pub pending_approvals: Vec<ApprovalRequest>,
    /// The latest resume summary returned by the server.
    pub resume_summary: Option<ResumeSummary>,
    /// The currently loaded wake-up slice.
    pub wakeup: Option<MemoryWakeup>,
    /// The currently loaded recent conversation view.
    pub recent_conversation: Option<RecentConversationWakeupView>,
    /// Topics loaded for the topic list surface.
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
    /// The active input mode.
    pub composer_mode: ComposerMode,
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

    /// Applies one server response to the current CLI projection.
    pub fn apply_response(&mut self, response: &ServerResponseEnvelope) {
        match response {
            ServerResponseEnvelope::Success(success) => match &success.response {
                ResponsePayload::ThreadStart(response) => {
                    self.upsert_thread(response.thread.clone());
                    self.status_line = format!("Started thread {}", response.thread.title);
                }
                ResponsePayload::ThreadResume(response) => {
                    self.upsert_thread(response.thread.clone());
                    self.resume_summary = response.resume_summary.clone();
                    self.status_line = format!("Resumed thread {}", response.thread.title);
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
                    self.status_line = format!("Loaded {} threads", self.threads.len());
                }
                ResponsePayload::ThreadFork(response) => {
                    self.upsert_thread(response.thread.clone());
                    self.status_line = format!("Forked thread {}", response.thread.title);
                }
                ResponsePayload::MemoryReadWakeup(response) => {
                    self.wakeup = Some(response.wakeup.clone());
                    self.recent_conversation = Some(response.recent_conversation.clone());
                    self.status_line = format!("Loaded wake-up for {}", response.thread_id);
                }
                ResponsePayload::MemoryListTopics(response) => {
                    self.topics = response.topics.clone();
                    self.status_line = format!("Loaded {} topics", self.topics.len());
                }
                ResponsePayload::MemorySearch(response) => {
                    self.recall_hits = response.hits.clone();
                    self.status_line = format!(
                        "Search returned {} hits for {}",
                        self.recall_hits.len(),
                        response.query
                    );
                }
                ResponsePayload::MemoryOpenSession(response) => {
                    self.session_view = Some(response.session.clone());
                    self.status_line = format!("Opened session {}", response.session.title);
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
                    self.status_line = format!("Expanded evidence {}", response.evidence.citation.note);
                }
                ResponsePayload::MemoryCompileNow(response) => {
                    self.candidate_procedures = response.compilation.candidate_procedures.clone();
                    self.status_line = response.compilation.delta_summary.clone();
                }
                ResponsePayload::ApprovalRespond(response) => {
                    self.pending_approvals.retain(|approval| approval.id != response.approval.id);
                    self.status_line = format!("Resolved approval {}", response.approval.id);
                }
                ResponsePayload::TurnStart(response) => {
                    self.status_line = format!("Turn {} started", response.turn.id);
                }
                ResponsePayload::TurnCancel(response) => {
                    self.status_line = format!("Turn {} cancelled", response.turn.id);
                }
                ResponsePayload::ToolCall(response) => {
                    self.status_line = response.summary.clone();
                }
                _ => {}
            },
            ServerResponseEnvelope::Error(error) => {
                self.transcript_lines.push(format!(
                    "[error] {}: {}",
                    error.error.code, error.error.message
                ));
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
                self.transcript_lines.push(format!(
                    "[{}] turn started: {}",
                    turn.thread_id,
                    turn.goal.clone().unwrap_or_default()
                ));
            }
            ServerEventPayload::TurnCompleted(TurnCompletedEvent { turn }) => {
                self.transcript_lines.push(format!(
                    "[{}] turn completed: {}",
                    turn.thread_id,
                    turn.summary.clone().unwrap_or_default()
                ));
            }
            ServerEventPayload::TurnFailed(TurnFailedEvent { turn, message }) => {
                self.transcript_lines
                    .push(format!("[{}] turn failed: {}", turn.thread_id, message));
            }
            ServerEventPayload::TurnCancelled(TurnCancelledEvent { turn }) => {
                self.transcript_lines.push(format!(
                    "[{}] turn interrupted: {}",
                    turn.thread_id,
                    turn.goal.clone().unwrap_or_default()
                ));
            }
            ServerEventPayload::AssistantChunk(AssistantChunkEvent { chunk, .. }) => {
                let preview = self.assistant_streaming.get_or_insert_with(String::new);
                preview.push_str(chunk);
            }
            ServerEventPayload::AssistantCompleted(AssistantCompletedEvent { message }) => {
                self.assistant_streaming = None;
                self.transcript_lines.push(format!("[assistant] {message}"));
            }
            ServerEventPayload::ToolCallStarted(ToolCallStartedEvent {
                tool_name, summary, ..
            }) => {
                self.transcript_lines
                    .push(format!("[tool] {tool_name} starting: {summary}"));
            }
            ServerEventPayload::ToolCallUpdated(ToolCallUpdatedEvent {
                tool_name,
                delta_summary,
                preview,
                ..
            }) => {
                self.transcript_lines.push(format!(
                    "[tool] {tool_name} updating: {}{}",
                    delta_summary,
                    preview
                        .as_ref()
                        .map(|value| format!(" ({value})"))
                        .unwrap_or_default()
                ));
            }
            ServerEventPayload::ToolCallCommitted(ToolCallCommittedEvent {
                tool_name,
                arguments_summary,
                ..
            }) => {
                self.transcript_lines.push(format!(
                    "[tool] {tool_name} committed: {arguments_summary}"
                ));
            }
            ServerEventPayload::ToolCompleted(ToolCompletedEvent { tool_name, summary, .. }) => {
                self.transcript_lines
                    .push(format!("[tool] {tool_name} completed: {summary}"));
            }
            ServerEventPayload::ToolFailed(ToolFailedEvent { tool_name, summary }) => {
                self.transcript_lines.push(format!("[tool] {tool_name} failed: {summary}"));
            }
            ServerEventPayload::ApprovalRequested(ApprovalRequestedEvent { approval }) => {
                self.pending_approvals.push(approval.clone());
                self.transcript_lines.push(format!(
                    "[approval] {} needs approval: {}",
                    approval.id, approval.reason
                ));
            }
            ServerEventPayload::ApprovalResolved(ApprovalResolvedEvent { approval, decision }) => {
                self.pending_approvals.retain(|pending| pending.id != approval.id);
                self.transcript_lines.push(format!(
                    "[approval] {} resolved as {:?}",
                    approval.id, decision
                ));
            }
            ServerEventPayload::DiffAvailable(DiffAvailableEvent { artifact }) => {
                self.transcript_lines
                    .push(format!("[diff] {} ready: {}", artifact.id, artifact.summary));
            }
            ServerEventPayload::MemoryWakeupLoaded(MemoryWakeupLoadedEvent { wakeup }) => {
                self.wakeup = Some(wakeup.clone());
                self.transcript_lines.push(format!(
                    "[{}] wake-up loaded: {}",
                    event.thread_id,
                    wakeup.recent_topics.join(", ")
                ));
            }
            ServerEventPayload::MemoryCompilationApplied(MemoryCompilationAppliedEvent {
                compilation,
            }) => {
                self.candidate_procedures = compilation.candidate_procedures.clone();
                self.transcript_lines.push(format!(
                    "[{}] memory compiled: {}",
                    event.thread_id, compilation.delta_summary
                ));
            }
            ServerEventPayload::MemoryDreamingCompleted(MemoryDreamingCompletedEvent {
                summary,
            }) => {
                self.dreaming_summaries.push(summary.clone());
                self.transcript_lines
                    .push(format!("[{}] dreaming: {}", event.thread_id, summary));
            }
            _ => {}
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
    }
}
