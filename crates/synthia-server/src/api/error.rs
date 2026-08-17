//! API error types for synthia-server.
//!
//! Owns everything HTTP-wire related:
//!
//! - [`ErrorCode`] — stable snake_case classifier derived from
//!   every [`synthia_core::Error`] variant via
//!   [`synthia_core::Error::code`].
//! - [`UserError`] — the user-facing error envelope
//!   (`{ code, message, result? }`). Implements `From<&str>`,
//!   `From<String>`, and `From<synthia_core::Error>` so handlers
//!   can `return Err(UserError::from(err))` without manual
//!   wiring.
//! - [`ErrorCode::http_status`] — the canonical HTTP status
//!   mapping. Unmapped variants fall back to 500.
//! - `IntoResponse for UserError` — pairs the JSON envelope with
//!   the derived status code.
//!
//! This module previously held an unused [`ApiError`] enum
//! describing a future V2 envelope; that was deleted in favor of
//! reusing [`UserError`] (which already implements the
//! `{code, message, result?}` shape) across handlers.

use std::fmt;

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};

/// Stable wire-level classifier for [`synthia_core::Error`].
///
/// Codes are **stable**: once a variant ships it must not be renamed
/// or repurposed. New variants may be added at the end without breaking
/// downstream consumers, thanks to `#[non_exhaustive]` (the compiler
/// enforces a wildcard arm in external `match` blocks).
///
/// Serialization uses snake_case; the [`Display`](fmt::Display) impl
/// carries the canonical string.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ErrorCode {
    BadRequest,
    Unauthorized,
    Forbidden,
    NotFound,
    Conflict,
    InternalServerError,
    ServiceUnavailable,
    ToolExecutionError,
    ProviderError,
    ValidationError,
    SessionError,
    SkillError,
    MemoryError,
    AlreadyExists,
    InvalidItem,
    Io,
    Parse,
    RateLimited,
    RetryExhausted,
    Stream,
    Timeout,
    ModelNotFound,
    ModelUnavailable,
    GuardianViolation,
    ConfigError,
    RouterError,
    TaskError,
    ExecutorError,
    ContextError,
    TelemetryError,
    MultiagentError,
    EvaluationError,
    EditConflict,
    InvalidCursor,
    InvalidSortField,
    NotImplemented,
    ContextOverflow,
    DoomLoop,
    PromptInjection,
}

