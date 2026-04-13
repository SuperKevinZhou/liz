//! Server event envelopes and event payloads.

use crate::approval::{ApprovalDecision, ApprovalRequest};
use crate::artifact::ArtifactRef;
use crate::checkpoint::Checkpoint;
use crate::ids::{ArtifactId, EventId, ExecutorTaskId, ThreadId, TurnId};
use crate::memory::{MemoryCompilationSummary, MemoryWakeup};
use crate::primitives::{RiskLevel, Timestamp};
use crate::thread::Thread;
use crate::turn::Turn;
use serde::{Deserialize, Serialize};

/// The stream a chunk originated from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutorStream {
    /// Standard output.
    Stdout,
    /// Standard error.
    Stderr,
}

/// The stable event type classification for a server event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventType {
    /// A thread was started.
    ThreadStarted,
    /// A thread was resumed.
    ThreadResumed,
    /// A thread was forked.
    ThreadForked,
    /// A thread summary was updated.
    ThreadUpdated,
    /// A thread was interrupted.
    ThreadInterrupted,
    /// A thread was archived.
    ThreadArchived,
    /// A turn was started.
    TurnStarted,
    /// A turn completed successfully.
    TurnCompleted,
    /// A turn failed.
    TurnFailed,
    /// A turn was cancelled.
    TurnCancelled,
    /// Assistant text streamed in.
    AssistantChunk,
    /// An assistant response completed.
    AssistantCompleted,
    /// A tool call started forming.
    ToolCallStarted,
    /// A tool call was updated while forming.
    ToolCallUpdated,
    /// A tool call became executable.
    ToolCallCommitted,
    /// A tool finished successfully.
    ToolCompleted,
    /// A tool failed.
    ToolFailed,
    /// An executor output chunk was emitted.
    ExecutorOutputChunk,
    /// An approval was requested.
    ApprovalRequested,
    /// An approval was resolved.
    ApprovalResolved,
    /// A generic artifact was created.
    ArtifactCreated,
    /// A diff artifact became available.
    DiffAvailable,
    /// A checkpoint was created.
    CheckpointCreated,
    /// A memory wake-up payload was loaded.
    MemoryWakeupLoaded,
    /// A memory compilation delta was applied.
    MemoryCompilationApplied,
    /// A memory invalidation delta was applied.
    MemoryInvalidationApplied,
    /// A dreaming pass completed.
    MemoryDreamingCompleted,
}

/// The top-level server event envelope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerEvent {
    /// The event identifier used for ordering and de-duplication.
    pub event_id: EventId,
    /// The thread the event belongs to.
    pub thread_id: ThreadId,
    /// The turn the event belongs to, if any.
    pub turn_id: Option<TurnId>,
    /// The timestamp when the event was emitted.
    pub created_at: Timestamp,
    /// The typed event payload.
    #[serde(flatten)]
    pub payload: ServerEventPayload,
}

impl ServerEvent {
    /// Returns the stable event type for the payload.
    pub fn event_type(&self) -> EventType {
        self.payload.event_type()
    }
}

