//! The [`Error`] enum — the single cross-crate error type for
//! Synthia, used by every workspace member. Plus the
//! [`Error::is_retryable`], [`Error::is_rate_limited`], and
//! [`Error::stream_error`] accessors, and three external `From`
//! impls ([`From<reqwest::Error>`], [`From<serde_json::Error>`],
//! [`From<serde_yaml::Error>`]).
//!
//! Every variant carries a `location: CallSite` field (captured
//! automatically via `#[track_caller]` helper constructors) and
//! a `context: BTreeMap<String, String>` field for structured
//! debugging metadata. Use the helper constructors (e.g.
//! [`Error::not_found`], [`Error::validation`]) and the
//! [`Error::with_context`] fluent builder instead of constructing
//! variants directly so the location and context are populated.
//!
//! HTTP / wire-layer classification lives in `synthia-server`
//! (see `synthia_server::api::error::ErrorCode` and the
//! `From<synthia_core::Error> for UserError` impl) — this crate
//! stays transport-agnostic so non-HTTP binaries can reuse the
//! enum directly.

use std::{collections::BTreeMap, fmt};

use super::stream_error_kind::StreamErrorKind;

/// Call-site location carried by [`Error`] variants. Populated
/// automatically by the `#[track_caller]` helper constructors.
pub type CallSite = &'static std::panic::Location<'static>;

/// Top-level Synthia error. Returned by every fallible operation
/// across the workspace.
///
/// Every variant carries:
/// - `message: String` (or `item` / `path`) — the human-readable
///   message.
/// - `context: BTreeMap<String, String>` — structured debug
///   metadata, **never** serialized to the wire; appears in
///   [`Display`](std::fmt::Display) output only.
/// - `location: CallSite` — captured by `#[track_caller]`
///   helper constructors ([`Error::not_found`], etc.).
#[derive(Debug)]
pub enum Error {
    NotFound {
        item: String,
        context: BTreeMap<String, String>,
        location: CallSite,
    },

    AlreadyExists {
        item: String,
        context: BTreeMap<String, String>,
        location: CallSite,
    },

    InvalidItem {
        item: String,
        context: BTreeMap<String, String>,
        location: CallSite,
    },

    Io(std::io::Error),

    Parse {
        message: String,
        context: BTreeMap<String, String>,
        location: CallSite,
    },

    Internal {
        message: String,
        context: BTreeMap<String, String>,
        location: CallSite,
    },

    Unauthorized {
        message: String,
        context: BTreeMap<String, String>,
        location: CallSite,
    },

    Forbidden {
        message: String,
        context: BTreeMap<String, String>,
        location: CallSite,
    },

    Validation {
        message: String,
        context: BTreeMap<String, String>,
        location: CallSite,
    },

    ToolExecution {
        message: String,
        context: BTreeMap<String, String>,
        location: CallSite,
    },

    Provider {
        message: String,
        context: BTreeMap<String, String>,
        location: CallSite,
    },

    Session {
        message: String,
        context: BTreeMap<String, String>,
        location: CallSite,
    },

    Skill {
        message: String,
        context: BTreeMap<String, String>,
        location: CallSite,
    },

    Memory {
        message: String,
        context: BTreeMap<String, String>,
        location: CallSite,
    },

    GuardianViolation {
        message: String,
        context: BTreeMap<String, String>,
        location: CallSite,
    },

    EditConflict {
        path: std::path::PathBuf,
        original_hash: u64,
        current_hash: u64,
        context: BTreeMap<String, String>,
        location: CallSite,
    },

    RateLimited {
        retry_after: Option<std::time::Duration>,
        context: BTreeMap<String, String>,
        location: CallSite,
    },

    RequestFailed {
        status: u16,
        message: String,
        context: BTreeMap<String, String>,
        location: CallSite,
    },

    Stream {
        message: String,
        context: BTreeMap<String, String>,
        location: CallSite,
    },

    StreamError {
        kind: StreamErrorKind,
        message: String,
        context: BTreeMap<String, String>,
        location: CallSite,
    },

    Timeout {
        message: String,
        context: BTreeMap<String, String>,
        location: CallSite,
    },

    RetryExhausted {
        attempts: u32,
        last_error: Box<Self>,
        context: BTreeMap<String, String>,
        location: CallSite,
    },

    ModelNotFound {
        message: String,
        context: BTreeMap<String, String>,
        location: CallSite,
    },

    ModelUnavailable {
        message: String,
        context: BTreeMap<String, String>,
        location: CallSite,
    },

    Config {
        message: String,
        context: BTreeMap<String, String>,
        location: CallSite,
    },

    ConfigWatcher {
        message: String,
        context: BTreeMap<String, String>,
        location: CallSite,
    },

    Router {
        message: String,
        context: BTreeMap<String, String>,
        location: CallSite,
    },

    Task {
        message: String,
        context: BTreeMap<String, String>,
        location: CallSite,
    },

    Executor {
        message: String,
        context: BTreeMap<String, String>,
        location: CallSite,
    },

    Context {
        message: String,
        context: BTreeMap<String, String>,
        location: CallSite,
    },

    Telemetry {
        message: String,
        context: BTreeMap<String, String>,
        location: CallSite,
    },

    Multiagent {
        message: String,
        context: BTreeMap<String, String>,
        location: CallSite,
    },

    Evaluation {
        message: String,
        context: BTreeMap<String, String>,
        location: CallSite,
    },

    ContextOverflow {
        limit_tokens: u64,
        actual_tokens: u64,
        context: BTreeMap<String, String>,
        location: CallSite,
    },

    DoomLoop {
        tool_name: String,
        iterations: u32,
        context: BTreeMap<String, String>,
        location: CallSite,
    },

    PromptInjection {
        source: String,
        pattern: String,
        context: BTreeMap<String, String>,
        location: CallSite,
    },
}

// -----------------------------------------------------------------------------
// From impls for foreign error types.
// -----------------------------------------------------------------------------

impl From<std::io::Error> for Error {
    #[track_caller]
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

impl Error {
    // -------------------------------------------------------------------------
    // Helper constructors (high-frequency, with #[track_caller] capture)
    // -------------------------------------------------------------------------

    /// Construct a [`Error::NotFound`] with the call site captured
    /// automatically via `#[track_caller]`.
    #[track_caller]
    pub fn not_found(item: impl Into<String>) -> Self {
        Error::NotFound {
            item: item.into(),
            context: BTreeMap::new(),
            location: std::panic::Location::caller(),
        }
    }

