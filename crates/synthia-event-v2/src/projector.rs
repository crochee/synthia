//! Projector trait + reference impls for the Event V2 bus (PR-1.5).
//!
//! A `Projector` consumes a [`crate::sink::in_memory::StoredEventSnapshot`]
//! and turns it into a typed projection. The default [`IdentityProjector`]
//! is a no-op (used as a placeholder while consumers build their own).
//!
//! Projectors are typically paired with a
//! [`crate::commit_guard::CommitGuard`] — the guard decides whether the
//! event should be admitted to the projection surface; the projector
//! executes the actual reduction.

use thiserror::Error;

use crate::sink::in_memory::StoredEventSnapshot;

/// Errors that can surface from a [`Projector`].
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ProjectorError {
    /// The projector is closed (e.g. its sink was dropped).
    #[error("projector closed")]
    Closed,
    /// The snapshot's payload type is incompatible with this projector.
    #[error("projector payload type mismatch: {0}")]
    TypeMismatch(String),
}

/// Consumer-side projection of an Event V2 snapshot.
///
/// Projectors are stateless with respect to the bus; any state they
/// accumulate lives in the implementing struct.
pub trait Projector: Send + Sync + 'static {
    /// Project the snapshot. Returning `Err` does NOT roll back the
    /// emitter — failures are reported via [`crate::commit_guard`]
    /// metrics + tracing spans and the projection surface skips the
    /// event.
    fn project(
        &self,
        snapshot: &StoredEventSnapshot,
    ) -> Result<(), ProjectorError>;
}

/// Reference projector: counts events and records the latest seen
/// sequence. Cheap enough to keep enabled in tests as a smoke probe.
#[derive(Debug, Default, Clone)]
pub struct IdentityProjector {
    /// Number of projections accepted.
    seen: std::sync::Arc<std::sync::atomic::AtomicU64>,
}

impl IdentityProjector {
    /// Construct a fresh identity projector.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of `project` calls that have returned `Ok(())`.
    #[must_use]
    pub fn seen(&self) -> u64 {
        self.seen.load(std::sync::atomic::Ordering::SeqCst)
    }
}

impl Projector for IdentityProjector {
    fn project(
        &self,
        snapshot: &StoredEventSnapshot,
    ) -> Result<(), ProjectorError> {
        let _ = snapshot; // identity makes no claim about the payload
        self.seen.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::event::{EventSource, PrefixHash};

    fn snapshot_with(
        sequence: u64,
        source: EventSource,
    ) -> StoredEventSnapshot {
        StoredEventSnapshot {
            source,
            sequence,
            prefix_hash: PrefixHash::default(),
            created_at_ms: 0,
            payload: json!({}),
        }
    }

    #[test]
    fn identity_projector_counts_accepts() {
        let p = IdentityProjector::new();
        assert_eq!(p.seen(), 0);
        p.project(&snapshot_with(1, EventSource::System)).unwrap();
        p.project(&snapshot_with(2, EventSource::Agent)).unwrap();
        assert_eq!(p.seen(), 2);
    }

    #[test]
    fn identity_projector_returns_ok_even_when_payload_is_unexpected() {
        // The identity projector makes no claims about payload shape;
        // mismatched payloads should not surface as `TypeMismatch`.
        let p = IdentityProjector::new();
        let r = p.project(&snapshot_with(1, EventSource::Tool));
        assert!(r.is_ok());
    }
}