/// The typed event payloads supported by protocol v0.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event_type", content = "payload")]
pub enum ServerEventPayload {
    /// `thread_started`
    #[serde(rename = "thread_started")]
    ThreadStarted(ThreadStartedEvent),
    /// `thread_resumed`
    #[serde(rename = "thread_resumed")]
    ThreadResumed(ThreadResumedEvent),
    /// `thread_forked`
    #[serde(rename = "thread_forked")]
    ThreadForked(ThreadForkedEvent),
    /// `thread_updated`
    #[serde(rename = "thread_updated")]
    ThreadUpdated(ThreadUpdatedEvent),
    /// `thread_interrupted`
    #[serde(rename = "thread_interrupted")]
    ThreadInterrupted(ThreadInterruptedEvent),
    /// `thread_archived`
    #[serde(rename = "thread_archived")]
    ThreadArchived(ThreadArchivedEvent),
    /// `turn_started`
    #[serde(rename = "turn_started")]
    TurnStarted(TurnStartedEvent),
    /// `turn_completed`
    #[serde(rename = "turn_completed")]
    TurnCompleted(TurnCompletedEvent),
    /// `turn_failed`
    #[serde(rename = "turn_failed")]
    TurnFailed(TurnFailedEvent),
    /// `turn_cancelled`
    #[serde(rename = "turn_cancelled")]
    TurnCancelled(TurnCancelledEvent),
    /// `assistant_chunk`
    #[serde(rename = "assistant_chunk")]
    AssistantChunk(AssistantChunkEvent),
    /// `assistant_completed`
    #[serde(rename = "assistant_completed")]
    AssistantCompleted(AssistantCompletedEvent),
    /// `tool_call_started`
    #[serde(rename = "tool_call_started")]
    ToolCallStarted(ToolCallStartedEvent),
    /// `tool_call_updated`
    #[serde(rename = "tool_call_updated")]
    ToolCallUpdated(ToolCallUpdatedEvent),
    /// `tool_call_committed`
    #[serde(rename = "tool_call_committed")]
    ToolCallCommitted(ToolCallCommittedEvent),
    /// `tool_completed`
    #[serde(rename = "tool_completed")]
    ToolCompleted(ToolCompletedEvent),
    /// `tool_failed`
    #[serde(rename = "tool_failed")]
    ToolFailed(ToolFailedEvent),
    /// `executor_output_chunk`
    #[serde(rename = "executor_output_chunk")]
    ExecutorOutputChunk(ExecutorOutputChunkEvent),
    /// `approval_requested`
    #[serde(rename = "approval_requested")]
    ApprovalRequested(ApprovalRequestedEvent),
    /// `approval_resolved`
    #[serde(rename = "approval_resolved")]
    ApprovalResolved(ApprovalResolvedEvent),
    /// `artifact_created`
    #[serde(rename = "artifact_created")]
    ArtifactCreated(ArtifactCreatedEvent),
    /// `diff_available`
    #[serde(rename = "diff_available")]
    DiffAvailable(DiffAvailableEvent),
    /// `checkpoint_created`
    #[serde(rename = "checkpoint_created")]
    CheckpointCreated(CheckpointCreatedEvent),
    /// `memory_wakeup_loaded`
    #[serde(rename = "memory_wakeup_loaded")]
    MemoryWakeupLoaded(MemoryWakeupLoadedEvent),
    /// `memory_compilation_applied`
    #[serde(rename = "memory_compilation_applied")]
    MemoryCompilationApplied(MemoryCompilationAppliedEvent),
    /// `memory_invalidation_applied`
    #[serde(rename = "memory_invalidation_applied")]
    MemoryInvalidationApplied(MemoryInvalidationAppliedEvent),
    /// `memory_dreaming_completed`
    #[serde(rename = "memory_dreaming_completed")]
    MemoryDreamingCompleted(MemoryDreamingCompletedEvent),
}

impl ServerEventPayload {
    /// Returns the stable event type for this payload.
    pub fn event_type(&self) -> EventType {
        match self {
            Self::ThreadStarted(_) => EventType::ThreadStarted,
            Self::ThreadResumed(_) => EventType::ThreadResumed,
            Self::ThreadForked(_) => EventType::ThreadForked,
            Self::ThreadUpdated(_) => EventType::ThreadUpdated,
            Self::ThreadInterrupted(_) => EventType::ThreadInterrupted,
            Self::ThreadArchived(_) => EventType::ThreadArchived,
            Self::TurnStarted(_) => EventType::TurnStarted,
            Self::TurnCompleted(_) => EventType::TurnCompleted,
            Self::TurnFailed(_) => EventType::TurnFailed,
            Self::TurnCancelled(_) => EventType::TurnCancelled,
            Self::AssistantChunk(_) => EventType::AssistantChunk,
            Self::AssistantCompleted(_) => EventType::AssistantCompleted,
            Self::ToolCallStarted(_) => EventType::ToolCallStarted,
            Self::ToolCallUpdated(_) => EventType::ToolCallUpdated,
            Self::ToolCallCommitted(_) => EventType::ToolCallCommitted,
            Self::ToolCompleted(_) => EventType::ToolCompleted,
            Self::ToolFailed(_) => EventType::ToolFailed,
            Self::ExecutorOutputChunk(_) => EventType::ExecutorOutputChunk,
            Self::ApprovalRequested(_) => EventType::ApprovalRequested,
            Self::ApprovalResolved(_) => EventType::ApprovalResolved,
            Self::ArtifactCreated(_) => EventType::ArtifactCreated,
            Self::DiffAvailable(_) => EventType::DiffAvailable,
            Self::CheckpointCreated(_) => EventType::CheckpointCreated,
            Self::MemoryWakeupLoaded(_) => EventType::MemoryWakeupLoaded,
            Self::MemoryCompilationApplied(_) => EventType::MemoryCompilationApplied,
            Self::MemoryInvalidationApplied(_) => EventType::MemoryInvalidationApplied,
            Self::MemoryDreamingCompleted(_) => EventType::MemoryDreamingCompleted,
        }
    }
}

/// Payload for `thread_started`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreadStartedEvent {
    /// The started thread.
    pub thread: Thread,
}

