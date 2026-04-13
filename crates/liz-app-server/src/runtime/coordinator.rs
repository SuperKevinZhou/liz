//! High-level runtime coordination for thread and turn lifecycle work.

use crate::runtime::error::{RuntimeError, RuntimeResult};
use crate::runtime::ids::IdGenerator;
use crate::runtime::stores::RuntimeStores;
use crate::runtime::thread_manager::ThreadManager;
use crate::runtime::turn_manager::TurnManager;
use liz_protocol::memory::ResumeSummary;
use liz_protocol::requests::{
    ThreadForkRequest, ThreadResumeRequest, ThreadStartRequest, TurnCancelRequest,
    TurnStartRequest,
};
use liz_protocol::responses::{
    ThreadForkResponse, ThreadResumeResponse, ThreadStartResponse, TurnCancelResponse,
    TurnStartResponse,
};
use liz_protocol::{Thread, ThreadId, Turn};

/// Coordinates the persisted runtime state for thread and turn lifecycle actions.
#[derive(Debug)]
pub struct RuntimeCoordinator {
    stores: RuntimeStores,
    ids: IdGenerator,
    thread_manager: ThreadManager,
    turn_manager: TurnManager,
}

impl RuntimeCoordinator {
    /// Creates a runtime coordinator backed by the provided stores.
    pub fn new(stores: RuntimeStores) -> Self {
        Self {
            stores,
            ids: IdGenerator::default(),
            thread_manager: ThreadManager::default(),
            turn_manager: TurnManager::default(),
        }
    }

    /// Returns the short runtime mode label used by the binary banner.
    pub fn default_mode() -> &'static str {
        "thread-turn-runtime"
    }

    /// Starts a new thread and persists the initial thread projection.
    pub fn start_thread(&mut self, request: ThreadStartRequest) -> RuntimeResult<ThreadStartResponse> {
        let thread = self.thread_manager.start_thread(&self.stores, &mut self.ids, request)?;
        Ok(ThreadStartResponse { thread })
    }

    /// Resumes a thread and returns the current wake-up projection.
    pub fn resume_thread(
        &mut self,
        request: ThreadResumeRequest,
    ) -> RuntimeResult<ThreadResumeResponse> {
        let thread = self.thread_manager.resume_thread(&self.stores, request)?;
        let resume_summary = Some(self.build_resume_summary(&thread));
        Ok(ThreadResumeResponse { thread, resume_summary })
    }

    /// Forks a thread into a new line of work.
    pub fn fork_thread(&mut self, request: ThreadForkRequest) -> RuntimeResult<ThreadForkResponse> {
        let thread = self.thread_manager.fork_thread(&self.stores, &mut self.ids, request)?;
        Ok(ThreadForkResponse { thread })
    }

    /// Starts a turn on an existing thread.
    pub fn start_turn(&mut self, request: TurnStartRequest) -> RuntimeResult<TurnStartResponse> {
        let thread = self
            .stores
            .get_thread(&request.thread_id)?
            .ok_or_else(|| RuntimeError::not_found("thread_not_found", "thread does not exist"))?;
        let turn = self.turn_manager.start_turn(
            &self.stores,
            &mut self.ids,
            &self.thread_manager,
            thread,
            request,
        )?;

        Ok(TurnStartResponse { turn })
    }

    /// Cancels a running turn and projects the interruption back onto the thread.
    pub fn cancel_turn(&mut self, request: TurnCancelRequest) -> RuntimeResult<TurnCancelResponse> {
        let turn = self.turn_manager.cancel_turn(&self.stores, &mut self.ids, request)?;
        Ok(TurnCancelResponse { turn })
    }

    /// Returns a persisted thread when it exists.
    pub fn read_thread(&self, thread_id: &ThreadId) -> RuntimeResult<Option<Thread>> {
        Ok(self.stores.get_thread(thread_id)?)
    }

    /// Returns the active in-memory turn projection when it exists.
    pub fn read_turn(&self, turn_id: &liz_protocol::TurnId) -> Option<Turn> {
        self.turn_manager.read_turn(turn_id)
    }

    fn build_resume_summary(&self, thread: &Thread) -> ResumeSummary {
        let headline = match thread.latest_turn_id.as_ref() {
            Some(turn_id) => format!("Resume thread {} from {turn_id}", thread.title),
            None => format!("Resume thread {}", thread.title),
        };

        ResumeSummary {
            headline,
            active_summary: thread.active_summary.clone(),
            pending_commitments: thread.pending_commitments.clone(),
            last_interruption: thread.last_interruption.clone(),
        }
    }
}

impl Default for RuntimeCoordinator {
    fn default() -> Self {
        Self::new(RuntimeStores::from_default_layout())
    }
}
