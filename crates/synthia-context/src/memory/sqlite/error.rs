//! Errors the `SQLite`-backed tiers surface.
//!
//! See the module docs on [`super`] for why the underlying
//! `rusqlite::Error` is reduced to its message.
/// Errors the SQLite-backed tiers surface.
///
/// See the module docs for why the underlying `rusqlite::Error` is
/// reduced to its message.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SqliteMemoryError {
    /// The database file could not be opened.
    #[error("failed to open SQLite database at {path}: {reason}")]
    Open {
        /// Path the open was attempted on.
        path: String,
        /// `SQLite`'s own message.
        reason: String,
    },
    /// A schema statement failed.
    #[error("SQLite schema migration failed: {reason}")]
    Migrate {
        /// `SQLite`'s own message.
        reason: String,
    },
    /// A statement outside the migration failed.
    #[error("SQLite {context} failed: {reason}")]
    Query {
        /// What was being attempted (`"recall"`, `"store"`, …).
        context: &'static str,
        /// `SQLite`'s own message.
        reason: String,
    },
    /// A stored row could not be turned back into a typed value.
    #[error("stored row could not be decoded: {reason}")]
    Decode {
        /// What did not parse.
        reason: String,
    },
    /// A write was attempted on a read-only database.
    #[error("SQLite memory is read-only")]
    ReadOnly,
    /// A read-only open found no schema to read.
    #[error("SQLite database at {path} has no long_term_memory schema")]
    MissingSchema {
        /// Path the read-only open was attempted on.
        path: String,
    },
}

/// Build a query error for `context`.
pub(super) fn query_err(
    context: &'static str,
    error: rusqlite::Error,
) -> SqliteMemoryError {
    SqliteMemoryError::Query {
        context,
        reason: error.to_string(),
    }
}
