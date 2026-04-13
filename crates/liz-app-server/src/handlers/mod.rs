//! Request handling for app-server protocol messages.

use crate::events::PendingEvent;
use crate::runtime::{RuntimeCoordinator, RuntimeError};
use liz_protocol::events::{
    ThreadForkedEvent, ThreadInterruptedEvent, ThreadResumedEvent, ThreadStartedEvent,
    ThreadUpdatedEvent, TurnCancelledEvent, TurnStartedEvent,
};
use liz_protocol::requests::ClientRequest;
use liz_protocol::responses::{
    ErrorResponseEnvelope, ResponseError, ResponsePayload, ServerResponseEnvelope,
    SuccessResponseEnvelope,
};
use liz_protocol::{ClientRequestEnvelope, RequestId, ServerEventPayload};

/// The fully handled result of a request, including any events that should be published.
#[derive(Debug)]
pub struct HandledRequest {
    /// The response that should be returned to the caller.
    pub response: ServerResponseEnvelope,
    /// The events that should be emitted as a consequence of the request.
    pub events: Vec<PendingEvent>,
}

/// Dispatches a typed request to the runtime coordinator.
pub fn handle_request(
    runtime: &mut RuntimeCoordinator,
    envelope: ClientRequestEnvelope,
) -> HandledRequest {
    let ClientRequestEnvelope { request_id, request } = envelope;

    let response = match request {
        ClientRequest::ThreadStart(request) => runtime.start_thread(request).map(|response| {
            let thread = response.thread.clone();
            (
                ResponsePayload::ThreadStart(response),
                vec![PendingEvent::new(
                    thread.id.clone(),
                    None,
                    ServerEventPayload::ThreadStarted(ThreadStartedEvent { thread }),
                )],
            )
        }),
        ClientRequest::ThreadResume(request) => runtime.resume_thread(request).map(|response| {
            let thread = response.thread.clone();
            (
                ResponsePayload::ThreadResume(response),
                vec![PendingEvent::new(
                    thread.id.clone(),
                    None,
                    ServerEventPayload::ThreadResumed(ThreadResumedEvent { thread }),
                )],
            )
        }),
        ClientRequest::ThreadFork(request) => runtime.fork_thread(request).map(|response| {
            let thread = response.thread.clone();
            (
                ResponsePayload::ThreadFork(response),
                vec![PendingEvent::new(
                    thread.id.clone(),
                    None,
                    ServerEventPayload::ThreadForked(ThreadForkedEvent { thread }),
                )],
            )
        }),
        ClientRequest::TurnStart(request) => runtime.start_turn(request).map(|response| {
            let turn = response.turn.clone();
            let mut events = vec![PendingEvent::new(
                turn.thread_id.clone(),
                Some(turn.id.clone()),
                ServerEventPayload::TurnStarted(TurnStartedEvent { turn }),
            )];
            if let Ok(Some(thread)) = runtime.read_thread(&response.turn.thread_id) {
                events.push(PendingEvent::new(
                    thread.id.clone(),
                    response.turn.id.clone().into(),
                    ServerEventPayload::ThreadUpdated(ThreadUpdatedEvent { thread }),
                ));
            }
            (ResponsePayload::TurnStart(response), events)
        }),
        ClientRequest::TurnCancel(request) => {
            runtime.cancel_turn(request).map(|response| {
                let turn = response.turn.clone();
                let mut events = vec![PendingEvent::new(
                    turn.thread_id.clone(),
                    Some(turn.id.clone()),
                    ServerEventPayload::TurnCancelled(TurnCancelledEvent { turn }),
                )];
                if let Ok(Some(thread)) = runtime.read_thread(&response.turn.thread_id) {
                    events.push(PendingEvent::new(
                        thread.id.clone(),
                        Some(response.turn.id.clone()),
                        ServerEventPayload::ThreadInterrupted(ThreadInterruptedEvent { thread }),
                    ));
                }
                (ResponsePayload::TurnCancel(response), events)
            })
        }
        ClientRequest::ApprovalRespond(_) => Err(RuntimeError::unsupported(
            "approval_not_ready",
            "approval handling is implemented in a later phase",
        )),
        ClientRequest::ThreadRollback(_) => Err(RuntimeError::unsupported(
            "rollback_not_ready",
            "rollback handling is implemented in a later phase",
        )),
    };

    match response {
        Ok((response, events)) => HandledRequest {
            response: ServerResponseEnvelope::Success(Box::new(SuccessResponseEnvelope {
                ok: true,
                request_id,
                response,
            })),
            events,
        },
        Err(error) => HandledRequest {
            response: ServerResponseEnvelope::Error(error_envelope(request_id, error)),
            events: Vec::new(),
        },
    }
}

fn error_envelope(request_id: RequestId, error: RuntimeError) -> ErrorResponseEnvelope {
    ErrorResponseEnvelope {
        ok: false,
        request_id,
        error: ResponseError {
            code: error.code().to_owned(),
            message: error.to_string(),
            retryable: error.retryable(),
        },
    }
}