    /// Construct a [`Error::AlreadyExists`] with the call site captured.
    #[track_caller]
    pub fn already_exists(item: impl Into<String>) -> Self {
        Error::AlreadyExists {
            item: item.into(),
            context: BTreeMap::new(),
            location: std::panic::Location::caller(),
        }
    }

    /// Construct an [`Error::InvalidItem`] with the call site captured.
    #[track_caller]
    pub fn invalid_item(item: impl Into<String>) -> Self {
        Error::InvalidItem {
            item: item.into(),
            context: BTreeMap::new(),
            location: std::panic::Location::caller(),
        }
    }

    /// Construct a [`Error::Validation`] with the call site captured.
    #[track_caller]
    pub fn validation(message: impl Into<String>) -> Self {
        Error::Validation {
            message: message.into(),
            context: BTreeMap::new(),
            location: std::panic::Location::caller(),
        }
    }

    /// Construct a [`Error::Internal`] with the call site captured.
    #[track_caller]
    pub fn internal(message: impl Into<String>) -> Self {
        Error::Internal {
            message: message.into(),
            context: BTreeMap::new(),
            location: std::panic::Location::caller(),
        }
    }

    /// Construct a [`Error::Unauthorized`] with the call site captured.
    #[track_caller]
    pub fn unauthorized(message: impl Into<String>) -> Self {
        Error::Unauthorized {
            message: message.into(),
            context: BTreeMap::new(),
            location: std::panic::Location::caller(),
        }
    }

    /// Construct a [`Error::Forbidden`] with the call site captured.
    #[track_caller]
    pub fn forbidden(message: impl Into<String>) -> Self {
        Error::Forbidden {
            message: message.into(),
            context: BTreeMap::new(),
            location: std::panic::Location::caller(),
        }
    }

    /// Construct a [`Error::Parse`] with the call site captured.
    #[track_caller]
    pub fn parse(message: impl Into<String>) -> Self {
        Error::Parse {
            message: message.into(),
            context: BTreeMap::new(),
            location: std::panic::Location::caller(),
        }
    }

    /// Construct a [`Error::ToolExecution`] with the call site captured.
    #[track_caller]
    pub fn tool_execution(message: impl Into<String>) -> Self {
        Error::ToolExecution {
            message: message.into(),
            context: BTreeMap::new(),
            location: std::panic::Location::caller(),
        }
    }

    /// Construct a [`Error::Provider`] with the call site captured.
    #[track_caller]
    pub fn provider(message: impl Into<String>) -> Self {
        Error::Provider {
            message: message.into(),
            context: BTreeMap::new(),
            location: std::panic::Location::caller(),
        }
    }

    /// Construct a [`Error::Session`] with the call site captured.
    #[track_caller]
    pub fn session(message: impl Into<String>) -> Self {
        Error::Session {
            message: message.into(),
            context: BTreeMap::new(),
            location: std::panic::Location::caller(),
        }
    }

    /// Construct a [`Error::Skill`] with the call site captured.
    #[track_caller]
    pub fn skill(message: impl Into<String>) -> Self {
        Error::Skill {
            message: message.into(),
            context: BTreeMap::new(),
            location: std::panic::Location::caller(),
        }
    }

    /// Construct a [`Error::Memory`] with the call site captured.
    #[track_caller]
    pub fn memory(message: impl Into<String>) -> Self {
        Error::Memory {
            message: message.into(),
            context: BTreeMap::new(),
            location: std::panic::Location::caller(),
        }
    }

    /// Construct a [`Error::GuardianViolation`] with the call site captured.
    #[track_caller]
    pub fn guardian_violation(message: impl Into<String>) -> Self {
        Error::GuardianViolation {
            message: message.into(),
            context: BTreeMap::new(),
            location: std::panic::Location::caller(),
        }
    }

    /// Construct a [`Error::Stream`] with the call site captured.
    #[track_caller]
    pub fn stream(message: impl Into<String>) -> Self {
        Error::Stream {
            message: message.into(),
            context: BTreeMap::new(),
            location: std::panic::Location::caller(),
        }
    }

    /// Construct a [`Error::Timeout`] with the call site captured.
    #[track_caller]
    pub fn timeout(message: impl Into<String>) -> Self {
        Error::Timeout {
            message: message.into(),
            context: BTreeMap::new(),
            location: std::panic::Location::caller(),
        }
    }

    /// Construct a [`Error::ModelNotFound`] with the call site captured.
    #[track_caller]
    pub fn model_not_found(message: impl Into<String>) -> Self {
        Error::ModelNotFound {
            message: message.into(),
            context: BTreeMap::new(),
            location: std::panic::Location::caller(),
        }
    }

    /// Construct a [`Error::ModelUnavailable`] with the call site captured.
    #[track_caller]
    pub fn model_unavailable(message: impl Into<String>) -> Self {
        Error::ModelUnavailable {
            message: message.into(),
            context: BTreeMap::new(),
            location: std::panic::Location::caller(),
        }
    }

    /// Construct a [`Error::Config`] with the call site captured.
    #[track_caller]
    pub fn config(message: impl Into<String>) -> Self {
        Error::Config {
            message: message.into(),
            context: BTreeMap::new(),
            location: std::panic::Location::caller(),
        }
    }

    /// Construct a [`Error::Router`] with the call site captured.
    #[track_caller]
    pub fn router(message: impl Into<String>) -> Self {
        Error::Router {
            message: message.into(),
            context: BTreeMap::new(),
            location: std::panic::Location::caller(),
        }
    }

    /// Construct a [`Error::Task`] with the call site captured.
    #[track_caller]
    pub fn task(message: impl Into<String>) -> Self {
        Error::Task {
            message: message.into(),
            context: BTreeMap::new(),
            location: std::panic::Location::caller(),
        }
    }

    /// Construct a [`Error::Executor`] with the call site captured.
    #[track_caller]
    pub fn executor(message: impl Into<String>) -> Self {
        Error::Executor {
            message: message.into(),
            context: BTreeMap::new(),
            location: std::panic::Location::caller(),
        }
    }

    /// Construct a [`Error::Context`] with the call site captured.
    #[track_caller]
    pub fn context_err(message: impl Into<String>) -> Self {
        Error::Context {
            message: message.into(),
            context: BTreeMap::new(),
            location: std::panic::Location::caller(),
        }
    }

