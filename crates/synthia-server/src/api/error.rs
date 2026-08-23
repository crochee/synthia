//! HTTP boundary for `synthia_core::Error` — the single error
//! definition source for the whole workspace.
//!
//! The server owns **no error variants of its own**. Every error
//! raised anywhere in the stack is a [`synthia_core::Error`]; this
//! module is the transport adapter that turns one into an HTTP
//!   + `From<domain::Error>` shape (axum's official error-handling
//!     example, actix-web's `ResponseError`).
//!
//! # `AppError`
//!
//! The boundary error type. Holds the full source chain
//! (`anyhow::Error` wrapping the original `synthia_core::Error`),
//! the resolved HTTP status, the wire code, the human-facing
//! message, and an optional KV context map (request id,
//! authenticated subject, etc. — operator-facing; never crosses
//! the wire).
//!
//! Build directly with [`AppError::new`] when you need explicit
//! control, or rely on the `From<synthia_core::Error>` /
//! `From<std::io::Error>` impls for `?` to wrap domain errors
//! automatically. Add context via [`AppError::with_message`] /
//! [`AppError::with_context`] / [`AppError::merge_context`] in
//! `map_err` blocks.
//!
//! # Wire envelope (flat)
//!
//! ```json
//! { "code": "<snake_case kind>", "message": "<Display>" }
//! ```
//!
//! `message` is the error's full `Display` output (the
//! thiserror-generated, prefix-included message). The `source`
//! chain never crosses the wire; it is logged via `tracing::error!`
//! when the response is produced (operator-only detail).
//!
//! `RateLimited` additionally surfaces its payload as a
//! `Retry-After` header (seconds, minimum 1).

use std::collections::HashMap;

