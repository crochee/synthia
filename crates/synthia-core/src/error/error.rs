//! The [`Error`] enum — the single cross-crate error type for
//! Synthia, used by every workspace member. Plus the
//! [`Error::is_retryable`], [`Error::is_rate_limited`],
//! [`Error::stream_error`], and [`Error::code`] accessors and
//! the three external `From` impls
//! ([`From<reqwest::Error>`], [`From<serde_json::Error>`],
//!  [`From<serde_yaml::Error>`]).

use thiserror::Error;

use super::{error_code::ErrorCode, stream_error_kind::StreamErrorKind};

/// Top-level Synthia error. Returned by every fallible operation
/// across the workspace.
#[derive(Debug, Error)]
pub enum Error {
    #[error("not found: {0}")]
    NotFound(String),

    #[error("already exists: {0}")]
    AlreadyExists(String),

    #[error("invalid item: {0}")]
    InvalidItem(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("parse error: {0}")]
    Parse(String),

    #[error("internal error: {0}")]
    Internal(String),

    #[error("unauthorized: {0}")]
    Unauthorized(String),

    #[error("forbidden: {0}")]
    Forbidden(String),

    #[error("validation error: {0}")]
    Validation(String),

    #[error("tool execution error: {0}")]
    ToolExecution(String),

    #[error("provider error: {0}")]
    Provider(String),

    #[error("session error: {0}")]
    Session(String),

    #[error("skill error: {0}")]
    Skill(String),

    #[error("memory error: {0}")]
    Memory(String),

    #[error("guardian violation: {0}")]
    GuardianViolation(String),

    #[error(
        "edit conflict on {path}: original_hash={original_hash}, current_hash={current_hash}"
    )]
    EditConflict {
        path: std::path::PathBuf,
        original_hash: u64,
        current_hash: u64,
    },

    #[error("rate limited, retry after {0:?}")]
    RateLimited(Option<std::time::Duration>),

    #[error("request failed with status {status}: {message}")]
    RequestFailed { status: u16, message: String },

    #[error("stream error: {0}")]
    Stream(String),

    #[error("stream error ({kind}): {message}")]
    StreamError {
        kind: StreamErrorKind,
        message: String,
    },

    #[error("timeout: {0}")]
    Timeout(String),

    #[error("retry exhausted after {attempts} attempts: {last_error}")]
    RetryExhausted {
        attempts: u32,
        last_error: Box<Self>,
    },

    #[error("model not found: {0}")]
    ModelNotFound(String),

    #[error("model unavailable: {0}")]
    ModelUnavailable(String),

    #[error("config error: {0}")]
    Config(String),

    #[error("config watcher error: {0}")]
    ConfigWatcher(String),

    #[error("model router error: {0}")]
    Router(String),

    #[error("task error: {0}")]
    Task(String),

    #[error("executor error: {0}")]
    Executor(String),

    #[error("context error: {0}")]
    Context(String),

    #[error("telemetry error: {0}")]
    Telemetry(String),

    #[error("multiagent error: {0}")]
    Multiagent(String),

    #[error("evaluation error: {0}")]
    Evaluation(String),
}

impl Error {
    /// Returns true if this error is retryable based on its type or status code.
    pub fn is_retryable(&self) -> bool {
        match self {
            Error::RequestFailed { status, .. } => {
                matches!(status, 429 | 500 | 502 | 503 | 504)
            }
            Error::Stream(_) => true,
            Error::StreamError { kind, .. } => matches!(
                kind,
                StreamErrorKind::HttpFailure
                    | StreamErrorKind::ProtocolError
                    | StreamErrorKind::Internal
            ),
            Error::Timeout(_) => true,
            Error::RateLimited(_) => true,
            _ => false,
        }
    }

    /// Returns true if this error indicates rate limiting.
    pub fn is_rate_limited(&self) -> bool {
        matches!(self, Error::RateLimited(_))
    }

    /// Build a structured streaming error. Prefer this over
    /// `Error::Stream(String)` for code paths that participate in
    /// `complete_with_stream` and the truncate / cancel / fallback flow.
    pub fn stream_error(
        kind: StreamErrorKind,
        message: impl Into<String>,
    ) -> Self {
        Error::StreamError {
            kind,
            message: message.into(),
        }
    }

    /// Map this error to its stable wire-level [`ErrorCode`].
    pub fn code(&self) -> ErrorCode {
        match self {
            Error::NotFound(_) => ErrorCode::NotFound,
            Error::AlreadyExists(_) => ErrorCode::AlreadyExists,
            Error::InvalidItem(_) => ErrorCode::InvalidItem,
            Error::Io(_) => ErrorCode::Io,
            Error::Parse(_) => ErrorCode::Parse,
            Error::Internal(_) => ErrorCode::InternalServerError,
            Error::Unauthorized(_) => ErrorCode::Unauthorized,
            Error::Forbidden(_) => ErrorCode::Forbidden,
            Error::Validation(_) => ErrorCode::ValidationError,
            Error::ToolExecution(_) => ErrorCode::ToolExecutionError,
            Error::Provider(_) => ErrorCode::ProviderError,
            Error::Session(_) => ErrorCode::SessionError,
            Error::Skill(_) => ErrorCode::SkillError,
            Error::Memory(_) => ErrorCode::MemoryError,
            Error::GuardianViolation(_) => ErrorCode::GuardianViolation,
            Error::RateLimited(_) => ErrorCode::RateLimited,
            Error::RequestFailed { .. } => ErrorCode::ProviderError,
            Error::Stream(_) => ErrorCode::Stream,
            Error::StreamError { .. } => ErrorCode::Stream,
            Error::Timeout(_) => ErrorCode::Timeout,
            Error::RetryExhausted { .. } => ErrorCode::RetryExhausted,
            Error::ModelNotFound(_) => ErrorCode::ModelNotFound,
            Error::ModelUnavailable(_) => ErrorCode::ModelUnavailable,
            Error::Config(_) => ErrorCode::ConfigError,
            Error::ConfigWatcher(_) => ErrorCode::ConfigError,
            Error::Router(_) => ErrorCode::RouterError,
            Error::Task(_) => ErrorCode::TaskError,
            Error::Executor(_) => ErrorCode::ExecutorError,
            Error::Context(_) => ErrorCode::ContextError,
            Error::Telemetry(_) => ErrorCode::TelemetryError,
            Error::Multiagent(_) => ErrorCode::MultiagentError,
            Error::Evaluation(_) => ErrorCode::EvaluationError,
            Error::EditConflict { .. } => ErrorCode::EditConflict,
        }
    }
}

impl From<reqwest::Error> for Error {
    fn from(e: reqwest::Error) -> Self {
        if e.is_timeout() {
            Error::Timeout(e.to_string())
        } else if e.is_connect() {
            Error::Stream(e.to_string())
        } else if e.is_request() && e.is_redirect() {
            Error::RequestFailed {
                status: 0,
                message: e.to_string(),
            }
        } else {
            Error::Internal(e.to_string())
        }
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::Parse(e.to_string())
    }
}

impl From<serde_yaml::Error> for Error {
    fn from(e: serde_yaml::Error) -> Self {
        Error::Parse(format!("yaml error: {}", e))
    }
}