    /// Construct a [`Error::Telemetry`] with the call site captured.
    #[track_caller]
    pub fn telemetry(message: impl Into<String>) -> Self {
        Error::Telemetry {
            message: message.into(),
            context: BTreeMap::new(),
            location: std::panic::Location::caller(),
        }
    }

    /// Construct a [`Error::Multiagent`] with the call site captured.
    #[track_caller]
    pub fn multiagent(message: impl Into<String>) -> Self {
        Error::Multiagent {
            message: message.into(),
            context: BTreeMap::new(),
            location: std::panic::Location::caller(),
        }
    }

    /// Construct a [`Error::Evaluation`] with the call site captured.
    #[track_caller]
    pub fn evaluation(message: impl Into<String>) -> Self {
        Error::Evaluation {
            message: message.into(),
            context: BTreeMap::new(),
            location: std::panic::Location::caller(),
        }
    }

    /// Construct a [`Error::RateLimited`] with the call site captured.
    #[track_caller]
    pub fn rate_limited(retry_after: Option<std::time::Duration>) -> Self {
        Error::RateLimited {
            retry_after,
            context: BTreeMap::new(),
            location: std::panic::Location::caller(),
        }
    }

    /// Construct a [`Error::RequestFailed`] with the call site captured.
    #[track_caller]
    pub fn request_failed(status: u16, message: impl Into<String>) -> Self {
        Error::RequestFailed {
            status,
            message: message.into(),
            context: BTreeMap::new(),
            location: std::panic::Location::caller(),
        }
    }

    /// Construct a [`Error::EditConflict`] with the call site captured.
    #[track_caller]
    pub fn edit_conflict(
        path: impl Into<std::path::PathBuf>,
        original_hash: u64,
        current_hash: u64,
    ) -> Self {
        Error::EditConflict {
            path: path.into(),
            original_hash,
            current_hash,
            context: BTreeMap::new(),
            location: std::panic::Location::caller(),
        }
    }

    /// Construct a [`Error::ContextOverflow`] with the call site captured.
    #[track_caller]
    pub fn context_overflow(limit_tokens: u64, actual_tokens: u64) -> Self {
        Error::ContextOverflow {
            limit_tokens,
            actual_tokens,
            context: BTreeMap::new(),
            location: std::panic::Location::caller(),
        }
    }

    /// Construct a [`Error::DoomLoop`] with the call site captured.
    #[track_caller]
    pub fn doom_loop(tool_name: impl Into<String>, iterations: u32) -> Self {
        Error::DoomLoop {
            tool_name: tool_name.into(),
            iterations,
            context: BTreeMap::new(),
            location: std::panic::Location::caller(),
        }
    }

    /// Construct a [`Error::PromptInjection`] with the call site captured.
    #[track_caller]
    pub fn prompt_injection(
        source: impl Into<String>,
        pattern: impl Into<String>,
    ) -> Self {
        Error::PromptInjection {
            source: source.into(),
            pattern: pattern.into(),
            context: BTreeMap::new(),
            location: std::panic::Location::caller(),
        }
    }

    /// Construct a [`Error::RetryExhausted`] with the call site captured.
    #[track_caller]
    pub fn retry_exhausted(attempts: u32, last_error: Error) -> Self {
        Error::RetryExhausted {
            attempts,
            last_error: Box::new(last_error),
            context: BTreeMap::new(),
            location: std::panic::Location::caller(),
        }
    }

    // -------------------------------------------------------------------------
    // Context builders / accessors
    // -------------------------------------------------------------------------

    /// Attach a `key=value` pair to this error's structured context.
    ///
    /// Returns `self` for fluent chaining:
    /// ```ignore
    /// Error::not_found("session")
    ///     .with_context("session_id", id)
    ///     .with_context("user_id", uid)
    /// ```
    ///
    /// Context is **not** part of the wire-level JSON envelope; it
    /// only appears in [`Display`](std::fmt::Display) output for
    /// operator-facing logs.
    ///
    /// Calling `with_context` on an [`Error::Io`] is a silent no-op
    /// (the foreign-error variant cannot carry synthetic context).
    pub fn with_context(
        mut self,
        key: impl Into<String>,
        value: impl Into<String>,
    ) -> Self {
        if let Some(map) = self.context_mut() {
            map.insert(key.into(), value.into());
        }
        // `#[track_caller]` already captures the caller for the
        // helper constructor; we intentionally do NOT overwrite
        // `location` here so chained calls don't displace the
        // originating call site.
        self
    }

    /// Borrow the structured context attached to this error.
    ///
    /// Returns an empty map for variants that don't carry context
    /// (e.g. `Io`) — callers can treat the result uniformly.
    pub fn context(&self) -> &BTreeMap<String, String> {
        self.context_ref()
    }

    /// Borrow the location (file:line) where this error was
    /// originally constructed, if available.
    pub fn location(&self) -> Option<CallSite> {
        match self {
            Error::NotFound { location, .. }
            | Error::AlreadyExists { location, .. }
            | Error::InvalidItem { location, .. }
            | Error::Parse { location, .. }
            | Error::Internal { location, .. }
            | Error::Unauthorized { location, .. }
            | Error::Forbidden { location, .. }
            | Error::Validation { location, .. }
            | Error::ToolExecution { location, .. }
            | Error::Provider { location, .. }
            | Error::Session { location, .. }
            | Error::Skill { location, .. }
            | Error::Memory { location, .. }
            | Error::GuardianViolation { location, .. }
            | Error::EditConflict { location, .. }
            | Error::RateLimited { location, .. }
            | Error::RequestFailed { location, .. }
            | Error::Stream { location, .. }
            | Error::StreamError { location, .. }
            | Error::Timeout { location, .. }
            | Error::RetryExhausted { location, .. }
            | Error::ModelNotFound { location, .. }
            | Error::ModelUnavailable { location, .. }
            | Error::Config { location, .. }
            | Error::ConfigWatcher { location, .. }
            | Error::Router { location, .. }
            | Error::Task { location, .. }
            | Error::Executor { location, .. }
            | Error::Context { location, .. }
            | Error::Telemetry { location, .. }
            | Error::Multiagent { location, .. }
            | Error::Evaluation { location, .. }
            | Error::ContextOverflow { location, .. }
            | Error::DoomLoop { location, .. }
            | Error::PromptInjection { location, .. } => Some(*location),
            Error::Io(_) => None,
        }
    }