use axum::{
    Json,
    http::{HeaderName, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use serde_json::json;
use synthia_core::Error;

/// HTTP boundary error wrapping the single core error type
/// [`synthia_core::Error`].
///
/// Carries the full source chain via [`anyhow::Error`] so the
/// original cause stays reachable for logging. Constructors
/// ([`AppError::new`], [`AppError::with_code`],
/// [`AppError::with_message`], [`AppError::with_context`],
/// [`AppError::merge_context`]) are fluent and chainable for
/// use in `map_err` blocks.
#[derive(Debug)]
pub struct AppError {
    /// HTTP status the response will carry.
    pub status_code: StatusCode,
    /// Original error chain (wraps the original
    /// `synthia_core::Error` for core failures, or any foreign
    /// error for `From<std::io::Error>` / etc.).
    pub source: anyhow::Error,
    /// Stable wire code (snake_case, e.g. `"not_found"`,
    /// `"internal_server_error"`).
    pub code: &'static str,
    /// Human-facing message that crosses the wire in the envelope.
    pub message: String,
    /// Operator-facing KV context (request id, subject, etc.).
    /// Never crosses the wire — logged via `tracing::error!`.
    pub context: HashMap<String, String>,
}

impl AppError {
    /// Wrap an error source at a specific HTTP status. The
    /// wire code defaults to the canonical HTTP reason text
    /// (e.g. `"Internal Server Error"`) — refine via
    /// [`AppError::with_code`] when you have a more specific
    /// domain code. The message defaults to
    /// `source.to_string()`.
    pub fn new(
        status_code: StatusCode,
        source: impl Into<anyhow::Error>,
    ) -> Self {
        let source = source.into();
        let message = source.to_string();
        // `canonical_reason()` returns a static str for known
        // status codes; fall back to `"UNKNOWN"`.
        let code: &'static str =
            status_code.canonical_reason().unwrap_or("UNKNOWN");
        Self {
            status_code,
            source,
            code,
            message,
            context: HashMap::new(),
        }
    }

    /// Override the wire code. Returns `self` for fluent chaining.
    #[must_use]
    pub fn with_code(mut self, code: &'static str) -> Self {
        self.code = code;
        self
    }

    /// Override the message. Returns `self` for fluent chaining.
    #[must_use]
    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = message.into();
        self
    }

    /// Add a single KV entry to the operator-facing context map.
    /// Returns `self` for fluent chaining.
    #[must_use]
    pub fn with_context(
        mut self,
        key: impl Into<String>,
        value: impl Into<String>,
    ) -> Self {
        self.context.insert(key.into(), value.into());
        self
    }

    /// Merge another KV map into the operator-facing context.
    /// Existing keys are overwritten (last writer wins).
    /// Returns `self` for fluent chaining.
    #[must_use]
    pub fn merge_context(mut self, other: HashMap<String, String>) -> Self {
        self.context.extend(other);
        self
    }
}

impl From<Error> for AppError {
    /// Wrap a domain error. Resolves the HTTP status and the
    /// wire code inline from the variant — no helper table.
    /// The human-facing message comes from the thiserror-generated
    /// variant-appropriate prefix. Variant-specific payload
    /// fields (resource name, retry-after duration, token
    /// counts, …) are folded into the operator-facing
    /// `context` map so they show up in the `tracing::error!`
    /// log line without going on the wire.
    ///
    /// A future variant addition touches exactly one site: add
    /// a new arm with its `(status, code)` pair; the message
    /// comes for free from `Display`, and `payload_context`
    /// picks up the new fields automatically if you wire them
    /// into its match.
    fn from(err: Error) -> Self {
        let message = err.to_string();
        let mut context: HashMap<String, String> = HashMap::new();
        let (status_code, code) = match &err {
            Error::NotFound { item } => {
                context.insert("item".into(), item.clone());
                (StatusCode::NOT_FOUND, "not_found")
            }
            Error::AlreadyExists { item } => {
                context.insert("item".into(), item.clone());
                (StatusCode::CONFLICT, "already_exists")
            }
            Error::InvalidItem { item } => {
                context.insert("item".into(), item.clone());
                (StatusCode::BAD_REQUEST, "invalid_item")
            }
            Error::Parse { .. } => (StatusCode::BAD_REQUEST, "parse"),
            Error::Internal { .. } => {
                (StatusCode::INTERNAL_SERVER_ERROR, "internal_server_error")
            }
            Error::Unauthorized { .. } => {
                (StatusCode::UNAUTHORIZED, "unauthorized")
            }
            Error::Forbidden { .. } => (StatusCode::FORBIDDEN, "forbidden"),
            Error::Validation { .. } => {
                (StatusCode::UNPROCESSABLE_ENTITY, "validation")
            }
            Error::ToolExecution { .. } => {
                (StatusCode::INTERNAL_SERVER_ERROR, "tool_execution")
            }
            Error::Provider { .. } => {
                (StatusCode::BAD_GATEWAY, "request_failed")
            }
            Error::RequestFailed { status, .. } => {
                context.insert("status".into(), status.to_string());
                (StatusCode::BAD_GATEWAY, "request_failed")
            }
            Error::Session { .. } => {
                (StatusCode::INTERNAL_SERVER_ERROR, "session")
            }
            Error::Skill { .. } => (StatusCode::INTERNAL_SERVER_ERROR, "skill"),
            Error::Memory { .. } => {
                (StatusCode::INTERNAL_SERVER_ERROR, "memory")
            }
            Error::GuardianViolation { .. } => {
                (StatusCode::FORBIDDEN, "guardian_violation")
            }
            Error::EditConflict {
                path,
                original_hash,
                current_hash,
            } => {
                context.insert("path".into(), path.display().to_string());
                context
                    .insert("original_hash".into(), original_hash.to_string());
                context.insert("current_hash".into(), current_hash.to_string());
                (StatusCode::CONFLICT, "edit_conflict")
            }
            Error::RateLimited { retry_after } => {
                if let Some(d) = retry_after {
                    context.insert(
                        "retry_after_secs".into(),
                        d.as_secs().to_string(),
                    );
                }
                (StatusCode::TOO_MANY_REQUESTS, "rate_limited")
            }
            Error::Stream { .. } => {
                (StatusCode::INTERNAL_SERVER_ERROR, "stream")
            }
            Error::StreamHttpFailure { .. } => {
                (StatusCode::INTERNAL_SERVER_ERROR, "stream_http_failure")
            }
            Error::StreamProtocolError { .. } => {
                (StatusCode::INTERNAL_SERVER_ERROR, "stream_protocol_error")
            }
            Error::StreamAborted { .. } => {
                (StatusCode::INTERNAL_SERVER_ERROR, "stream_aborted")
            }
            Error::StreamInternal { .. } => {
                (StatusCode::INTERNAL_SERVER_ERROR, "stream_internal")
            }
            Error::Timeout { .. } => (StatusCode::REQUEST_TIMEOUT, "timeout"),
            Error::RetryExhausted { attempts, .. } => {
                context.insert("attempts".into(), attempts.to_string());
                (StatusCode::SERVICE_UNAVAILABLE, "retry_exhausted")
            }
            Error::ModelNotFound { .. } => {
                (StatusCode::NOT_FOUND, "model_not_found")
            }
            Error::ModelUnavailable { .. } => {
                (StatusCode::SERVICE_UNAVAILABLE, "model_unavailable")
            }
            Error::Config { .. } => {
                (StatusCode::INTERNAL_SERVER_ERROR, "config")
            }
            Error::Router { .. } => {
                (StatusCode::INTERNAL_SERVER_ERROR, "router")
            }
            Error::Context { .. } => {
                (StatusCode::INTERNAL_SERVER_ERROR, "context")
            }
            Error::Telemetry { .. } => {
                (StatusCode::INTERNAL_SERVER_ERROR, "telemetry")
            }
            Error::Multiagent { .. } => {
                (StatusCode::INTERNAL_SERVER_ERROR, "multiagent")
            }
            Error::Evaluation { .. } => {
                (StatusCode::INTERNAL_SERVER_ERROR, "evaluation")
            }
            Error::ContextOverflow {
                limit_tokens,
                actual_tokens,
            } => {
                context.insert("limit_tokens".into(), limit_tokens.to_string());
                context
                    .insert("actual_tokens".into(), actual_tokens.to_string());
                (StatusCode::PAYLOAD_TOO_LARGE, "context_overflow")
            }
            Error::DoomLoop {
                tool_name,
                iterations,
            } => {
                context.insert("tool_name".into(), tool_name.clone());
                context.insert("iterations".into(), iterations.to_string());
                (StatusCode::CONFLICT, "doom_loop")
            }
            Error::PromptInjection {
                input_source,
                pattern,
            } => {
                context.insert("input_source".into(), input_source.clone());
                context.insert("pattern".into(), pattern.clone());
                (StatusCode::UNPROCESSABLE_ENTITY, "prompt_injection")
            }
            Error::Io { inner } => {
                context.insert("io_kind".into(), format!("{:?}", inner.kind()));
                (StatusCode::INTERNAL_SERVER_ERROR, "io")
            }
            _ => (StatusCode::INTERNAL_SERVER_ERROR, "internal_server_error"),
        };
        Self {
            status_code,
            source: anyhow::Error::new(err),
            code,
            message,
            context,
        }
    }
}

impl From<std::io::Error> for AppError {
    /// Foreign `io::Error` — classifies by the OS error kind so
    /// callers keep their transport-specific semantics.
    fn from(err: std::io::Error) -> Self {
        let status = match err.kind() {
            std::io::ErrorKind::NotFound => StatusCode::NOT_FOUND,
            std::io::ErrorKind::PermissionDenied => StatusCode::FORBIDDEN,
            std::io::ErrorKind::TimedOut => StatusCode::REQUEST_TIMEOUT,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        let msg = err.to_string();
        Self::new(status, err)
            .with_code("io_error")
            .with_message(format!("I/O error: {msg}"))
    }
}

impl From<serde_json::Error> for AppError {
    fn from(err: serde_json::Error) -> Self {
        let msg = err.to_string();
        Self::new(StatusCode::BAD_REQUEST, err)
            .with_code("parse")
            .with_message(format!("JSON error: {msg}"))
    }
}

impl From<serde_yaml::Error> for AppError {
    fn from(err: serde_yaml::Error) -> Self {
        let msg = err.to_string();
        Self::new(StatusCode::BAD_REQUEST, err)
            .with_code("parse")
            .with_message(format!("YAML error: {msg}"))
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        // Operator-facing log line: the full detail (status, wire
        // code, message, source chain, KV context). The wire body
        // below carries only code + message.
        tracing::error!(
            status = self.status_code.as_u16(),
            code = self.code,
            message = %self.message,
            context = ?self.context,
            source = %self.source,
            "request failed"
        );

        let body = Json(json!({
            "code": self.code,
            "message": self.message,
        }));
        let mut response = (self.status_code, body).into_response();

        // `RateLimited` carries a `retry_after` payload — surface
        // it through the standard header so clients can back off
        // without parsing the body. Walk the source chain for
        // the original `Error::RateLimited`.
        let retry_after = self.source.chain().find_map(|src| {
            match src.downcast_ref::<Error>() {
                Some(Error::RateLimited { retry_after, .. }) => *retry_after,
                _ => None,
            }
        });
        if let Some(retry_after) = retry_after {
            let seconds = retry_after.as_secs().max(1);
            if let Ok(value) = HeaderValue::from_str(&seconds.to_string()) {
                response
                    .headers_mut()
                    .insert(HeaderName::from_static("retry-after"), value);
            }
        }

        response
    }
}

// -----------------------------------------------------------------------------
// Extractor wrappers
//
// axum's `Json` / `Query` / `Path` extractors fail with their own
// `*Rejection` types, whose default `IntoResponse` is a plain-text
// body. The front-end `ApiClient.toError` parser only understands
// the flat `{"code","message"}` envelope, so we wrap each
// extractor and map every rejection through `AppError`.
//
// Handler usage keeps the same shape as before:
//
//   async fn create(State(state): State<Arc<AppState>>,
//                   AppJson(req): AppJson<CreateSkillRequest>)
//       -> Result<Json<SkillInfo>, AppError>
// -----------------------------------------------------------------------------

use axum::extract::{
    FromRequest,
    FromRequestParts,
    Path as AxumPath,
    Query as AxumQuery,
};

/// [`axum::Json`] wrapper that rejects through [`AppError`] so the
/// unified envelope covers malformed / missing request bodies.
pub struct AppJson<T>(pub T);

impl<T, S> FromRequest<S> for AppJson<T>
where
    T: serde::de::DeserializeOwned + validator::Validate,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request(
        req: axum::http::Request<axum::body::Body>,
        state: &S,
    ) -> Result<Self, Self::Rejection> {
        let Json(value) = Json::<T>::from_request(req, state)
            .await
            .map_err(AppError::from)?;
        value.validate()?;
        Ok(Self(value))
    }
}

/// [`axum::extract::Query`] wrapper that rejects through
/// [`AppError`] so the unified envelope covers malformed query
/// strings.
pub struct AppQuery<T>(pub T);

impl<T, S> FromRequestParts<S> for AppQuery<T>
where
    T: serde::de::DeserializeOwned + validator::Validate,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        state: &S,
    ) -> Result<Self, Self::Rejection> {
        let AxumQuery(value) = AxumQuery::<T>::from_request_parts(parts, state)
            .await
            .map_err(AppError::from)?;
        value.validate()?;
        Ok(Self(value))
    }
}

