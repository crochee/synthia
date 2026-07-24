//! Error types for `synthia-session`.
//!
//! Centralised so that [`crate::types`] and [`crate::store`] can refer to the
//! same `StoreError` / `HashChainError` enums without a circular module
//! dependency. The `HashChainError` variants in particular close the
//! cross-user session access path that the 2026-06-16 adversarial review
//! identified as a P0 vulnerability.

use thiserror::Error;

/// Errors produced by [`crate::store::Store`] and the [`crate::types`]
/// constructors. Kept additive — existing callers that surface
/// `anyhow::Error` continue to work because each variant implements
/// `std::error::Error` via `thiserror`.
#[derive(Error, Debug)]
pub enum StoreError {
    /// Caller invoked `Session::new_with_user` with an empty `user_id`.
    /// Multi-tenant mode requires a non-empty `user_id`; the empty
    /// string is reserved for the legacy single-tenant layout and is
    /// rejected explicitly to satisfy the project memory hard constraint
    /// "cache hash MUST include user_id namespace".
    #[error("Empty user_id rejected for session {session_id:?}")]
    EmptyUserId { session_id: String },

    /// `Store::load_metadata` (or the migration shim) found a metadata
    /// file that predates the `owner_user_id` field. The shim
    /// automatically upgrades it; this error is reserved for callers
    /// that explicitly disable migration.
    #[error(
        "Session {session_id:?} predates user_id namespace; migration disabled"
    )]
    MissingUserId { session_id: String },

    /// `Store::list_sessions_with_metadata` was called with a
    /// `caller_user_id` that does not match the `owner_user_id` of one
    /// or more visible session directories. The store refuses to return
    /// the metadata of sessions that do not belong to the caller.
    #[error(
        "CrossUserAccess: caller {caller} cannot view session owned by {owner}"
    )]
    CrossUserAccess { caller: String, owner: String },

    /// Underlying I/O failure with full context.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Serialization/deserialization failure with full context.
    #[error("Serialization error: {0}")]
    Serde(#[from] serde_json::Error),
}

/// `HashChainError` is a thin alias kept for naming consistency with
/// the spec (`spec.md`: "MUST return
/// `Err(HashChainError::CrossUserAccess)`"). The single source of truth
/// is [`StoreError`]; this alias is for code that already names the
/// error after the integrity check.
pub type HashChainError = StoreError;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_user_id_error_displays_session_id() {
        let e = StoreError::EmptyUserId {
            session_id: "abc".to_string(),
        };
        let msg = format!("{}", e);
        assert!(msg.contains("abc"));
    }

    #[test]
    fn test_cross_user_access_error_displays_both() {
        let e = StoreError::CrossUserAccess {
            caller: "alice".to_string(),
            owner: "bob".to_string(),
        };
        let msg = format!("{}", e);
        assert!(msg.contains("alice"));
        assert!(msg.contains("bob"));
    }
}