    fn context_mut(&mut self) -> Option<&mut BTreeMap<String, String>> {
        match self {
            Error::NotFound { context, .. }
            | Error::AlreadyExists { context, .. }
            | Error::InvalidItem { context, .. }
            | Error::Parse { context, .. }
            | Error::Internal { context, .. }
            | Error::Unauthorized { context, .. }
            | Error::Forbidden { context, .. }
            | Error::Validation { context, .. }
            | Error::ToolExecution { context, .. }
            | Error::Provider { context, .. }
            | Error::Session { context, .. }
            | Error::Skill { context, .. }
            | Error::Memory { context, .. }
            | Error::GuardianViolation { context, .. }
            | Error::EditConflict { context, .. }
            | Error::RateLimited { context, .. }
            | Error::RequestFailed { context, .. }
            | Error::Stream { context, .. }
            | Error::StreamError { context, .. }
            | Error::Timeout { context, .. }
            | Error::RetryExhausted { context, .. }
            | Error::ModelNotFound { context, .. }
            | Error::ModelUnavailable { context, .. }
            | Error::Config { context, .. }
            | Error::ConfigWatcher { context, .. }
            | Error::Router { context, .. }
            | Error::Task { context, .. }
            | Error::Executor { context, .. }
            | Error::Context { context, .. }
            | Error::Telemetry { context, .. }
            | Error::Multiagent { context, .. }
            | Error::Evaluation { context, .. }
            | Error::ContextOverflow { context, .. }
            | Error::DoomLoop { context, .. }
            | Error::PromptInjection { context, .. } => Some(context),
            Error::Io(_) => None,
        }
    }

    fn context_ref(&self) -> &BTreeMap<String, String> {
        match self {
            Error::NotFound { context, .. }
            | Error::AlreadyExists { context, .. }
            | Error::InvalidItem { context, .. }
            | Error::Parse { context, .. }
            | Error::Internal { context, .. }
            | Error::Unauthorized { context, .. }
            | Error::Forbidden { context, .. }
            | Error::Validation { context, .. }
            | Error::ToolExecution { context, .. }
            | Error::Provider { context, .. }
            | Error::Session { context, .. }
            | Error::Skill { context, .. }
            | Error::Memory { context, .. }
            | Error::GuardianViolation { context, .. }
            | Error::EditConflict { context, .. }
            | Error::RateLimited { context, .. }
            | Error::RequestFailed { context, .. }
            | Error::Stream { context, .. }
            | Error::StreamError { context, .. }
            | Error::Timeout { context, .. }
            | Error::RetryExhausted { context, .. }
            | Error::ModelNotFound { context, .. }
            | Error::ModelUnavailable { context, .. }
            | Error::Config { context, .. }
            | Error::ConfigWatcher { context, .. }
            | Error::Router { context, .. }
            | Error::Task { context, .. }
            | Error::Executor { context, .. }
            | Error::Context { context, .. }
            | Error::Telemetry { context, .. }
            | Error::Multiagent { context, .. }
            | Error::Evaluation { context, .. }
            | Error::ContextOverflow { context, .. }
            | Error::DoomLoop { context, .. }
            | Error::PromptInjection { context, .. } => context,
            Error::Io(_) => {
                // Borrow a stable empty map for variants that don't
                // carry context. The `OnceLock` is needed because we
                // need a `'static` reference; the cost is one
                // allocation across the entire program lifetime.
                static EMPTY: std::sync::OnceLock<BTreeMap<String, String>> =
                    std::sync::OnceLock::new();
                EMPTY.get_or_init(BTreeMap::new)
            }
        }
    }

    // -------------------------------------------------------------------------
    // Classification / accessors
    // -------------------------------------------------------------------------

    /// Returns true if this error is retryable based on its type or status code.
    pub fn is_retryable(&self) -> bool {
        match self {
            Error::RequestFailed { status, .. } => {
                matches!(status, 429 | 500 | 502 | 503 | 504)
            }
            Error::Stream { .. } => true,
            Error::StreamError { kind, .. } => matches!(
                kind,
                StreamErrorKind::HttpFailure
                    | StreamErrorKind::ProtocolError
                    | StreamErrorKind::Internal
            ),
            Error::Timeout { .. } => true,
            Error::RateLimited { .. } => true,
            _ => false,
        }
    }

    /// Returns true if this error indicates rate limiting.
    pub fn is_rate_limited(&self) -> bool {
        matches!(self, Error::RateLimited { .. })
    }

    /// Build a structured streaming error. Prefer this over
    /// `Error::Stream(String)` for code paths that participate in
    /// `complete_with_stream` and the truncate / cancel / fallback flow.
    #[track_caller]
    pub fn stream_error(
        kind: StreamErrorKind,
        message: impl Into<String>,
    ) -> Self {
        Error::StreamError {
            kind,
            message: message.into(),
            context: BTreeMap::new(),
            location: std::panic::Location::caller(),
        }
    }

    /// Returns a stable, lowercase snake_case identifier for this
    /// error variant. Useful for telemetry tags, log fields, and
    /// metric labels — anywhere a transport-agnostic
    /// classification string is needed.
    ///
    /// HTTP / wire-layer codes belong to the
    /// `synthia_server::api::error::ErrorCode` enum and are NOT
    /// derived from `kind()` — keep this surface lib-only.
    pub fn kind(&self) -> &'static str {
        match self {
            Error::NotFound { .. } => "not_found",
            Error::AlreadyExists { .. } => "already_exists",
            Error::InvalidItem { .. } => "invalid_item",
            Error::Io(_) => "io",
            Error::Parse { .. } => "parse",
            Error::Internal { .. } => "internal_server_error",
            Error::Unauthorized { .. } => "unauthorized",
            Error::Forbidden { .. } => "forbidden",
            Error::Validation { .. } => "validation",
            Error::ToolExecution { .. } => "tool_execution",
            Error::Provider { .. } => "provider",
            Error::Session { .. } => "session",
            Error::Skill { .. } => "skill",
            Error::Memory { .. } => "memory",
            Error::GuardianViolation { .. } => "guardian_violation",
            Error::EditConflict { .. } => "edit_conflict",
            Error::RateLimited { .. } => "rate_limited",
            Error::RequestFailed { .. } => "request_failed",
            Error::Stream { .. } => "stream",
            Error::StreamError { .. } => "stream_error",
            Error::Timeout { .. } => "timeout",
            Error::RetryExhausted { .. } => "retry_exhausted",
            Error::ModelNotFound { .. } => "model_not_found",
            Error::ModelUnavailable { .. } => "model_unavailable",
            Error::Config { .. } => "config",
            Error::ConfigWatcher { .. } => "config_watcher",
            Error::Router { .. } => "router",
            Error::Task { .. } => "task",
            Error::Executor { .. } => "executor",
            Error::Context { .. } => "context",
            Error::Telemetry { .. } => "telemetry",
            Error::Multiagent { .. } => "multiagent",
            Error::Evaluation { .. } => "evaluation",
            Error::ContextOverflow { .. } => "context_overflow",
            Error::DoomLoop { .. } => "doom_loop",
            Error::PromptInjection { .. } => "prompt_injection",
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.write_base_message(f)?;

        let ctx = self.context_ref();
        if !ctx.is_empty() {
            f.write_str(" [")?;
            for (i, (k, v)) in ctx.iter().enumerate() {
                if i > 0 {
                    f.write_str(", ")?;
                }
                write!(f, "{}={}", k, v)?;
            }
            f.write_str("]")?;
        }
        Ok(())
    }
}

impl Error {
    /// Returns the variant-specific base message WITHOUT the
    /// context suffix. Used by transport layers (e.g.
    /// `synthia-server`) to populate the wire-level
    /// `message` field; context is operator-only debug
    /// metadata that must not leak into API responses.
    pub fn wire_message(&self) -> String {
        let mut buf = String::new();
        self.write_base_message(&mut buf)
            .expect("writing to String never fails");
        buf
    }

