//! The [`StreamErrorKind`] enum — 4 categories used by
//! `complete_with_stream`'s truncate / cancel / fallback flow.

use std::fmt;

use serde::{Deserialize, Serialize};

/// Streaming-specific error categories used by `complete_with_stream`
/// and downstream consumers (truncate fallback, retry policy,
/// metrics tagging).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StreamErrorKind {
    /// Upstream HTTP transport failure (connection, TLS, status code).
    HttpFailure,
    /// Malformed SSE / chunked / framing issue from the provider.
    ProtocolError,
    /// Stream aborted via cancellation token or context deadline.
    Aborted,
    /// Provider internal or unexpected error not classifiable above.
    Internal,
}

impl fmt::Display for StreamErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            StreamErrorKind::HttpFailure => "http_failure",
            StreamErrorKind::ProtocolError => "protocol_error",
            StreamErrorKind::Aborted => "aborted",
            StreamErrorKind::Internal => "internal",
        };
        f.write_str(s)
    }
}
