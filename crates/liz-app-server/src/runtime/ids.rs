//! Runtime-scoped identifier and timestamp helpers.

use liz_protocol::primitives::Timestamp;
use liz_protocol::{ThreadId, TurnId};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Generates stable-enough IDs for local runtime work.
#[derive(Debug, Default)]
pub struct IdGenerator {
    sequence: AtomicU64,
}

impl IdGenerator {
    /// Creates a thread identifier.
    pub fn next_thread_id(&self) -> ThreadId {
        ThreadId::new(self.next_value("thread"))
    }

    /// Creates a turn identifier.
    pub fn next_turn_id(&self) -> TurnId {
        TurnId::new(self.next_value("turn"))
    }

    /// Produces a serialized timestamp string for persisted protocol resources.
    pub fn now_timestamp(&self) -> Timestamp {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        Timestamp::new(format!("unix:{}.{:09}", now.as_secs(), now.subsec_nanos()))
    }

    fn next_value(&self, prefix: &str) -> String {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        let sequence = self.sequence.fetch_add(1, Ordering::Relaxed) + 1;
        format!("{prefix}_{:x}_{sequence}", now.as_nanos())
    }
}