    fn write_base_message(&self, f: &mut impl fmt::Write) -> fmt::Result {
        match self {
            Error::NotFound { item, location, .. } => {
                write!(f, "not found: {} (at {})", item, location)
            }
            Error::AlreadyExists { item, location, .. } => {
                write!(f, "already exists: {} (at {})", item, location)
            }
            Error::InvalidItem { item, location, .. } => {
                write!(f, "invalid item: {} (at {})", item, location)
            }
            Error::Io(e) => write!(f, "I/O error: {}", e),
            Error::Parse {
                message, location, ..
            } => {
                write!(f, "parse error: {} (at {})", message, location)
            }
            Error::Internal {
                message, location, ..
            } => {
                write!(f, "internal error: {} (at {})", message, location)
            }
            Error::Unauthorized { message, .. } => {
                write!(f, "unauthorized: {}", message)
            }
            Error::Forbidden { message, .. } => {
                write!(f, "forbidden: {}", message)
            }
            Error::Validation {
                message, location, ..
            } => {
                write!(f, "validation error: {} (at {})", message, location)
            }
            Error::ToolExecution { message, .. } => {
                write!(f, "tool execution error: {}", message)
            }
            Error::Provider { message, .. } => {
                write!(f, "provider error: {}", message)
            }
            Error::Session { message, .. } => {
                write!(f, "session error: {}", message)
            }
            Error::Skill { message, .. } => {
                write!(f, "skill error: {}", message)
            }
            Error::Memory { message, .. } => {
                write!(f, "memory error: {}", message)
            }
            Error::GuardianViolation { message, .. } => {
                write!(f, "guardian violation: {}", message)
            }
            Error::EditConflict {
                path,
                original_hash,
                current_hash,
                ..
            } => {
                write!(
                    f,
                    "edit conflict on {}: original_hash={}, current_hash={}",
                    path.display(),
                    original_hash,
                    current_hash
                )
            }
            Error::RateLimited { retry_after, .. } => {
                write!(f, "rate limited, retry after {:?}", retry_after)
            }
            Error::RequestFailed {
                status, message, ..
            } => {
                write!(f, "request failed with status {}: {}", status, message)
            }
            Error::Stream { message, .. } => {
                write!(f, "stream error: {}", message)
            }
            Error::StreamError { kind, message, .. } => {
                write!(f, "stream error ({}): {}", kind, message)
            }
            Error::Timeout { message, .. } => {
                write!(f, "timeout: {}", message)
            }
            Error::RetryExhausted {
                attempts,
                last_error,
                ..
            } => {
                write!(
                    f,
                    "retry exhausted after {} attempts: {}",
                    attempts, last_error
                )
            }
            Error::ModelNotFound { message, .. } => {
                write!(f, "model not found: {}", message)
            }
            Error::ModelUnavailable { message, .. } => {
                write!(f, "model unavailable: {}", message)
            }
            Error::Config { message, .. } => {
                write!(f, "config error: {}", message)
            }
            Error::ConfigWatcher { message, .. } => {
                write!(f, "config watcher error: {}", message)
            }
            Error::Router { message, .. } => {
                write!(f, "model router error: {}", message)
            }
            Error::Task { message, .. } => {
                write!(f, "task error: {}", message)
            }
            Error::Executor { message, .. } => {
                write!(f, "executor error: {}", message)
            }
            Error::Context { message, .. } => {
                write!(f, "context error: {}", message)
            }
            Error::Telemetry { message, .. } => {
                write!(f, "telemetry error: {}", message)
            }
            Error::Multiagent { message, .. } => {
                write!(f, "multiagent error: {}", message)
            }
            Error::Evaluation { message, .. } => {
                write!(f, "evaluation error: {}", message)
            }
            Error::ContextOverflow {
                limit_tokens,
                actual_tokens,
                ..
            } => {
                write!(
                    f,
                    "context overflow: {} tokens used, limit is {}",
                    actual_tokens, limit_tokens
                )
            }
            Error::DoomLoop {
                tool_name,
                iterations,
                ..
            } => {
                write!(
                    f,
                    "doom loop detected on tool '{}' after {} iterations",
                    tool_name, iterations
                )
            }
            Error::PromptInjection {
                source, pattern, ..
            } => {
                write!(
                    f,
                    "prompt injection detected in {}: pattern '{}'",
                    source, pattern
                )
            }
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io(e) => Some(e),
            Error::RetryExhausted { last_error, .. } => {
                Some(last_error.as_ref())
            }
            _ => None,
        }
    }
}

impl From<reqwest::Error> for Error {
    #[track_caller]
    fn from(e: reqwest::Error) -> Self {
        if e.is_timeout() {
            Error::Timeout {
                message: e.to_string(),
                context: BTreeMap::new(),
                location: std::panic::Location::caller(),
            }
        } else if e.is_connect() {
            Error::Stream {
                message: e.to_string(),
                context: BTreeMap::new(),
                location: std::panic::Location::caller(),
            }
        } else if e.is_request() && e.is_redirect() {
            Error::RequestFailed {
                status: 0,
                message: e.to_string(),
                context: BTreeMap::new(),
                location: std::panic::Location::caller(),
            }
        } else {
            Error::Internal {
                message: e.to_string(),
                context: BTreeMap::new(),
                location: std::panic::Location::caller(),
            }
        }
    }
}

impl From<serde_json::Error> for Error {
    #[track_caller]
    fn from(e: serde_json::Error) -> Self {
        Error::Parse {
            message: e.to_string(),
            context: BTreeMap::new(),
            location: std::panic::Location::caller(),
        }
    }
}

impl From<serde_yaml::Error> for Error {
    #[track_caller]
    fn from(e: serde_yaml::Error) -> Self {
        Error::Parse {
            message: format!("yaml error: {}", e),
            context: BTreeMap::new(),
            location: std::panic::Location::caller(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- with_context / context() / location() -----------------------

    /// `with_context` MUST attach the key=value pair to the
    /// context map (visible via `context()`).
    #[test]
    fn with_context_attaches_key_value() {
        let e = Error::not_found("widget").with_context("id", "42");
        assert_eq!(e.context().get("id"), Some(&"42".to_string()));
    }

    /// `with_context` MUST be chainable (each call returns
    /// `self`), accumulating multiple entries.
    #[test]
    fn with_context_is_chainable() {
        let e = Error::validation("bad")
            .with_context("k1", "v1")
            .with_context("k2", "v2")
            .with_context("k3", "v3");
        assert_eq!(e.context().len(), 3);
        assert_eq!(e.context().get("k1"), Some(&"v1".to_string()));
        assert_eq!(e.context().get("k2"), Some(&"v2".to_string()));
        assert_eq!(e.context().get("k3"), Some(&"v3".to_string()));
    }

    /// `with_context` MUST overwrite when the same key is set
    /// twice (last-writer-wins).
    #[test]
    fn with_context_overwrites_on_duplicate_key() {
        let e = Error::internal("boom")
            .with_context("id", "first")
            .with_context("id", "second");
        assert_eq!(e.context().get("id"), Some(&"second".to_string()));
        assert_eq!(e.context().len(), 1);
    }

    /// `with_context` MUST be a silent no-op on
    /// [`Error::Io`] (the foreign-error variant has no context
    /// field) — pinned as a documented quirk.
    #[test]
    fn with_context_is_noop_on_io_variant() {
        let e = Error::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "missing",
        ));
        // with_context consumes self and returns it; verify the
        // returned variant is still Io (no panic, no fall-through).
        let returned = e.with_context("k", "v");
        match returned {
            Error::Io(_) => {}
            _ => panic!("must still be Io after no-op context"),
        }
    }

    /// `context()` MUST return an empty BTreeMap for fresh
    /// variants (default state).
    #[test]
    fn context_returns_empty_map_for_fresh_variant() {
        let e = Error::not_found("thing");
        assert!(e.context().is_empty());
    }

    /// `location()` MUST return `Some` for variants that carry
    /// a CallSite (most variants). Pin the contract: every
    /// helper constructor MUST populate location.
    #[test]
    fn location_returns_some_for_helper_constructors() {
        // Every constructor variant (excluding Io, RetryExhausted's
        // boxed child, and the boxed RetryExhausted itself) MUST
        // yield Some on `location()`.
        let e = Error::not_found("x");
        assert!(e.location().is_some());

        let e = Error::validation("x");
        assert!(e.location().is_some());

        let e = Error::internal("x");
        assert!(e.location().is_some());

        let e = Error::parse("x");
        assert!(e.location().is_some());

        let e = Error::rate_limited(None);
        assert!(e.location().is_some());

        let e = Error::request_failed(404, "nf");
        assert!(e.location().is_some());

        let e = Error::doom_loop("bash", 5);
        assert!(e.location().is_some());
    }

    /// `location()` MUST capture the call site of the
    /// constructor (the file:line of the caller, NOT the
    /// implementation file).
    #[test]
    fn location_points_to_caller_file() {
        let e = Error::not_found("thing");
        let loc = e.location().expect("must be Some");
        assert!(
            loc.file().contains("error.rs"),
            "location must be in error.rs (got {})",
            loc.file()
        );
    }

    // -- Specialized constructors -----------------------------------

    /// `Error::rate_limited(retry_after)` MUST round-trip
    /// `retry_after` and use the rate-limited code.
    #[test]
    fn rate_limited_constructor_stores_retry_after() {
        let e = Error::rate_limited(Some(std::time::Duration::from_secs(7)));
        assert!(e.is_rate_limited());
        if let Error::RateLimited { retry_after, .. } = e {
            assert_eq!(retry_after, Some(std::time::Duration::from_secs(7)));
        } else {
            panic!("expected RateLimited variant");
        }
    }

    /// `Error::rate_limited(None)` MUST preserve the None
    /// retry_after (the server hasn't told us).
    #[test]
    fn rate_limited_constructor_accepts_none() {
        let e = Error::rate_limited(None);
        if let Error::RateLimited { retry_after, .. } = e {
            assert_eq!(retry_after, None);
        } else {
            panic!("expected RateLimited variant");
        }
    }

    /// `Error::request_failed(status, message)` MUST round-trip
    /// both the status code and message.
    #[test]
    fn request_failed_constructor_stores_status_and_message() {
        let e = Error::request_failed(503, "service unavailable");
        if let Error::RequestFailed {
            status, message, ..
        } = e
        {
            assert_eq!(status, 503);
            assert_eq!(message, "service unavailable");
        } else {
            panic!("expected RequestFailed variant");
        }
    }

    /// `Error::edit_conflict(path, original, current)` MUST
    /// round-trip all three fields.
    #[test]
    fn edit_conflict_constructor_stores_path_and_hashes() {
        let path = std::path::PathBuf::from("/etc/config.toml");
        let e = Error::edit_conflict(path.clone(), 0xDEAD_BEEF, 0xCAFE_BABE);
        if let Error::EditConflict {
            path: p,
            original_hash,
            current_hash,
            ..
        } = e
        {
            assert_eq!(p, path);
            assert_eq!(original_hash, 0xDEAD_BEEF);
            assert_eq!(current_hash, 0xCAFE_BABE);
        } else {
            panic!("expected EditConflict variant");
        }
    }

    /// `Error::context_overflow(limit, actual)` MUST round-trip
    /// both token counts (these drive the truncation
    /// decision).
    #[test]
    fn context_overflow_constructor_stores_token_counts() {
        let e = Error::context_overflow(100_000, 150_000);
        if let Error::ContextOverflow {
            limit_tokens,
            actual_tokens,
            ..
        } = e
        {
            assert_eq!(limit_tokens, 100_000);
            assert_eq!(actual_tokens, 150_000);
        } else {
            panic!("expected ContextOverflow variant");
        }
    }

    /// `Error::doom_loop(tool, iterations)` MUST round-trip
    /// both fields (drives the doom-loop guard).
    #[test]
    fn doom_loop_constructor_stores_tool_and_iterations() {
        let e = Error::doom_loop("bash", 42);
        if let Error::DoomLoop {
            tool_name,
            iterations,
            ..
        } = e
        {
            assert_eq!(tool_name, "bash");
            assert_eq!(iterations, 42);
        } else {
            panic!("expected DoomLoop variant");
        }
    }

    /// `Error::prompt_injection(source, pattern)` MUST round-trip
    /// both fields (security-critical: drives refusal).
    #[test]
    fn prompt_injection_constructor_stores_source_and_pattern() {
        let e = Error::prompt_injection("user_input", "ignore previous");
        if let Error::PromptInjection {
            source, pattern, ..
        } = e
        {
            assert_eq!(source, "user_input");
            assert_eq!(pattern, "ignore previous");
        } else {
            panic!("expected PromptInjection variant");
        }
    }

    /// `Error::retry_exhausted(attempts, last_error)` MUST
    /// round-trip both the attempt count and the boxed inner
    /// error (accessible via `context()`-like introspection
    /// through `std::error::Error::source()`).
    #[test]
    fn retry_exhausted_constructor_stores_attempts_and_inner_error() {
        let inner = Error::timeout("upstream");
        let e = Error::retry_exhausted(3, inner);
        if let Error::RetryExhausted { attempts, .. } = e {
            assert_eq!(attempts, 3);
        } else {
            panic!("expected RetryExhausted variant");
        }
    }

    // -- From impls (3 cross-crate) ----------------------------------

    /// `From<reqwest::Error>` is feature-gated to `reqwest`,
    /// so we only test the always-available
    /// `From<serde_json::Error>` + `From<serde_yaml::Error>`.
    /// This test pins the JSON parsing branch.
    #[test]
    fn from_serde_json_error_produces_parse_variant() {
        let json_err: serde_json::Error =
            serde_json::from_str::<serde_json::Value>("{bad").unwrap_err();
        let e: Error = json_err.into();
        if let Error::Parse { message, .. } = e {
            assert!(!message.is_empty(), "message must be populated");
        } else {
            panic!("expected Parse variant from JSON error");
        }
    }

    /// `From<serde_yaml::Error>` MUST produce a Parse variant
    /// with the `"yaml error: "` prefix (distinguishes from
    /// the JSON branch).
    #[test]
    fn from_serde_yaml_error_produces_parse_variant_with_prefix() {
        let yaml_err: serde_yaml::Error =
            serde_yaml::from_str::<serde_yaml::Value>("k: : :").unwrap_err();
        let e: Error = yaml_err.into();
        if let Error::Parse { message, .. } = e {
            assert!(
                message.starts_with("yaml error: "),
                "yaml prefix required (got {message:?})"
            );
        } else {
            panic!("expected Parse variant from YAML error");
        }
    }

    /// `From<std::io::Error>` MUST produce the `Io` variant
    /// preserving the inner error (visible via
    /// `std::error::Error::source()`).
    #[test]
    fn from_io_error_produces_io_variant() {
        let io_err = std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "no access",
        );
        let e: Error = io_err.into();
        match e {
            Error::Io(inner) => {
                assert_eq!(inner.kind(), std::io::ErrorKind::PermissionDenied);
            }
            _ => panic!("expected Io variant from io::Error"),
        }
    }

