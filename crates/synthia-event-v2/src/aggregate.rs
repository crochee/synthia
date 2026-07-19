//! `aggregate_events::<T>()` facade — typed replay over Event V2 snapshots (PR-1.5).
//!
//! Consumers fetch snapshots via
//! [`InMemoryEventBus::snapshot`](crate::sink::in_memory::InMemoryEventBus::snapshot)
//! (or, in change #2 once the durable sink lands, via `EventStore`),
//! pair them with a [`crate::commit_guard::CommitGuard`] admission
//! policy, then feed the surviving snapshots into
//! `aggregate_events::<T>()` to obtain typed projections.

use serde::de::DeserializeOwned;
use thiserror::Error;

use crate::{
    commit_guard::{CommitGuard, CommitGuardError},
    projector::{IdentityProjector, Projector},
    sink::in_memory::StoredEventSnapshot,
};

/// Errors that can surface from [`aggregate_events`].
#[derive(Debug, Error)]
pub enum AggregateError {
    /// A snapshot failed the [`CommitGuard`] admission policy.
    #[error("commit guard rejection at sequence {sequence}: {source}")]
    GuardRejection {
        /// Offending sequence number.
        sequence: u64,
        /// Underlying [`CommitGuardError`].
        #[source]
        source: CommitGuardError,
    },
    /// A snapshot's payload could not be deserialized into the requested
    /// type.
    #[error("payload deserialize failed at sequence {sequence}: {source}")]
    Deserialize {
        /// Offending sequence number.
        sequence: u64,
        /// Underlying [`serde_json::Error`].
        #[source]
        source: serde_json::Error,
    },
}

/// Typed replay facade.
///
/// Each input snapshot is admitted through `guard`, then fed to
/// `projector` (callers may swap in custom projectors), then the
/// JSON payload is deserialized into `T`. Failed deserializations
/// surface as `Err(AggregateError::Deserialize { .. })` so the
/// caller can decide between skip-and-continue vs. fail-fast.
pub fn aggregate_events<T>(
    snapshots: &[StoredEventSnapshot],
    guard: &CommitGuard,
    projector: &dyn Projector,
) -> Result<Vec<T>, AggregateError>
where
    T: DeserializeOwned,
{
    let mut out = Vec::with_capacity(snapshots.len());
    for snap in snapshots {
        if let Err(e) = guard.validate(snap) {
            // Per spec "Scenario: commit guard rejection": log + skip
            // — do NOT call downstream projectors.
            tracing::trace!(
                target: "synthia::event_v2",
                sequence = snap.sequence,
                source = ?snap.source,
                error = %e,
                "aggregate_events: commit guard rejected snapshot",
            );
            // Continue with the next snapshot — single failure does
            // not abort the whole replay.
            continue;
        }
        if let Err(e) = projector.project(snap) {
            tracing::trace!(
                target: "synthia::event_v2",
                sequence = snap.sequence,
                error = %e,
                "aggregate_events: projector rejected snapshot",
            );
            continue;
        }
        let value: T =
            serde_json::from_value(snap.payload.clone()).map_err(|e| {
                AggregateError::Deserialize {
                    sequence: snap.sequence,
                    source: e,
                }
            })?;
        out.push(value);
    }
    Ok(out)
}

/// Convenience wrapper that uses a permissive guard + identity
/// projector. Useful in tests + examples.
pub fn aggregate_events_default<T>(
    snapshots: &[StoredEventSnapshot],
) -> Result<Vec<T>, AggregateError>
where
    T: DeserializeOwned,
{
    aggregate_events::<T>(
        snapshots,
        &CommitGuard::permissive(),
        &IdentityProjector::new(),
    )
}

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};
    use serde_json::json;

    use super::*;
    use crate::{
        commit_guard::Rule,
        event::{EventSource, PrefixHash},
    };

    fn snap_with(
        source: EventSource,
        sequence: u64,
        payload: serde_json::Value,
    ) -> StoredEventSnapshot {
        StoredEventSnapshot {
            source,
            sequence,
            prefix_hash: PrefixHash::default(),
            created_at_ms: 0,
            payload,
        }
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct TickEvent {
        n: u64,
    }

    #[test]
    fn aggregate_events_default_typed() {
        let snaps = vec![
            snap_with(EventSource::System, 1, json!({ "n": 1 })),
            snap_with(EventSource::System, 2, json!({ "n": 2 })),
        ];
        let out = aggregate_events_default::<TickEvent>(&snaps)
            .expect("typed replay");
        assert_eq!(out.len(), 2);
        assert_eq!(out[0], TickEvent { n: 1 });
        assert_eq!(out[1], TickEvent { n: 2 });
    }

    #[test]
    fn aggregate_events_commit_guard_skips_rejected() {
        let guard = CommitGuard::with_rules(vec![Rule::DisallowSource(
            EventSource::Tool,
        )]);
        let snaps = vec![
            snap_with(EventSource::System, 1, json!({ "n": 1 })),
            snap_with(EventSource::Tool, 2, json!({ "n": 2 })), // skipped
            snap_with(EventSource::System, 3, json!({ "n": 3 })),
        ];
        let out = aggregate_events::<TickEvent>(
            &snaps,
            &guard,
            &IdentityProjector::new(),
        )
        .expect("typed replay");
        assert_eq!(out.len(), 2, "Tool-source event must be skipped by guard");
        assert_eq!(guard.rejected_count(), 1);
    }

    #[test]
    fn aggregate_events_deserialize_failure_returns_err() {
        let snaps = vec![
            snap_with(EventSource::System, 1, json!({ "n": 1 })),
            snap_with(EventSource::System, 2, json!({ "missing_field": true })),
        ];
        let err = aggregate_events_default::<TickEvent>(&snaps)
            .expect_err("missing required field must surface");
        match err {
            AggregateError::Deserialize { sequence, .. } => {
                assert_eq!(sequence, 2);
            }
            other @ AggregateError::GuardRejection { .. } => {
                panic!("expected Deserialize, got {other:?}")
            }
        }
    }
}