/// [`axum::extract::Path`] wrapper that rejects through
/// [`AppError`] so the unified envelope covers malformed path
/// parameters.
pub struct AppPath<T>(pub T);

impl<T, S> FromRequestParts<S> for AppPath<T>
where
    T: serde::de::DeserializeOwned + Send,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        state: &S,
    ) -> Result<Self, Self::Rejection> {
        let AxumPath(value) = AxumPath::<T>::from_request_parts(parts, state)
            .await
            .map_err(AppError::from)?;
        Ok(Self(value))
    }
}

impl From<axum::extract::rejection::JsonRejection> for AppError {
    fn from(err: axum::extract::rejection::JsonRejection) -> Self {
        let status = err.status();
        let message = err.body_text();
        Self::new(status, err)
            .with_code("invalid_json")
            .with_message(message)
    }
}

impl From<axum::extract::rejection::QueryRejection> for AppError {
    fn from(err: axum::extract::rejection::QueryRejection) -> Self {
        let status = err.status();
        let message = err.body_text();
        Self::new(status, err)
            .with_code("invalid_query")
            .with_message(message)
    }
}

impl From<axum::extract::rejection::PathRejection> for AppError {
    fn from(err: axum::extract::rejection::PathRejection) -> Self {
        let status = err.status();
        let message = err.body_text();
        Self::new(status, err)
            .with_code("invalid_path")
            .with_message(message)
    }
}