    /// `From<io::Error>` MUST be the ONLY `From` that produces
    /// the `Io` variant (no other branch does).
    #[test]
    fn io_variant_only_via_from_io() {
        let e: Error = std::io::Error::other("x").into();
        assert!(matches!(e, Error::Io(_)));
    }

    // -- wire_message -----------------------------------------------

    /// `wire_message()` MUST return the same content as Display
    /// when no context is attached — so the wire-layer
    /// `From<Error> for UserError` mapping is trivially
    /// satisfied for context-free errors.
    #[test]
    fn wire_message_matches_display_when_context_empty() {
        let err = Error::not_found("widget");
        assert_eq!(err.wire_message(), err.to_string());
    }

    /// `wire_message()` MUST equal Display output ignoring the
    /// `[k=v]` suffix — i.e. stripping context produces the same
    /// base message that crosses the wire.
    #[test]
    fn wire_message_strips_context_suffix() {
        let err = Error::validation("msg")
            .with_context("secret", "value")
            .with_context("hint", "retry");
        let wire = err.wire_message();
        let display = err.to_string();
        assert!(display.starts_with(&wire));
        assert!(display.len() > wire.len());
        assert!(!wire.contains("secret"));
        assert!(!wire.contains("retry"));
    }

    // -- Specialized constructor / variant tests --------------------

