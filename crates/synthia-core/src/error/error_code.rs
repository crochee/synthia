//! The [`ErrorCode`] enum — 33 stable snake_case codes that
//! classify every [`super::Error`] for wire-level API responses.
//!
//! These codes are **stable**: once a variant ships, it must not
//! be renamed or repurposed. New variants are added at the end.
//! The serde `rename_all = "snake_case"` attribute on the enum
//! controls the wire format (see the [`Display`](fmt::Display)
//! impl for the canonical string).

use std::fmt;

use serde::{Deserialize, Serialize};

/// Stable classifier for [`super::Error`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
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
        };
        f.write_str(s)
    }
}