/// Schema-level validation failure from the `validator` crate
/// (the `#[validate(...)]` attributes on request DTOs). The
/// extractor wrappers (`AppJson` / `AppQuery` / `AppPath`) call
/// `Validate::validate` after deserialization, so this maps the
/// failure onto the unified envelope: 422 + `validation` code +
/// a flattened `field: message` summary.
impl From<validator::ValidationErrors> for AppError {
    fn from(err: validator::ValidationErrors) -> Self {
        let message = err
            .field_errors()
            .iter()
            .map(|(field, errors)| {
                let reasons: Vec<String> = errors
                    .iter()
                    .map(|e| {
                        e.message
                            .clone()
                            .unwrap_or_else(|| e.code.to_string().into())
                            .to_string()
                    })
                    .collect();
                format!("{field}: {}", reasons.join(", "))
            })
            .collect::<Vec<_>>()
            .join("; ");
        Error::validation(message).into()
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use axum::body::to_bytes;

    use super::*;

    async fn body_json(response: Response) -> serde_json::Value {
        let bytes = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body is collectable");
        serde_json::from_slice(&bytes).expect("body is valid JSON")
    }

    #[test]
    fn from_core_error_populates_status_code_message() {
        let err =
            AppError::from(Error::validation("limit must be greater than 0"));
        assert_eq!(err.status_code, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(err.code, "validation");
        assert_eq!(
            err.message,
            "validation error: limit must be greater than 0"
        );
        assert!(err.context.is_empty());
    }
    #[test]
    fn context_populated_for_variant_payloads() {
        // The `From<Error> for AppError` arm body folds
        // variant-specific payload fields into the
        // operator-facing `context` map. Confirm the structured
        // fields for the most-instrumented variants below.
        let app = AppError::from(Error::not_found("session-123"));
        assert_eq!(
            app.context.get("item").map(String::as_str),
            Some("session-123")
        );

        let app =
            AppError::from(Error::rate_limited(Some(Duration::from_secs(42))));
        assert_eq!(
            app.context.get("retry_after_secs").map(String::as_str),
            Some("42")
        );

        let path = std::path::PathBuf::from("/etc/config.toml");
        let app =
            AppError::from(Error::edit_conflict(path.clone(), 0xDEAD, 0xCAFE));
        assert_eq!(
            app.context.get("path").map(String::as_str),
            Some("/etc/config.toml")
        );
        assert_eq!(
            app.context.get("original_hash").map(String::as_str),
            Some("57005")
        );
        assert_eq!(
            app.context.get("current_hash").map(String::as_str),
            Some("51966")
        );

        let app = AppError::from(Error::context_overflow(100, 200));
        assert_eq!(
            app.context.get("limit_tokens").map(String::as_str),
            Some("100")
        );
        assert_eq!(
            app.context.get("actual_tokens").map(String::as_str),
            Some("200")
        );

        let app = AppError::from(Error::doom_loop("bash", 5));
        assert_eq!(
            app.context.get("tool_name").map(String::as_str),
            Some("bash")
        );
        assert_eq!(
            app.context.get("iterations").map(String::as_str),
            Some("5")
        );
    }

    #[test]
    fn builder_methods_are_chainable() {
        let err = AppError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            anyhow::anyhow!("base"),
        )
        .with_code("internal_server_error")
        .with_message("human text")
        .with_context("request_id", "abc-123")
        .with_context("subject", "user:42");
        assert_eq!(err.status_code, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(err.code, "internal_server_error");
        assert_eq!(err.message, "human text");
        assert_eq!(
            err.context.get("request_id").map(String::as_str),
            Some("abc-123")
        );
        assert_eq!(
            err.context.get("subject").map(String::as_str),
            Some("user:42")
        );
    }

    #[test]
    fn merge_context_overwrites() {
        let mut base = HashMap::new();
        base.insert("k".to_string(), "first".to_string());
        let extra = HashMap::from([
            ("k".to_string(), "second".to_string()),
            ("new".to_string(), "v".to_string()),
        ]);
        let err = AppError::new(StatusCode::BAD_REQUEST, anyhow::anyhow!("x"))
            .merge_context(base)
            .merge_context(extra);
        assert_eq!(err.context.get("k").map(String::as_str), Some("second"));
        assert_eq!(err.context.get("new").map(String::as_str), Some("v"));
    }

    #[tokio::test]
    async fn envelope_carries_core_code_and_display_message() {
        let response =
            AppError::from(Error::not_found("session 'abc'")).into_response();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let body = body_json(response).await;
        assert_eq!(body["code"], "not_found");
        assert_eq!(body["message"], "not found: session 'abc'");
    }

    #[tokio::test]
    async fn internal_error_uses_core_wire_name() {
        let response = AppError::from(Error::internal("boom")).into_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let body = body_json(response).await;
        // The `Internal` variant's stable wire name is
        // "internal_server_error" — pinned by the per-variant
        // arm body in `From<Error> for AppError`.
        assert_eq!(body["code"], "internal_server_error");
        assert_eq!(body["message"], "internal error: boom");
    }

    #[tokio::test]
    async fn rate_limited_emits_retry_after_header() {
        let response =
            AppError::from(Error::rate_limited(Some(Duration::from_secs(42))))
                .into_response();
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        let header = response
            .headers()
            .get("retry-after")
            .expect("retry-after header should be present");
        assert_eq!(header, "42");
    }

    #[tokio::test]
    async fn rate_limited_without_retry_after_omits_header() {
        let response =
            AppError::from(Error::rate_limited(None)).into_response();
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        assert!(response.headers().get("retry-after").is_none());
    }

    #[test]
    fn source_downcasts_back_to_core_error() {
        // Round-tripping the synthia_core::Error through anyhow
        // must keep the original error reachable (the `source`
        // chain feeds `tracing::error!` and rate-limit detection).
        let inner = Error::timeout("upstream");
        let app = AppError::from(inner);
        assert!(
            app.source.downcast_ref::<synthia_core::Error>().is_some(),
            "downcast_ref must recover the original Error"
        );
    }

    #[test]
    fn validation_errors_map_to_422_envelope() {
        // Schema-level validation failure (the `#[validate(...)]`
        // attributes on request DTOs) must produce the unified
        // envelope: 422 + `validation` code + a flattened
        // `field: message` summary.
        use validator::Validate;

        #[derive(validator::Validate)]
        struct Sample {
            #[validate(length(min = 3, message = "too short"))]
            name: String,
        }

        let err = Sample { name: "ab".into() }.validate().unwrap_err();
        let app = AppError::from(err);
        assert_eq!(app.status_code, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(app.code, "validation");
        assert!(app.message.contains("name"));
        assert!(app.message.contains("too short"));
    }
}
