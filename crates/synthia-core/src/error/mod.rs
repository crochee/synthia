//! Cross-cutting error types for the `synthia-core` library.
//!
//! Defines the single [`error::Error`] enum used by every
//! workspace member that depends on `synthia-core`, plus the
//! streaming-specific [`stream_error_kind::StreamErrorKind`]
//! sub-categorizer and structured `context` / `location`
//! metadata per variant.
//!
//! HTTP / wire-layer classification (e.g. `ErrorCode`,
//! `UserError`, `IntoResponse`) lives in
//! `synthia_server::api::error` — this module is
//! transport-agnostic so non-HTTP binaries can reuse the
//! [`error::Error`] enum directly.
//!
//! # Module Layout
//!
//! - [`stream_error_kind`]: The [`stream_error_kind::StreamErrorKind`]
//!   enum — 4 categories used by `complete_with_stream`'s
//!   truncate / cancel / fallback flow.
//! - [`error`]: The [`error::Error`] enum itself, plus the
//!   [`error::Error::is_retryable`],
//!   [`error::Error::is_rate_limited`],
//!   [`error::Error::stream_error`], and
//!   [`error::Error::wire_message`] accessors and the three
//!   external `From` impls
//!   ([`error::From<reqwest::Error>`],
//!   [`error::From<serde_json::Error>`],
//!   [`error::From<serde_yaml::Error>`]).
//!
//! [`error::Error::wire_message`]: error::Error::wire_message

#[allow(clippy::module_inception)]
mod error;
mod stream_error_kind;

pub use error::Error;
pub use stream_error_kind::StreamErrorKind;
