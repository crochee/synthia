//! [`SessionEndReason`] — why a session ended.
//!
//! Derives `Clone + Debug + Serialize + Deserialize + PartialEq`.

use serde::{Deserialize, Serialize};

/// Reason why a session ended.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum SessionEndReason {
    /// The agent finished all iterations with a text-only response.
    Completed,
    /// The session was cancelled (e.g. via `Ctrl+C` or
    /// `SessionOp::Cancel`).
    Cancelled,
    /// An unrecoverable error surfaced from the provider.
    Error(String),
    /// The agent hit `MAX_ITERATIONS` (25) without converging.
    MaxIterations,
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- serde 4-way mapping (default = externally-tagged PascalCase) -

    /// `SessionEndReason` MUST serialize
    /// unit variants as PascalCase
    /// strings (`"Completed"`, etc.),
    /// and `Error` as
    /// `{"Error": "..."}` (default
    /// externally-tagged serde).
    #[test]
    fn serializes_each_variant_as_pascal_case() {
        assert_eq!(
            serde_json::to_string(&SessionEndReason::Completed).unwrap(),
            "\"Completed\""
        );
        assert_eq!(
            serde_json::to_string(&SessionEndReason::Cancelled).unwrap(),
            "\"Cancelled\""
        );
        assert_eq!(
            serde_json::to_string(&SessionEndReason::MaxIterations).unwrap(),
            "\"MaxIterations\""
        );
        // Error variant uses the externally
        // tagged form with the inner
        // String as the value.
        assert_eq!(
            serde_json::to_string(&SessionEndReason::Error(
                "rate_limit_exceeded".to_string()
            ))
            .unwrap(),
            r#"{"Error":"rate_limit_exceeded"}"#
        );
    }

    /// `SessionEndReason` MUST round-trip
    /// each variant through JSON.
    #[test]
    fn round_trips_each_variant_through_json() {
        for reason in [
            SessionEndReason::Completed,
            SessionEndReason::Cancelled,
            SessionEndReason::Error("provider_429".to_string()),
            SessionEndReason::MaxIterations,
        ] {
            let json = serde_json::to_string(&reason).unwrap();
            let parsed: SessionEndReason =
                serde_json::from_str(&json).expect("round-trip");
            assert_eq!(parsed, reason);
        }
    }

    /// `SessionEndReason` MUST reject
    /// unknown variant strings.
    #[test]
    fn rejects_unknown_variant_string() {
        let result: Result<SessionEndReason, _> =
            serde_json::from_str("\"NonexistentReason\"");
        assert!(result.is_err());
    }

    // -- Distinctness -----------------------------------------------

    /// All 4 `SessionEndReason` variants
    /// MUST be pairwise distinct.
    #[test]
    fn all_four_variants_are_pairwise_distinct() {
        assert_ne!(SessionEndReason::Completed, SessionEndReason::Cancelled);
        assert_ne!(
            SessionEndReason::Completed,
            SessionEndReason::MaxIterations
        );
        assert_ne!(
            SessionEndReason::Completed,
            SessionEndReason::Error("x".to_string())
        );
        assert_ne!(
            SessionEndReason::Cancelled,
            SessionEndReason::MaxIterations
        );
        assert_ne!(
            SessionEndReason::Cancelled,
            SessionEndReason::Error("x".to_string())
        );
        assert_ne!(
            SessionEndReason::MaxIterations,
            SessionEndReason::Error("x".to_string())
        );
    }

    /// `SessionEndReason::Error` MUST
    /// distinguish by inner payload.
    #[test]
    fn error_variant_distinguishes_by_inner_payload() {
        let a = SessionEndReason::Error("a".to_string());
        let b = SessionEndReason::Error("b".to_string());
        assert_ne!(a, b);
    }

    /// `SessionEndReason::Error` MUST
    /// accept the empty string.
    #[test]
    fn error_variant_accepts_empty_string() {
        let e = SessionEndReason::Error(String::new());
        let json = serde_json::to_string(&e).unwrap();
        let parsed: SessionEndReason =
            serde_json::from_str(&json).expect("round-trip");
        assert_eq!(parsed, e);
    }

    // -- Trait surface ----------------------------------------------

    /// `SessionEndReason` MUST implement
    /// Clone + Debug + PartialEq + Eq.
    #[test]
    fn supports_clone_debug_partial_eq_eq() {
        let r = SessionEndReason::Error("debug".to_string());
        let _copy = r.clone();
        let _ = format!("{:?}", r);
        assert_eq!(r, r);
    }
}