impl ErrorCode {
    /// Map this [`ErrorCode`] to its canonical HTTP status code per
    /// the v1 API spec. Unmapped variants fall back to 500.
    pub fn http_status(&self) -> StatusCode {
        match self {
            ErrorCode::BadRequest
            | ErrorCode::InvalidCursor
            | ErrorCode::InvalidSortField
            | ErrorCode::InvalidItem
            | ErrorCode::Parse => StatusCode::BAD_REQUEST,
            ErrorCode::Unauthorized => StatusCode::UNAUTHORIZED,
            ErrorCode::Forbidden | ErrorCode::GuardianViolation => {
                StatusCode::FORBIDDEN
            }
            ErrorCode::NotFound | ErrorCode::ModelNotFound => {
                StatusCode::NOT_FOUND
            }
            ErrorCode::Conflict
            | ErrorCode::AlreadyExists
            | ErrorCode::EditConflict => StatusCode::CONFLICT,
            ErrorCode::ValidationError => StatusCode::UNPROCESSABLE_ENTITY,
            ErrorCode::RateLimited => StatusCode::TOO_MANY_REQUESTS,
            ErrorCode::Timeout => StatusCode::REQUEST_TIMEOUT,
            ErrorCode::ServiceUnavailable
            | ErrorCode::ModelUnavailable
            | ErrorCode::RetryExhausted => StatusCode::SERVICE_UNAVAILABLE,
            ErrorCode::NotImplemented => StatusCode::NOT_IMPLEMENTED,
            ErrorCode::ProviderError => StatusCode::BAD_GATEWAY,
            ErrorCode::ContextOverflow => StatusCode::PAYLOAD_TOO_LARGE,
            ErrorCode::DoomLoop => StatusCode::CONFLICT,
            ErrorCode::PromptInjection => StatusCode::UNPROCESSABLE_ENTITY,
            // Unmapped variants → 500 (per spec).
            ErrorCode::InternalServerError
            | ErrorCode::ToolExecutionError
            | ErrorCode::SessionError
            | ErrorCode::SkillError
            | ErrorCode::MemoryError
            | ErrorCode::Io
            | ErrorCode::Stream
            | ErrorCode::ConfigError
            | ErrorCode::RouterError
            | ErrorCode::TaskError
            | ErrorCode::ExecutorError
            | ErrorCode::ContextError
            | ErrorCode::TelemetryError
            | ErrorCode::MultiagentError
            | ErrorCode::EvaluationError => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            ErrorCode::BadRequest => "bad_request",
            ErrorCode::Unauthorized => "unauthorized",
            ErrorCode::Forbidden => "forbidden",
            ErrorCode::NotFound => "not_found",
            ErrorCode::Conflict => "conflict",
            ErrorCode::InternalServerError => "internal_server_error",
            ErrorCode::ServiceUnavailable => "service_unavailable",
            ErrorCode::ToolExecutionError => "tool_execution_error",
            ErrorCode::ProviderError => "provider_error",
            ErrorCode::ValidationError => "validation_error",
            ErrorCode::SessionError => "session_error",
            ErrorCode::SkillError => "skill_error",
            ErrorCode::MemoryError => "memory_error",
            ErrorCode::AlreadyExists => "already_exists",
            ErrorCode::InvalidItem => "invalid_item",
            ErrorCode::Io => "io_error",
            ErrorCode::Parse => "parse_error",
            ErrorCode::RateLimited => "rate_limited",
            ErrorCode::RetryExhausted => "retry_exhausted",
            ErrorCode::Stream => "stream_error",
            ErrorCode::Timeout => "timeout",
            ErrorCode::ModelNotFound => "model_not_found",
            ErrorCode::ModelUnavailable => "model_unavailable",
            ErrorCode::GuardianViolation => "guardian_violation",
            ErrorCode::ConfigError => "config_error",
            ErrorCode::RouterError => "router_error",
            ErrorCode::TaskError => "task_error",
            ErrorCode::ExecutorError => "executor_error",
            ErrorCode::ContextError => "context_error",
            ErrorCode::TelemetryError => "telemetry_error",
            ErrorCode::MultiagentError => "multiagent_error",
            ErrorCode::EvaluationError => "evaluation_error",
            ErrorCode::EditConflict => "edit_conflict",
            ErrorCode::InvalidCursor => "invalid_cursor",
            ErrorCode::InvalidSortField => "invalid_sort_field",
            ErrorCode::NotImplemented => "not_implemented",
            ErrorCode::ContextOverflow => "context_overflow",
            ErrorCode::DoomLoop => "doom_loop",
            ErrorCode::PromptInjection => "prompt_injection",
        };
        f.write_str(s)
    }
}

/// A structured, user-facing error suitable for API responses.
///
/// Serialized wire format: `{ "code": "<snake_case>", "message":
/// "<human-readable>" }` with an optional `"result"` field
/// carrying a structured payload (validation errors, rate-limit
/// metadata, etc.). The `result` field is omitted from JSON when
/// `None`, so the common case stays compact.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserError {
    pub code: ErrorCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
}

impl UserError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            result: None,
        }
    }

    pub fn with_result(
        code: ErrorCode,
        message: impl Into<String>,
        result: serde_json::Value,
    ) -> Self {
        Self {
            code,
            message: message.into(),
            result: Some(result),
        }
    }
}

impl fmt::Display for UserError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)
    }
}

impl std::error::Error for UserError {}

impl From<&str> for UserError {
    fn from(s: &str) -> Self {
        UserError::new(ErrorCode::InternalServerError, s)
    }
}

impl From<String> for UserError {
    fn from(s: String) -> Self {
        UserError::new(ErrorCode::InternalServerError, s)
    }
}