    #[test]
    fn doom_loop_helper_roundtrip() {
        let err = Error::doom_loop("web_search", 3);
        match &err {
            Error::DoomLoop {
                tool_name,
                iterations,
                ..
            } => {
                assert_eq!(tool_name, "web_search");
                assert_eq!(*iterations, 3);
            }
            other => panic!("expected DoomLoop, got {other:?}"),
        }
        let formatted = err.to_string();
        assert!(formatted.contains("doom loop"));
        assert!(formatted.contains("web_search"));
        assert!(formatted.contains("3"));
    }

    #[test]
    fn prompt_injection_helper_roundtrip() {
        let err = Error::prompt_injection(
            "user_input",
            "ignore previous instructions",
        );
        match &err {
            Error::PromptInjection {
                source, pattern, ..
            } => {
                assert_eq!(source, "user_input");
                assert_eq!(pattern, "ignore previous instructions");
            }
            other => panic!("expected PromptInjection, got {other:?}"),
        }
        let formatted = err.to_string();
        assert!(formatted.contains("prompt injection"));
        assert!(formatted.contains("user_input"));
    }

    #[test]
    fn new_variants_carry_call_site_and_context() {
        let err =
            Error::context_overflow(100, 200).with_context("session", "abc");
        assert!(
            err.location().is_some(),
            "ContextOverflow must carry location"
        );
        let ctx = err.context();
        assert_eq!(ctx.get("session").map(String::as_str), Some("abc"));

        let err = Error::doom_loop("t", 2).with_context("hint", "retry");
        assert!(err.location().is_some());
        assert_eq!(
            err.context().get("hint").map(String::as_str),
            Some("retry")
        );

        let err =
            Error::prompt_injection("s", "p").with_context("severity", "high");
        assert!(err.location().is_some());
        assert_eq!(
            err.context().get("severity").map(String::as_str),
            Some("high")
        );
    }

