//! Event-driven CLI view model primitives.

use liz_protocol::events::{
    ApprovalRequestedEvent, ThreadInterruptedEvent, ThreadResumedEvent, ThreadStartedEvent,
    ThreadUpdatedEvent, TurnCancelledEvent, TurnStartedEvent,
};
use liz_protocol::{ApprovalRequest, ServerEvent, ServerEventPayload, ThreadId, ThreadStatus};
use std::collections::BTreeMap;

/// Minimal event-projected view model for the reference CLI.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ViewModel {
    /// The currently known thread statuses.
    pub thread_statuses: BTreeMap<ThreadId, ThreadStatus>,
    /// Human-readable transcript lines derived from the event stream.
    pub transcript_lines: Vec<String>,
    /// Approvals currently waiting on user action.
    pub pending_approvals: Vec<ApprovalRequest>,
}

impl ViewModel {
    /// Returns the primary view name surfaced by the CLI banner.
    pub fn primary_view() -> &'static str {
        "transcript"
    }

    /// Applies one server event to the current CLI projection.
    pub fn apply_event(&mut self, event: &ServerEvent) {
        match &event.payload {
            ServerEventPayload::ThreadStarted(ThreadStartedEvent { thread })
            | ServerEventPayload::ThreadResumed(ThreadResumedEvent { thread })
            | ServerEventPayload::ThreadUpdated(ThreadUpdatedEvent { thread })
            | ServerEventPayload::ThreadInterrupted(ThreadInterruptedEvent { thread }) => {
                self.thread_statuses.insert(thread.id.clone(), thread.status);
            }
            ServerEventPayload::TurnStarted(TurnStartedEvent { turn }) => {
                self.transcript_lines.push(format!(
                    "[{}] turn started: {}",
                    turn.thread_id,
                    turn.goal.clone().unwrap_or_default()
                ));
            }
            ServerEventPayload::TurnCancelled(TurnCancelledEvent { turn }) => {
                self.transcript_lines.push(format!(
                    "[{}] turn interrupted: {}",
                    turn.thread_id,
                    turn.goal.clone().unwrap_or_default()
                ));
            }
            ServerEventPayload::ApprovalRequested(ApprovalRequestedEvent { approval }) => {
                self.pending_approvals.push(approval.clone());
            }
            _ => {}
        }
    }
}