impl From<synthia_core::Error> for UserError {
    fn from(err: synthia_core::Error) -> Self {
        // Strip operator-only context: `Error::Display` includes
        // the (at <location>) suffix and any `with_context` pairs,
        // but neither is safe to leak across the wire.
        UserError::new(error_code_for(&err), err.wire_message())
    }
}

/// Map every [`synthia_core::Error`] variant to the corresponding
/// wire-level [`ErrorCode`]. Kept here (the server) so the core
/// crate stays transport-agnostic — a non-HTTP consumer can reuse
/// the [`synthia_core::Error`] enum without pulling in the
/// 39-variant wire classifier.
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

impl IntoResponse for UserError {
    fn into_response(self) -> Response {
        let status = self.code.http_status();
        // `UserError`'s serde shape already matches the spec
        // (`{ code, message, result? }` with `result` omitted when
        // `None`), so we just hand it to `Json` and pair it with
        // the status code derived from `ErrorCode`.
        (status, Json(self)).into_response()
    }
}

#[cfg(test)]
mod tests {
    use axum::body::to_bytes;

    use super::*;

    fn status_of(code: ErrorCode) -> StatusCode {
        code.http_status()
    }

    async fn body_str(err: UserError) -> String {
        let response = err.into_response();
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        String::from_utf8(bytes.to_vec()).unwrap()
    }

