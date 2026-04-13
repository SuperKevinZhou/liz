//! Request handling for app-server protocol messages.

use crate::runtime::{RuntimeCoordinator, RuntimeError};
use liz_protocol::requests::ClientRequest;
use liz_protocol::responses::{
    ErrorResponseEnvelope, ResponseError, ResponsePayload, ServerResponseEnvelope,
    SuccessResponseEnvelope,
};
use liz_protocol::{ClientRequestEnvelope, RequestId};

/// Dispatches a typed request to the runtime coordinator.
pub fn handle_request(
    runtime: &mut RuntimeCoordinator,
    envelope: ClientRequestEnvelope,
) -> ServerResponseEnvelope {
    let ClientRequestEnvelope { request_id, request } = envelope;

    let response = match request {
        ClientRequest::ThreadStart(request) => runtime
            .start_thread(request)
            .map(ResponsePayload::ThreadStart),
        ClientRequest::ThreadResume(request) => runtime
            .resume_thread(request)
            .map(ResponsePayload::ThreadResume),
        ClientRequest::ThreadFork(request) => runtime
            .fork_thread(request)
            .map(ResponsePayload::ThreadFork),
        ClientRequest::TurnStart(request) => runtime.start_turn(request).map(ResponsePayload::TurnStart),
        ClientRequest::TurnCancel(request) => {
            runtime.cancel_turn(request).map(ResponsePayload::TurnCancel)
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
        Ok(response) => ServerResponseEnvelope::Success(Box::new(SuccessResponseEnvelope {
            ok: true,
            request_id,
            response,
        })),
        Err(error) => ServerResponseEnvelope::Error(error_envelope(request_id, error)),
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
