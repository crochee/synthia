//! Error type for the search engine.
//!
//! Every fallible API in `synthia-search` returns
//! [`SearchError`]. Variants are deliberately narrow:
//!
//! - [`SearchError::NotBuilt`] — defensive fall-through for any
//!   unreachable lock-failure path (parking_lot does not poison,
//!   so this should be unreachable in practice).
//! - [`SearchError::DimMismatch`] — the embedder's `dim` did not
//!   match the engine's expected dimension.
//! - [`SearchError::NotFound`] — a `remove(id)` / `get(id)` call
//!   for an id the engine has never seen.
//! - [`SearchError::Empty`] — `Registry::search` was called with
//!   no engines registered.
//! - [`SearchError::Persist`] — a `save` / `load` filesystem or
//!   serialisation failure; the in-memory engine is never modified
//!   by a failed persistence call.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum SearchError {
    #[error("engine not built")]
    NotBuilt,

    #[error("embedding dim mismatch: expected {expected}, got {got}")]
    DimMismatch { expected: usize, got: usize },

    #[error("not found: {0}")]
    NotFound(String),

    /// Reserved for future use: `Registry::search` currently
    /// returns an empty `Vec` when no engines are registered
    /// rather than an error, per spec §7.
    #[error("registry is empty")]
    Empty,

    #[error("persistence error: {0}")]
    Persist(String),
}

pub type Result<T> = std::result::Result<T, SearchError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dim_mismatch_displays_dimensions() {
        let e = SearchError::DimMismatch {
            expected: 64,
            got: 128,
        };
        assert_eq!(
            e.to_string(),
            "embedding dim mismatch: expected 64, got 128"
        );
    }

    #[test]
    fn not_found_displays_id() {
        let e = SearchError::NotFound("skill.pdf_extract".into());
        assert_eq!(e.to_string(), "not found: skill.pdf_extract");
    }

    #[test]
    fn empty_displays_message() {
        assert_eq!(SearchError::Empty.to_string(), "registry is empty");
    }

    #[test]
    fn result_alias_default_is_search_error() {
        fn check(_: Result<()>) {}
        check(Ok(()));
    }
}
