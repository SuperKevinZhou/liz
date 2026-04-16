//! Event streaming primitives for the app server.

use liz_protocol::{EventId, ServerEvent, ServerEventPayload, ThreadId, Timestamp, TurnId};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

/// Minimal event-stream metadata for the app server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventStreamSkeleton {
    /// The transport that the event stream assumes for v0.
    pub transport: &'static str,
}

impl Default for EventStreamSkeleton {
    fn default() -> Self {
        Self { transport: "websocket" }
    }
}

/// A not-yet-materialized event emitted by the runtime.
#[derive(Debug, Clone)]
pub struct PendingEvent {
    /// The thread the event belongs to.
    pub thread_id: ThreadId,
    /// The turn the event belongs to, if any.
    pub turn_id: Option<TurnId>,
    /// The typed server-event payload.
    pub payload: ServerEventPayload,
}

impl PendingEvent {
    /// Creates a pending event that can later be assigned an ID and timestamp.
    pub fn new(thread_id: ThreadId, turn_id: Option<TurnId>, payload: ServerEventPayload) -> Self {
        Self { thread_id, turn_id, payload }
    }
}

/// Broadcasts runtime events to every active subscriber.
#[derive(Debug, Clone, Default)]
pub struct EventBus {
    subscribers: Arc<Mutex<Vec<Sender<ServerEvent>>>>,
    sequence: Arc<AtomicU64>,
}

impl EventBus {
    /// Creates a new event bus.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a new subscriber and returns its receiving end.
    pub fn subscribe(&self) -> Receiver<ServerEvent> {
        let (sender, receiver) = mpsc::channel();
        self.subscribers.lock().expect("event bus mutex should not be poisoned").push(sender);
        receiver
    }

    /// Publishes all provided pending events to the current subscribers.
    pub fn publish_all(&self, events: Vec<PendingEvent>) -> Vec<ServerEvent> {
        events.into_iter().map(|event| self.publish(event)).collect()
    }

    /// Publishes a single pending event to the current subscribers.
    pub fn publish(&self, pending: PendingEvent) -> ServerEvent {
        let sequence = self.sequence.fetch_add(1, Ordering::Relaxed) + 1;
        let event = ServerEvent {
            event_id: EventId::new(format!("event_{sequence}")),
            thread_id: pending.thread_id,
            turn_id: pending.turn_id,
            created_at: now_timestamp(),
            payload: pending.payload,
        };

        let mut subscribers =
            self.subscribers.lock().expect("event bus mutex should not be poisoned");
        subscribers.retain(|sender| sender.send(event.clone()).is_ok());
        event
    }
}

fn now_timestamp() -> Timestamp {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    Timestamp::new(format!("unix:{}.{:09}", now.as_secs(), now.subsec_nanos()))
}
