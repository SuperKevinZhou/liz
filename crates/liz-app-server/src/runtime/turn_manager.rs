//! Turn lifecycle management and thread-state projection.

use crate::runtime::error::{RuntimeError, RuntimeResult};
use crate::runtime::ids::IdGenerator;
use crate::runtime::stores::RuntimeStores;
use crate::runtime::thread_manager::ThreadManager;
use crate::storage::TurnLogEntry;
use liz_protocol::requests::{TurnCancelRequest, TurnStartRequest};
use liz_protocol::{Thread, ThreadStatus, Turn, TurnId, TurnInputKind, TurnKind, TurnStatus};
use std::collections::HashMap;

/// Coordinates turn state and turn-to-thread projection.
#[derive(Debug, Clone, Default)]
pub struct TurnManager {
    active_turns: HashMap<TurnId, Turn>,
    next_sequence: HashMap<liz_protocol::ThreadId, u64>,
}

impl TurnManager {
    /// Starts a new turn and updates the parent thread projection.
    pub fn start_turn(
        &mut self,
        stores: &RuntimeStores,
        ids: &mut IdGenerator,
        thread_manager: &ThreadManager,
        mut thread: Thread,
        request: TurnStartRequest,
    ) -> RuntimeResult<Turn> {
        if self.thread_has_running_turn(&thread.id) {
            return Err(RuntimeError::invalid_state(
                "turn_already_running",
                "thread already has a running turn",
            ));
        }

        let now = ids.now_timestamp();
        let goal = request.input.trim().to_owned();
        let turn = Turn {
            id: ids.next_turn_id(),
            thread_id: thread.id.clone(),
            kind: map_input_kind(request.input_kind),
            status: TurnStatus::Running,
            started_at: now.clone(),
            ended_at: None,
            goal: Some(goal.clone()),
            summary: Some(format!("Started turn for: {goal}")),
            checkpoint_before: thread.latest_checkpoint_id.clone(),
            checkpoint_after: None,
        };

        thread.status = ThreadStatus::Active;
        thread.updated_at = now.clone();
        thread.active_goal = Some(goal.clone());
        thread.active_summary = Some(format!("Currently working on: {goal}"));
        thread.last_interruption = None;
        thread.latest_turn_id = Some(turn.id.clone());
        thread.pending_commitments.retain(|commitment| !commitment.contains(&goal));

        stores.append_turn_log(&TurnLogEntry {
            thread_id: thread.id.clone(),
            sequence: self.next_sequence_for(&thread.id),
            turn_id: Some(turn.id.clone()),
            recorded_at: now.clone(),
            event: "turn_started".to_owned(),
            summary: turn.summary.clone().unwrap_or_default(),
        })?;
        thread_manager.update_thread_after_turn(stores, &thread)?;

        self.active_turns.insert(turn.id.clone(), turn.clone());
        Ok(turn)
    }

    /// Cancels a running turn and records an interruption marker on the thread.
    pub fn cancel_turn(
        &mut self,
        stores: &RuntimeStores,
        ids: &mut IdGenerator,
        request: TurnCancelRequest,
    ) -> RuntimeResult<Turn> {
        let mut thread = stores
            .get_thread(&request.thread_id)?
            .ok_or_else(|| RuntimeError::not_found("thread_not_found", "thread does not exist"))?;
        let turn = self
            .active_turns
            .get(&request.turn_id)
            .cloned()
            .ok_or_else(|| RuntimeError::not_found("turn_not_found", "turn is not running"))?;

        if turn.thread_id != request.thread_id {
            return Err(RuntimeError::invalid_state(
                "turn_thread_mismatch",
                "turn does not belong to the requested thread",
            ));
        }

        let mut turn = turn;
        self.active_turns.remove(&request.turn_id);
        let ended_at = ids.now_timestamp();
        let goal = turn.goal.clone().unwrap_or_else(|| "current turn".to_owned());
        turn.status = TurnStatus::Cancelled;
        turn.ended_at = Some(ended_at.clone());
        turn.summary = Some(format!("Interrupted before finishing: {goal}"));

        let interruption = format!("Interrupted while working on: {goal}");
        let commitment = format!("Resume interrupted work: {goal}");

        thread.status = ThreadStatus::Interrupted;
        thread.updated_at = ended_at.clone();
        thread.active_summary = Some(interruption.clone());
        thread.last_interruption = Some(interruption);
        if !thread.pending_commitments.iter().any(|item| item == &commitment) {
            thread.pending_commitments.push(commitment);
        }
        thread.latest_turn_id = Some(turn.id.clone());

        stores.append_turn_log(&TurnLogEntry {
            thread_id: thread.id.clone(),
            sequence: self.next_sequence_for(&thread.id),
            turn_id: Some(turn.id.clone()),
            recorded_at: ended_at,
            event: "turn_cancelled".to_owned(),
            summary: turn.summary.clone().unwrap_or_default(),
        })?;
        stores.put_thread(&thread)?;
        Ok(turn)
    }

