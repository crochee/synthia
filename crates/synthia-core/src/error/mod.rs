//! Cross-cutting error types for the entire Synthia workspace.
//!
//! Defines a single [`error::Error`] enum (used by every crate),
//! its stable wire-level [`error_code::ErrorCode`] classifier, a
//! streaming-specific [`stream_error_kind::StreamErrorKind`]
//! sub-categorizer, a structured [`user_error::UserError`] for
//! safe surfacing to API consumers, and a generic
//! [`api_response::ApiResponse<T>`] envelope for JSON-RPC /
//! HTTP responses.
//!
//! # Module Layout
//!
//! - [`error_code`]: The [`error_code::ErrorCode`] enum — 33
//!   stable snake_case codes that never change (so they can be
//!   used as API wire-level discriminators).
//! - [`stream_error_kind`]: The [`stream_error_kind::StreamErrorKind`]
//!   enum — 4 categories used by `complete_with_stream`'s
//!   truncate / cancel / fallback flow.
//! - [`error`]: The [`error::Error`] enum itself, plus the
//!   [`error::Error::is_retryable`], [`error::Error::is_rate_limited`],
//!   [`error::Error::stream_error`], [`error::Error::code`] accessors
//!   and the three external `From` impls
//!   ([`error::From<reqwest::Error>`],
//!   [`error::From<serde_json::Error>`],
//!   [`error::From<serde_yaml::Error>`]).
//! - [`user_error`]: The [`user_error::UserError`] struct — a
//!   user-facing (safe-to-display) wrapper carrying an
//!   [`error_code::ErrorCode`], message, and optional structured
//!   details. Implements `Display`, `std::error::Error`, and four
//!   `From` impls.
//! - [`api_response`]: The [`api_response::ApiResponse<T>`] enum —
//!   a JSON-RPC-style envelope with `Ok { data }` and
//!   `Err { error: UserError }` variants. The `tag = "status"`
//!   serde attribute means the wire format is
//!   `{ "status": "ok", "data": T }` or
//!   `{ "status": "err", "error": UserError }`.
//! - [`tests`]: All 17 unit tests covering serialization round-trips,
//!   `Display` formatting, code mapping, retryability, and the
//!   structured stream-error API.

mod api_response;
#[allow(clippy::module_inception)]
mod error;
mod error_code;
mod stream_error_kind;
mod user_error;

#[cfg(test)]
mod tests;

pub use api_response::ApiResponse;
pub use error::Error;
pub use error_code::ErrorCode;
pub use stream_error_kind::StreamErrorKind;
pub use user_error::UserError;