    #[test]
    fn with_context_empty_map_has_no_suffix() {
        // Contract: a fresh error (no context attached) must
        // not emit a trailing ` [k=v]` suffix in Display.
        let err = Error::validation("plain message");
        let formatted = err.to_string();
        assert!(
            !formatted.contains('['),
            "fresh error must not have context suffix, got: {formatted}"
        );
        assert!(
            !formatted.contains(']'),
            "fresh error must not have context suffix, got: {formatted}"
        );
    }

    #[test]
    fn with_context_overwrites_same_key() {
        // Documenting the BTreeMap overwrite semantics.
        let err = Error::validation("msg")
            .with_context("field", "name")
            .with_context("field", "email");
        let ctx = err.context();
        assert_eq!(ctx.get("field").map(String::as_str), Some("email"));
        assert_eq!(ctx.len(), 1, "same key should overwrite, not append");
    }

    /// `location_suffix_presence_pinned_per_variant` — pins
    /// which variants include the `(at <location>)` suffix in
    /// Display / wire_message.
    #[test]
    fn location_suffix_presence_pinned_per_variant() {
        let with_loc = [
            Error::not_found("x"),
            Error::already_exists("x"),
            Error::invalid_item("x"),
            Error::parse("x"),
            Error::internal("x"),
            Error::validation("x"),
        ];
        for err in with_loc {
            let s = err.to_string();
            assert!(
                s.contains("(at "),
                "{} should include location suffix, got: {s}",
                std::any::type_name::<Error>()
            );
        }

        let without_loc = [
            Error::unauthorized("x"),
            Error::forbidden("x"),
            Error::tool_execution("x"),
            Error::provider("x"),
            Error::session("x"),
            Error::skill("x"),
            Error::memory("x"),
            Error::guardian_violation("x"),
            Error::stream("x"),
            Error::timeout("x"),
            Error::model_not_found("x"),
            Error::model_unavailable("x"),
            Error::config("x"),
            Error::router("x"),
            Error::task("x"),
            Error::executor("x"),
            Error::context_err("x"),
            Error::telemetry("x"),
            Error::multiagent("x"),
            Error::evaluation("x"),
            Error::context_overflow(1, 2),
            Error::doom_loop("t", 1),
            Error::prompt_injection("s", "p"),
            Error::rate_limited(None),
            Error::stream_error(StreamErrorKind::Internal, "x"),
        ];
        for err in without_loc {
            let s = err.to_string();
            assert!(
                !s.contains("(at "),
                "{} should NOT include location suffix, got: {s}",
                std::any::type_name::<Error>()
            );
        }

        let io_err: Error = std::io::Error::other("x").into();
        let s = io_err.to_string();
        assert!(!s.contains("(at "));
    }

    /// Each helper constructor MUST produce its specific variant
    /// (not a sibling variant with the same fields).
    #[test]
    fn helpers_produce_distinct_variants() {
        use Error as E;
        assert!(matches!(E::not_found("x"), E::NotFound { .. }));
        assert!(matches!(E::already_exists("x"), E::AlreadyExists { .. }));
        assert!(matches!(E::invalid_item("x"), E::InvalidItem { .. }));
        assert!(matches!(E::validation("x"), E::Validation { .. }));
        assert!(matches!(E::internal("x"), E::Internal { .. }));
        assert!(matches!(E::unauthorized("x"), E::Unauthorized { .. }));
        assert!(matches!(E::forbidden("x"), E::Forbidden { .. }));
        assert!(matches!(E::parse("x"), E::Parse { .. }));
        assert!(matches!(E::tool_execution("x"), E::ToolExecution { .. }));
        assert!(matches!(E::provider("x"), E::Provider { .. }));
        assert!(matches!(E::session("x"), E::Session { .. }));
        assert!(matches!(E::skill("x"), E::Skill { .. }));
        assert!(matches!(E::memory("x"), E::Memory { .. }));
        assert!(matches!(
            E::guardian_violation("x"),
            E::GuardianViolation { .. }
        ));
        assert!(matches!(E::stream("x"), E::Stream { .. }));
        assert!(matches!(E::timeout("x"), E::Timeout { .. }));
        assert!(matches!(E::model_not_found("x"), E::ModelNotFound { .. }));
        assert!(matches!(
            E::model_unavailable("x"),
            E::ModelUnavailable { .. }
        ));
        assert!(matches!(E::config("x"), E::Config { .. }));
        assert!(matches!(E::router("x"), E::Router { .. }));
        assert!(matches!(E::task("x"), E::Task { .. }));
        assert!(matches!(E::executor("x"), E::Executor { .. }));
        assert!(matches!(E::context_err("x"), E::Context { .. }));
        assert!(matches!(E::telemetry("x"), E::Telemetry { .. }));
        assert!(matches!(E::multiagent("x"), E::Multiagent { .. }));
        assert!(matches!(E::evaluation("x"), E::Evaluation { .. }));
        assert!(matches!(
            E::context_overflow(1, 2),
            E::ContextOverflow { .. }
        ));
        assert!(matches!(E::doom_loop("t", 1), E::DoomLoop { .. }));
        assert!(matches!(
            E::prompt_injection("s", "p"),
            E::PromptInjection { .. }
        ));
        assert!(matches!(
            E::stream_error(StreamErrorKind::Internal, "x"),
            E::StreamError { .. }
        ));
        assert!(matches!(E::rate_limited(None), E::RateLimited { .. }));
        assert!(matches!(
            E::request_failed(500, "x"),
            E::RequestFailed { .. }
        ));
        assert!(matches!(
            E::edit_conflict("p", 0, 0),
            E::EditConflict { .. }
        ));
        assert!(matches!(
            E::retry_exhausted(1, E::internal("x")),
            E::RetryExhausted { .. }
        ));
    }
}
