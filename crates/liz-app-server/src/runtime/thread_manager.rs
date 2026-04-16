//! Thread lifecycle management.

use crate::runtime::error::{RuntimeError, RuntimeResult};
use crate::runtime::ids::IdGenerator;
use crate::runtime::stores::RuntimeStores;
use liz_protocol::requests::{ThreadForkRequest, ThreadResumeRequest, ThreadStartRequest};
use liz_protocol::{Thread, ThreadStatus};

/// Coordinates thread-level lifecycle transitions.
#[derive(Debug, Clone, Default)]
pub struct ThreadManager;

impl ThreadManager {
    /// Creates and persists a new thread.
    pub fn start_thread(
        &self,
        stores: &RuntimeStores,
        ids: &mut IdGenerator,
        request: ThreadStartRequest,
    ) -> RuntimeResult<Thread> {
        let now = ids.now_timestamp();
        let title = request
            .title
            .filter(|value| !value.trim().is_empty())
            .or_else(|| request.initial_goal.clone())
            .unwrap_or_else(|| "Untitled thread".to_owned());

        let thread = Thread {
            id: ids.next_thread_id(),
            title,
            status: ThreadStatus::Active,
            created_at: now.clone(),
            updated_at: now,
            active_goal: request.initial_goal.clone(),
            active_summary: request
                .initial_goal
                .as_ref()
                .map(|goal| format!("New thread initialized for: {goal}")),
            last_interruption: None,
            pending_commitments: Vec::new(),
            latest_turn_id: None,
            latest_checkpoint_id: None,
            parent_thread_id: None,
        };

        stores.put_thread(&thread)?;
        self.sync_active_thread_ids(stores, &thread.id, true)?;
        Ok(thread)
    }

    /// Marks a thread as resumed and returns the updated projection.
    pub fn resume_thread(
        &self,
        stores: &RuntimeStores,
        request: ThreadResumeRequest,
    ) -> RuntimeResult<Thread> {
        let mut thread = stores
            .get_thread(&request.thread_id)?
            .ok_or_else(|| RuntimeError::not_found("thread_not_found", "thread does not exist"))?;

        thread.status = ThreadStatus::Active;
        thread.updated_at = IdGenerator::default().now_timestamp();
        stores.put_thread(&thread)?;
        self.sync_active_thread_ids(stores, &thread.id, true)?;
        Ok(thread)
    }

    /// Creates a child thread that inherits the active state of its parent.
    pub fn fork_thread(
        &self,
        stores: &RuntimeStores,
        ids: &mut IdGenerator,
        request: ThreadForkRequest,
    ) -> RuntimeResult<Thread> {
        let parent = stores
            .get_thread(&request.thread_id)?
            .ok_or_else(|| RuntimeError::not_found("thread_not_found", "thread does not exist"))?;
        let now = ids.now_timestamp();
        let title = request
            .title
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| format!("{} (fork)", parent.title));

        let mut commitments = parent.pending_commitments.clone();
        if let Some(reason) = request.fork_reason.as_ref() {
            commitments.push(format!("Fork created for: {reason}"));
        }

        let thread = Thread {
            id: ids.next_thread_id(),
            title,
            status: ThreadStatus::Active,
            created_at: now.clone(),
            updated_at: now,
            active_goal: parent.active_goal.clone(),
            active_summary: Some(match request.fork_reason {
                Some(reason) => format!("Forked from {} to explore: {reason}", parent.title),
                None => format!("Forked from {}", parent.title),
            }),
            last_interruption: parent.last_interruption.clone(),
            pending_commitments: commitments,
            latest_turn_id: None,
            latest_checkpoint_id: parent.latest_checkpoint_id.clone(),
            parent_thread_id: Some(parent.id),
        };

        stores.put_thread(&thread)?;
        self.sync_active_thread_ids(stores, &thread.id, true)?;
        Ok(thread)
    }

    /// Persists a thread projection after a turn transition.
    pub fn update_thread_after_turn(
        &self,
        stores: &RuntimeStores,
        thread: &Thread,
    ) -> RuntimeResult<()> {
        stores.put_thread(thread)?;
        let should_be_active =
            !matches!(thread.status, ThreadStatus::Archived | ThreadStatus::Completed);
        self.sync_active_thread_ids(stores, &thread.id, should_be_active)?;
        Ok(())
    }

    fn sync_active_thread_ids(
        &self,
        stores: &RuntimeStores,
        thread_id: &liz_protocol::ThreadId,
        should_be_present: bool,
    ) -> RuntimeResult<()> {
        let mut snapshot = stores.read_global_memory()?;
        snapshot.active_thread_ids.retain(|value| value != thread_id);
        if should_be_present {
            snapshot.active_thread_ids.push(thread_id.clone());
        }
        stores.write_global_memory(&snapshot)?;
        Ok(())
    }
}
