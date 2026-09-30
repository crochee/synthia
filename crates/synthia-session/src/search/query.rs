//! The search seam: what a consumer asks for, what a backend
//! answers with, and the trait between them.

use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::FoldError;

/// What to look for.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SearchQuery {
    /// Free text. Terms are lowercased and matched on word boundaries
    /// inside the entry text; empty matches nothing.
    pub text: String,
    /// Maximum hits to return (per method). `None` = [`DEFAULT_LIMIT`].
    pub limit: Option<usize>,
    /// Restrict to one session. `None` searches every indexed session.
    pub session_id: Option<String>,
    /// Restrict to one user's sessions. `None` searches every user's —
    /// deliberate for a caller that owns the whole store, and never what
    /// a request handler wants: a store holds several tenants'
    /// transcripts, so an unscoped query answers one tenant with
    /// another's history.
    pub user_id: Option<String>,
}

/// Default result cap when [`SearchQuery::limit`] is `None`.
pub const DEFAULT_LIMIT: usize = 20;

impl SearchQuery {
    /// A query for `text` with the default limit.
    #[must_use]
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            limit: None,
            session_id: None,
            user_id: None,
        }
    }

    /// Cap the number of hits.
    #[must_use]
    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit.max(1));
        self
    }

    /// Restrict the search to one session.
    #[must_use]
    pub fn in_session(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }

    /// Restrict the search to one user's sessions.
    ///
    /// The store keeps every user's sessions side by side, and two users
    /// may pick the same session id, so a search that must answer one
    /// tenant has to say which one it is.
    #[must_use]
    pub fn in_user(mut self, user_id: impl Into<String>) -> Self {
        self.user_id = Some(user_id.into());
        self
    }

    pub(super) fn effective_limit(&self) -> usize {
        self.limit.unwrap_or(DEFAULT_LIMIT).max(1)
    }

    /// The query's terms, lowercased, duplicates dropped, in order.
    pub(super) fn terms(&self) -> Vec<String> {
        let mut terms: Vec<String> = Vec::new();
        for term in self.text.split_whitespace() {
            let term = term.to_lowercase();
            if !terms.contains(&term) {
                terms.push(term);
            }
        }
        terms
    }
}

/// One matching entry — a single conversation row.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EntryHit {
    pub session_id: String,
    /// The log row's 1-based sequence number, so a caller can jump to it
    /// (the same number [`fold_log_surface`](crate::fold_log_surface) reports in
    /// `surface_seqs`).
    pub seq: u64,
    /// ISO-8601 timestamp of the row when the log carries one.
    pub timestamp: Option<String>,
    /// The text around the first match, char-boundary safe.
    pub snippet: String,
    pub score: f32,
}

/// One matching session, with its best entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionHit {
    pub session_id: String,
    pub score: f32,
    /// How many of the session's entries matched.
    pub matched_entries: usize,
    /// The highest-scoring entry, for the summary line a UI shows.
    pub top: Option<EntryHit>,
}

/// Failure modes of a search backend.
#[derive(Debug, thiserror::Error)]
pub enum SearchError {
    /// The store could not be read.
    #[error("session search io: {0}")]
    Io(String),
    /// The stored log is not readable as a session log.
    #[error("session search log: {0}")]
    Log(String),
    /// The index could not be folded (a provenance violation the lenient
    /// fold does not skip).
    #[error("session search fold: {0}")]
    Fold(#[from] FoldError),
}

/// Search a store of sessions.
///
/// Runtime-neutral (`#[async_trait]`, no runtime type in any signature), so
/// the index can live in memory, in `SQLite`, or behind a service.
///
/// A query may carry a scope ([`SearchQuery::user_id`]); an implementation
/// MUST NOT answer a scoped query with a session outside it. `None` means
/// "the whole store", which is only right for a caller that owns all of
/// it.
#[async_trait]
pub trait SessionSearch: Send + Sync {
    /// Sessions matching the query, best first. `limit` hits at most.
    ///
    /// # Errors
    ///
    /// Backend-specific: unreadable store, unreadable log, index failure.
    async fn search_sessions(
        &self,
        query: &SearchQuery,
    ) -> Result<Vec<SessionHit>, SearchError>;

    /// Individual entries matching the query, best first.
    ///
    /// # Errors
    ///
    /// Same as [`SessionSearch::search_sessions`].
    async fn search_entries(
        &self,
        query: &SearchQuery,
    ) -> Result<Vec<EntryHit>, SearchError>;

    /// Refresh the index from the store and report how many sessions are
    /// indexed. Cheap when nothing changed (a log's size is the cheap
    /// change signal; a session log is append-only).
    ///
    /// # Errors
    ///
    /// Same as [`SessionSearch::search_sessions`].
    async fn sync(&self) -> Result<usize, SearchError>;

    /// Forget one user's session (a deletion, or a caller that no longer
    /// wants it searchable). Returns whether it was indexed.
    ///
    /// The user is part of the address, not a filter: forgetting
    /// `("alice", "main")` leaves `("bob", "main")` searchable.
    ///
    /// The forget is durable across [`SessionSearch::sync`]: a log that
    /// still exists on disk does not resurrect itself, so "deleted but not
    /// yet erased" stays unsearchable.
    ///
    /// # Errors
    ///
    /// Same as [`SessionSearch::search_sessions`].
    async fn remove(
        &self,
        user_id: &str,
        session_id: &str,
    ) -> Result<bool, SearchError>;
}

/// A shared handle, the shape a server stores on its state.
pub type SharedSessionSearch = Arc<dyn SessionSearch>;
