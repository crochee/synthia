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

#[cfg(test)]
mod tests {
    use super::*;

    // -- serde 4-way mapping -----------------------------------------

    /// `StreamErrorKind` MUST serialize
    /// each variant in snake_case form
    /// (the wire format contract that
    /// downstream consumers parse).
    #[test]
    fn serializes_each_variant_as_snake_case() {
        assert_eq!(
            serde_json::to_string(&StreamErrorKind::HttpFailure).unwrap(),
            "\"http_failure\""
        );
        assert_eq!(
            serde_json::to_string(&StreamErrorKind::ProtocolError).unwrap(),
            "\"protocol_error\""
        );
        assert_eq!(
            serde_json::to_string(&StreamErrorKind::Aborted).unwrap(),
            "\"aborted\""
        );
        assert_eq!(
            serde_json::to_string(&StreamErrorKind::Internal).unwrap(),
            "\"internal\""
        );
    }

    /// `StreamErrorKind` MUST round-trip
    /// each variant through JSON without
    /// loss.
    #[test]
    fn round_trips_each_variant_through_json() {
        for kind in [
            StreamErrorKind::HttpFailure,
            StreamErrorKind::ProtocolError,
            StreamErrorKind::Aborted,
            StreamErrorKind::Internal,
        ] {
            let json = serde_json::to_string(&kind).unwrap();
            let parsed: StreamErrorKind =
                serde_json::from_str(&json).expect("round-trip parse");
            assert_eq!(parsed, kind);
        }
    }

    /// `StreamErrorKind` MUST reject an
    /// unknown variant (defensive: an
    /// upstream provider adding a new
    /// category must not silently
    /// round-trip into our existing
    /// variant set).
    #[test]
    fn rejects_unknown_variant_string() {
        let result: Result<StreamErrorKind, _> =
            serde_json::from_str("\"nonexistent_kind\"");
        assert!(result.is_err());
    }

    // -- Display 4-way mapping ---------------------------------------

    /// `Display` MUST emit the same
    /// snake_case string as the
    /// serialized wire form for each
    /// variant (consumers can rely on
    /// `format!("{e}")` for log lines).
    #[test]
    fn display_matches_serde_for_each_variant() {
        assert_eq!(format!("{}", StreamErrorKind::HttpFailure), "http_failure");
        assert_eq!(
            format!("{}", StreamErrorKind::ProtocolError),
            "protocol_error"
        );
        assert_eq!(format!("{}", StreamErrorKind::Aborted), "aborted");
        assert_eq!(format!("{}", StreamErrorKind::Internal), "internal");
    }

    // -- Trait surface -----------------------------------------------

    /// `StreamErrorKind` MUST implement
    /// `Copy` (it is a tiny 4-variant
    /// enum used in hot stream paths).
    #[test]
    fn copy_trait_does_not_move() {
        let k = StreamErrorKind::Aborted;
        let _copy = k; // would move if not Copy
        let _still_valid = k; // ok if Copy
    }

    /// `StreamErrorKind` MUST implement
    /// `Eq` (used in error equivalence
    /// checks).
    #[test]
    fn eq_trait_compares_variants() {
        assert_eq!(StreamErrorKind::Aborted, StreamErrorKind::Aborted);
        assert_ne!(StreamErrorKind::Aborted, StreamErrorKind::Internal);
    }

    /// All four variants MUST be
    /// distinct (sanity check that no
    /// two variants accidentally alias).
    #[test]
    fn all_four_variants_are_distinct() {
        let all = [
            StreamErrorKind::HttpFailure,
            StreamErrorKind::ProtocolError,
            StreamErrorKind::Aborted,
            StreamErrorKind::Internal,
        ];
        for i in 0..all.len() {
            for j in 0..all.len() {
                if i != j {
                    assert_ne!(all[i], all[j], "variants {i} and {j} alias");
                }
            }
        }
    }
}
