//! Server error and response types.
//!
//! Provides comprehensive error handling with HTTP status mapping
//! and unified response format for all API endpoints. The V1
//! envelope (`{ "error": { "type", "message" } }`) is emitted by
//! this module; the V2 envelope (`{ "error": { "code", "message",
//! "result"? } }`) lives in `crate::api::error` (reused by the
//! full-crate [`crate::api::UserError`] which implements
//! `IntoResponse`).

use std::time::Duration;

use axum::{
    Json,
    http::{HeaderName, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use serde_json::json;

use crate::api::error::ErrorCode;

/// HTTP-boundary error type. Use `#[non_exhaustive]` so that adding
/// new variants is non-breaking for downstream consumers. Prefer
/// [`crate::api::UserError`] for new handlers — it implements
/// `IntoResponse` directly so a handler can `return Err(UserError)`
/// without a wrapping enum.
#[derive(Debug)]
#[non_exhaustive]
pub enum ServerError {
    Internal(String),
    NotFound(String),
    BadRequest(String),
    Unauthorized(String),
    Forbidden(String),
    Conflict(String),
    TooManyRequests {
        message: String,
        retry_after: Option<Duration>,
    },
    ServiceUnavailable(String),
}

impl IntoResponse for ServerError {
    fn into_response(self) -> Response {
        let (status, error_type, message, retry_after) = match self {
            ServerError::Internal(msg) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                msg,
                None,
            ),
            ServerError::NotFound(msg) => {
                (StatusCode::NOT_FOUND, "not_found", msg, None)
            }
            ServerError::BadRequest(msg) => {
                (StatusCode::BAD_REQUEST, "bad_request", msg, None)
            }
            ServerError::Unauthorized(msg) => {
                (StatusCode::UNAUTHORIZED, "unauthorized", msg, None)
            }
            ServerError::Forbidden(msg) => {
                (StatusCode::FORBIDDEN, "forbidden", msg, None)
            }
            ServerError::Conflict(msg) => {
                (StatusCode::CONFLICT, "conflict", msg, None)
            }
            ServerError::TooManyRequests {
                message,
                retry_after,
            } => (
                StatusCode::TOO_MANY_REQUESTS,
                "too_many_requests",
                message,
                retry_after,
            ),
            ServerError::ServiceUnavailable(msg) => (
                StatusCode::SERVICE_UNAVAILABLE,
                "service_unavailable",
                msg,
                None,
            ),
        };

        let body = Json(json!({
            "error": {
                "type": error_type,
                "message": message,
            }
        }));

        let mut response = (status, body).into_response();

        if let Some(duration) = retry_after {
            let seconds = duration.as_secs().max(1);
            if let Ok(value) = HeaderValue::from_str(&seconds.to_string()) {
                response
                    .headers_mut()
                    .insert(HeaderName::from_static("retry-after"), value);
            }
        }

        response
    }
}

/// Map a wire-level [`ErrorCode`] (the HTTP classifier owned by
/// `crate::api::error`) to the equivalent V1 [`ServerError`] variant.
/// Both V1 and V2 envelopes are kept in lock-step at the HTTP status
/// level; the difference is body shape (`{type, message}` vs
/// `{code, message, result?}`).
fn map_core_error_code(code: ErrorCode, msg: String) -> ServerError {
    use axum::http::StatusCode as S;
    match code.http_status() {
        S::NOT_FOUND => ServerError::NotFound(msg),
        S::BAD_REQUEST => ServerError::BadRequest(msg),
        S::UNAUTHORIZED => ServerError::Unauthorized(msg),
        S::FORBIDDEN => ServerError::Forbidden(msg),
        S::CONFLICT => ServerError::Conflict(msg),
        S::TOO_MANY_REQUESTS => ServerError::TooManyRequests {
            message: msg,
            retry_after: None,
        },
        S::SERVICE_UNAVAILABLE => ServerError::ServiceUnavailable(msg),
        _ => ServerError::Internal(msg),
    }
}

/// Bridge from the cross-crate core error to the V1 envelope.
impl From<synthia_core::Error> for ServerError {
    fn from(e: synthia_core::Error) -> Self {
        let msg = e.wire_message();
        // `RateLimited` carries a `retry_after` — surface it
        // through the V1 envelope so the frontend can read the
        // header directly.
        if let synthia_core::Error::RateLimited { retry_after, .. } = &e {
            return ServerError::TooManyRequests {
                message: msg,
                retry_after: *retry_after,
            };
        }
        map_core_error_code(error_code_for(&e), msg)
    }
}

/// Map every [`synthia_core::Error`] variant to the corresponding
/// wire-level [`ErrorCode`]. Same table as in
/// `crate::api::error` — duplicated here so the V1 envelope
/// stays a self-contained bridge without dragging in the full
/// `UserError` machinery.
fn error_code_for(err: &synthia_core::Error) -> ErrorCode {
    use synthia_core::Error as E;
    match err {
        E::NotFound { .. } | E::ModelNotFound { .. } => ErrorCode::NotFound,
        E::AlreadyExists { .. } | E::EditConflict { .. } => ErrorCode::Conflict,
        E::InvalidItem { .. } => ErrorCode::InvalidItem,
        E::Io(_) => ErrorCode::Io,
        E::Parse { .. } => ErrorCode::Parse,
        E::Internal { .. } => ErrorCode::InternalServerError,
        E::Unauthorized { .. } => ErrorCode::Unauthorized,
        E::Forbidden { .. } => ErrorCode::Forbidden,
        E::Validation { .. } => ErrorCode::ValidationError,
        E::ToolExecution { .. } => ErrorCode::ToolExecutionError,
        E::Provider { .. } | E::RequestFailed { .. } => {
            ErrorCode::ProviderError
        }
        E::Session { .. } => ErrorCode::SessionError,
        E::Skill { .. } => ErrorCode::SkillError,
        E::Memory { .. } => ErrorCode::MemoryError,
        E::GuardianViolation { .. } => ErrorCode::GuardianViolation,
        E::RateLimited { .. } => ErrorCode::RateLimited,
        E::Stream { .. } | E::StreamError { .. } => ErrorCode::Stream,
        E::Timeout { .. } => ErrorCode::Timeout,
        E::RetryExhausted { .. } => ErrorCode::RetryExhausted,
        E::ModelUnavailable { .. } => ErrorCode::ModelUnavailable,
        E::Config { .. } | E::ConfigWatcher { .. } => ErrorCode::ConfigError,
        E::Router { .. } => ErrorCode::RouterError,
        E::Task { .. } => ErrorCode::TaskError,
        E::Executor { .. } => ErrorCode::ExecutorError,
        E::Context { .. } => ErrorCode::ContextError,
        E::Telemetry { .. } => ErrorCode::TelemetryError,
        E::Multiagent { .. } => ErrorCode::MultiagentError,
        E::Evaluation { .. } => ErrorCode::EvaluationError,
        E::ContextOverflow { .. } => ErrorCode::ContextOverflow,
        E::DoomLoop { .. } => ErrorCode::DoomLoop,
        E::PromptInjection { .. } => ErrorCode::PromptInjection,
    }
}

impl From<crate::api::UserError> for ServerError {
    fn from(e: crate::api::UserError) -> Self {
        map_core_error_code(e.code, e.message)
    }
}

impl From<synthia_session::SessionError> for ServerError {
    fn from(e: synthia_session::SessionError) -> Self {
        // The new `SessionSink::SessionError` is a flat
        // enum (no `StoreError` / `StateMachineError`
        // sub-variants). Map each variant to a sensible
        // `ServerError` so the existing call sites keep
        // working.
        match e {
            synthia_session::SessionError::Closed => {
                ServerError::Internal("session is closed".into())
            }
            synthia_session::SessionError::AppendFailed(msg)
            | synthia_session::SessionError::ReadFailed(msg)
            | synthia_session::SessionError::SnapshotFailed(msg)
            | synthia_session::SessionError::CloseFailed(msg)
            | synthia_session::SessionError::Invalid(msg) => {
                ServerError::Internal(msg)
            }
        }
    }
}

impl From<anyhow::Error> for ServerError {
    fn from(e: anyhow::Error) -> Self {
        ServerError::Internal(e.to_string())
    }
}

impl From<std::io::Error> for ServerError {
    fn from(e: std::io::Error) -> Self {
        ServerError::Internal(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn too_many_requests_emits_retry_after_header() {
        let err = ServerError::TooManyRequests {
            message: "rate limit hit".to_string(),
            retry_after: Some(Duration::from_secs(42)),
        };
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        let header = response
            .headers()
            .get("retry-after")
            .expect("retry-after header should be present");
        assert_eq!(header, "42");
    }

    #[tokio::test]
    async fn too_many_requests_without_retry_after_omits_header() {
        let err = ServerError::TooManyRequests {
            message: "rate limit hit".to_string(),
            retry_after: None,
        };
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        assert!(response.headers().get("retry-after").is_none());
    }

    #[test]
    fn from_core_rate_limited_propagates_retry_after() {
        let err =
            synthia_core::Error::rate_limited(Some(Duration::from_secs(7)));
        let server_err: ServerError = err.into();
        match server_err {
            ServerError::TooManyRequests { retry_after, .. } => {
                assert_eq!(retry_after, Some(Duration::from_secs(7)));
            }
            other => panic!("expected TooManyRequests, got {other:?}"),
        }
    }
}