    /// Reads the current in-memory turn projection.
    pub fn read_turn(&self, turn_id: &TurnId) -> Option<Turn> {
        self.active_turns.get(turn_id).cloned()
    }

    /// Completes a running turn and persists the updated thread projection.
    pub fn complete_turn(
        &mut self,
        stores: &RuntimeStores,
        ids: &mut IdGenerator,
        thread_id: &liz_protocol::ThreadId,
        turn_id: &TurnId,
        final_message: String,
    ) -> RuntimeResult<Turn> {
        let mut thread = stores
            .get_thread(thread_id)?
            .ok_or_else(|| RuntimeError::not_found("thread_not_found", "thread does not exist"))?;
        let mut turn = self
            .active_turns
            .remove(turn_id)
            .ok_or_else(|| RuntimeError::not_found("turn_not_found", "turn is not running"))?;

        let ended_at = ids.now_timestamp();
        turn.status = TurnStatus::Completed;
        turn.ended_at = Some(ended_at.clone());
        turn.summary = Some(final_message.clone());

        thread.status = ThreadStatus::Active;
        thread.updated_at = ended_at.clone();
        thread.active_summary = Some(final_message);
        thread.last_interruption = None;
        thread.latest_turn_id = Some(turn.id.clone());

        stores.append_turn_log(&TurnLogEntry {
            thread_id: thread.id.clone(),
            sequence: self.next_sequence_for(&thread.id),
            turn_id: Some(turn.id.clone()),
            recorded_at: ended_at,
            event: "turn_completed".to_owned(),
            summary: turn.summary.clone().unwrap_or_default(),
        })?;
        stores.put_thread(&thread)?;
        Ok(turn)
    }

    /// Fails a running turn and projects the failure onto the thread.
    pub fn fail_turn(
        &mut self,
        stores: &RuntimeStores,
        ids: &mut IdGenerator,
        thread_id: &liz_protocol::ThreadId,
        turn_id: &TurnId,
        message: String,
    ) -> RuntimeResult<Turn> {
        let mut thread = stores
            .get_thread(thread_id)?
            .ok_or_else(|| RuntimeError::not_found("thread_not_found", "thread does not exist"))?;
        let mut turn = self
            .active_turns
            .remove(turn_id)
            .ok_or_else(|| RuntimeError::not_found("turn_not_found", "turn is not running"))?;

        let ended_at = ids.now_timestamp();
        turn.status = TurnStatus::Failed;
        turn.ended_at = Some(ended_at.clone());
        turn.summary = Some(message.clone());

        thread.status = ThreadStatus::Failed;
        thread.updated_at = ended_at.clone();
        thread.active_summary = Some(format!("Turn failed: {message}"));
        thread.latest_turn_id = Some(turn.id.clone());

        stores.append_turn_log(&TurnLogEntry {
            thread_id: thread.id.clone(),
            sequence: self.next_sequence_for(&thread.id),
            turn_id: Some(turn.id.clone()),
            recorded_at: ended_at,
            event: "turn_failed".to_owned(),
            summary: turn.summary.clone().unwrap_or_default(),
        })?;
        stores.put_thread(&thread)?;
        Ok(turn)
    }

    fn next_sequence_for(&mut self, thread_id: &liz_protocol::ThreadId) -> u64 {
        let sequence = self.next_sequence.entry(thread_id.clone()).or_insert(0);
        *sequence += 1;
        *sequence
    }

    fn thread_has_running_turn(&self, thread_id: &liz_protocol::ThreadId) -> bool {
        self.active_turns.values().any(|turn| {
            turn.thread_id == *thread_id && matches!(turn.status, TurnStatus::Running | TurnStatus::WaitingApproval)
        })
    }
}

fn map_input_kind(kind: TurnInputKind) -> TurnKind {
    match kind {
        TurnInputKind::UserMessage | TurnInputKind::ResumeCommand => TurnKind::User,
        TurnInputKind::SteeringNote => TurnKind::Assistant,
    }
}