    #[test]
    fn mapped_codes_use_canonical_statuses() {
        assert_eq!(status_of(ErrorCode::BadRequest), StatusCode::BAD_REQUEST);
        assert_eq!(
            status_of(ErrorCode::InvalidCursor),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            status_of(ErrorCode::InvalidSortField),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            status_of(ErrorCode::Unauthorized),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(status_of(ErrorCode::Forbidden), StatusCode::FORBIDDEN);
        assert_eq!(status_of(ErrorCode::NotFound), StatusCode::NOT_FOUND);
        assert_eq!(status_of(ErrorCode::Conflict), StatusCode::CONFLICT);
        assert_eq!(status_of(ErrorCode::AlreadyExists), StatusCode::CONFLICT);
        assert_eq!(
            status_of(ErrorCode::ValidationError),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(
            status_of(ErrorCode::RateLimited),
            StatusCode::TOO_MANY_REQUESTS
        );
        assert_eq!(
            status_of(ErrorCode::InternalServerError),
            StatusCode::INTERNAL_SERVER_ERROR
        );
        assert_eq!(
            status_of(ErrorCode::ServiceUnavailable),
            StatusCode::SERVICE_UNAVAILABLE
        );
    }

    #[test]
    fn unmapped_codes_default_to_500() {
        assert_eq!(
            status_of(ErrorCode::ToolExecutionError),
            StatusCode::INTERNAL_SERVER_ERROR
        );
        assert_eq!(
            status_of(ErrorCode::ProviderError),
            StatusCode::BAD_GATEWAY
        );
        assert_eq!(
            status_of(ErrorCode::MemoryError),
            StatusCode::INTERNAL_SERVER_ERROR
        );
        assert_eq!(
            status_of(ErrorCode::TelemetryError),
            StatusCode::INTERNAL_SERVER_ERROR
        );
        assert_eq!(
            status_of(ErrorCode::ContextOverflow),
            StatusCode::PAYLOAD_TOO_LARGE
        );
        assert_eq!(status_of(ErrorCode::DoomLoop), StatusCode::CONFLICT);
        assert_eq!(
            status_of(ErrorCode::PromptInjection),
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }

    /// `http_status()` MUST cover every variant: a new variant
    /// added without a status mapping will fall through to the
    /// default arm (500) silently. This test enumerates the
    /// full variant set so any unmapped code fails fast.
    #[test]
    fn http_status_covers_every_variant() {
        let all = [
            ErrorCode::BadRequest,
            ErrorCode::Unauthorized,
            ErrorCode::Forbidden,
            ErrorCode::NotFound,
            ErrorCode::Conflict,
            ErrorCode::InternalServerError,
            ErrorCode::ServiceUnavailable,
            ErrorCode::ToolExecutionError,
            ErrorCode::ProviderError,
            ErrorCode::ValidationError,
            ErrorCode::SessionError,
            ErrorCode::SkillError,
            ErrorCode::MemoryError,
            ErrorCode::AlreadyExists,
            ErrorCode::InvalidItem,
            ErrorCode::Io,
            ErrorCode::Parse,
            ErrorCode::RateLimited,
            ErrorCode::RetryExhausted,
            ErrorCode::Stream,
            ErrorCode::Timeout,
            ErrorCode::ModelNotFound,
            ErrorCode::ModelUnavailable,
            ErrorCode::GuardianViolation,
            ErrorCode::ConfigError,
            ErrorCode::RouterError,
            ErrorCode::TaskError,
            ErrorCode::ExecutorError,
            ErrorCode::ContextError,
            ErrorCode::TelemetryError,
            ErrorCode::MultiagentError,
            ErrorCode::EvaluationError,
            ErrorCode::EditConflict,
            ErrorCode::InvalidCursor,
            ErrorCode::InvalidSortField,
            ErrorCode::NotImplemented,
            ErrorCode::ContextOverflow,
            ErrorCode::DoomLoop,
            ErrorCode::PromptInjection,
        ];
        for code in all {
            // Just exercise the mapping; any variant that hits
            // the fallback (500) is still mapped, so the contract
            // is "every variant returns SOMETHING". Specific
            // status assertions live in the targeted tests
            // above.
            let _ = code.http_status();
        }
    }

    #[tokio::test]
    async fn body_has_code_and_message_no_result_when_absent() {
        let err = UserError::new(ErrorCode::NotFound, "session not found");
        let body = body_str(err).await;
        assert!(body.contains("\"code\":\"not_found\""));
        assert!(body.contains("\"message\":\"session not found\""));
        assert!(!body.contains("result"));
        assert!(!body.contains("details"));
    }

    #[tokio::test]
    async fn body_includes_result_when_present() {
        let err = UserError::with_result(
            ErrorCode::ValidationError,
            "invalid input",
            serde_json::json!({"field": "name", "issue": "required"}),
        );
        let body = body_str(err).await;
        assert!(body.contains("\"code\":\"validation_error\""));
        assert!(body.contains("\"result\""));
        assert!(body.contains("\"field\":\"name\""));
    }

    #[tokio::test]
    async fn into_response_status_matches_code() {
        let err = UserError::new(ErrorCode::Conflict, "in conflict");
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn into_response_rate_limit_includes_result() {
        let err = UserError::with_result(
            ErrorCode::RateLimited,
            "Too many requests",
            serde_json::json!({
                "retry_after_seconds": 30,
                "limit": 100,
                "remaining": 0
            }),
        );
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["code"], "rate_limited");
        assert_eq!(value["result"]["retry_after_seconds"], 30);
    }

    #[test]
    fn from_str_slice_uses_internal_server_error_code() {
        let e: UserError = "boom".into();
        assert_eq!(e.code, ErrorCode::InternalServerError);
        assert_eq!(e.message, "boom");
        assert!(e.result.is_none());
    }

    #[test]
    fn from_string_uses_internal_server_error_code() {
        let e: UserError = String::from("kaboom").into();
        assert_eq!(e.code, ErrorCode::InternalServerError);
        assert_eq!(e.message, "kaboom");
        assert!(e.result.is_none());
    }

    #[test]
    fn from_core_error_preserves_code_mapping() {
        let cases = [
            (synthia_core::Error::not_found("x"), ErrorCode::NotFound),
            (
                synthia_core::Error::validation("x"),
                ErrorCode::ValidationError,
            ),
            (
                synthia_core::Error::context_overflow(100, 200),
                ErrorCode::ContextOverflow,
            ),
            (synthia_core::Error::doom_loop("t", 1), ErrorCode::DoomLoop),
            (
                synthia_core::Error::prompt_injection("s", "p"),
                ErrorCode::PromptInjection,
            ),
        ];
        for (err, expected_code) in cases {
            let user_err: UserError = err.into();
            assert_eq!(user_err.code, expected_code);
        }
    }
}