/// Payload for `thread_resumed`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreadResumedEvent {
    /// The resumed thread.
    pub thread: Thread,
}

/// Payload for `thread_forked`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreadForkedEvent {
    /// The forked thread.
    pub thread: Thread,
}

/// Payload for `thread_updated`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreadUpdatedEvent {
    /// The updated thread projection.
    pub thread: Thread,
}

/// Payload for `thread_interrupted`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreadInterruptedEvent {
    /// The interrupted thread projection.
    pub thread: Thread,
}

/// Payload for `thread_archived`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreadArchivedEvent {
    /// The archived thread projection.
    pub thread: Thread,
}

/// Payload for `turn_started`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurnStartedEvent {
    /// The started turn.
    pub turn: Turn,
}

/// Payload for `turn_completed`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurnCompletedEvent {
    /// The completed turn.
    pub turn: Turn,
}

/// Payload for `turn_failed`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurnFailedEvent {
    /// The failed turn.
    pub turn: Turn,
    /// The failure message.
    pub message: String,
}

/// Payload for `turn_cancelled`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurnCancelledEvent {
    /// The cancelled turn.
    pub turn: Turn,
}

/// Payload for `assistant_chunk`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssistantChunkEvent {
    /// The streamed text chunk.
    pub chunk: String,
    /// An optional stream identifier.
    pub stream_id: Option<String>,
    /// Whether the chunk closes the current stream.
    pub is_final: bool,
}

/// Payload for `assistant_completed`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssistantCompletedEvent {
    /// The completed assistant message.
    pub message: String,
}

/// Payload for `tool_call_started`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCallStartedEvent {
    /// The tool call identifier.
    pub call_id: String,
    /// The tool name.
    pub tool_name: String,
    /// A short preview of the call intent.
    pub summary: String,
}

/// Payload for `tool_call_updated`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCallUpdatedEvent {
    /// The tool call identifier.
    pub call_id: String,
    /// The tool name.
    pub tool_name: String,
    /// A summary of what changed in the call.
    pub delta_summary: String,
    /// An optional preview of the in-progress arguments.
    pub preview: Option<String>,
}

/// Payload for `tool_call_committed`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCallCommittedEvent {
    /// The tool call identifier.
    pub call_id: String,
    /// The tool name.
    pub tool_name: String,
    /// A short summary of the committed arguments.
    pub arguments_summary: String,
    /// An optional risk hint for the committed action.
    pub risk_hint: Option<RiskLevel>,
}

/// Payload for `tool_completed`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCompletedEvent {
    /// The tool name.
    pub tool_name: String,
    /// A short completion summary.
    pub summary: String,
    /// Artifact identifiers created by the tool.
    pub artifact_ids: Vec<ArtifactId>,
}

/// Payload for `tool_failed`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolFailedEvent {
    /// The tool name.
    pub tool_name: String,
    /// A short failure summary.
    pub summary: String,
}

/// Payload for `executor_output_chunk`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutorOutputChunkEvent {
    /// The executor task identifier.
    pub executor_task_id: ExecutorTaskId,
    /// The stream that emitted the chunk.
    pub stream: ExecutorStream,
    /// The text chunk that was emitted.
    pub chunk: String,
}

/// Payload for `approval_requested`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalRequestedEvent {
    /// The approval that now requires a decision.
    pub approval: ApprovalRequest,
}

/// Payload for `approval_resolved`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalResolvedEvent {
    /// The resolved approval request.
    pub approval: ApprovalRequest,
    /// The decision that resolved the request.
    pub decision: ApprovalDecision,
}

/// Payload for `artifact_created`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactCreatedEvent {
    /// The artifact that was created.
    pub artifact: ArtifactRef,
}

/// Payload for `diff_available`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffAvailableEvent {
    /// The diff artifact that is now available.
    pub artifact: ArtifactRef,
}

/// Payload for `checkpoint_created`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckpointCreatedEvent {
    /// The checkpoint that was created.
    pub checkpoint: Checkpoint,
}

/// Payload for `memory_wakeup_loaded`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryWakeupLoadedEvent {
    /// The wake-up payload that was loaded.
    pub wakeup: MemoryWakeup,
}

/// Payload for `memory_compilation_applied`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryCompilationAppliedEvent {
    /// The compilation summary that was applied.
    pub compilation: MemoryCompilationSummary,
}

/// Payload for `memory_invalidation_applied`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryInvalidationAppliedEvent {
    /// The invalidation summary that was applied.
    pub compilation: MemoryCompilationSummary,
}

/// Payload for `memory_dreaming_completed`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryDreamingCompletedEvent {
    /// The summary emitted by the dreaming pass.
    pub summary: String,
}
