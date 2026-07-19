//! `CommitGuard` — admission policy for projector reads (PR-1.5).
//!
//! A `CommitGuard` is consulted *before* a [`Projector`](crate::projector::Projector)
//! runs. Returning `Err` from `validate` causes the event to be skipped
//! from the projection surface AND increments a metrics counter
//! (`event_v2_commit_guard_rejected_total`) called for in
//! `specs/event-v2-system/spec.md` ("Scenario: commit guard
//! rejection").
//!
//! The default `CommitGuard::permissive()` admits every event.
//! Consumers compose rules via [`Rule`].

use std::sync::atomic::{AtomicU64, Ordering};

use thiserror::Error;

use crate::sink::in_memory::StoredEventSnapshot;

/// Rejection error returned by [`CommitGuard::validate`].
#[derive(Debug, Error, PartialEq, Eq)]
pub enum CommitGuardError {
    /// Event's source is not in the allowed list.
    #[error("commit guard rejected source {0:?}")]
    DisallowedSource(crate::event::EventSource),
    /// Event's sequence number is below the configured floor.
    #[error("commit guard rejected sequence {sequence} (floor {floor})")]
    SequenceBelowFloor {
        /// Offending sequence number.
        sequence: u64,
        /// Required minimum sequence number for this source.
        floor: u64,
    },
    /// Event's prefix hash is not in the allow list.
    #[error("commit guard rejected non-allowlisted prefix hash")]
    PrefixHashNotAllowed,
}

/// One rule in a `CommitGuard`'s policy chain.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Rule {
    /// Disallow events from the given source.
    DisallowSource(crate::event::EventSource),
    /// Require `sequence >= floor` for the given source.
    MinSequence {
        /// Source the floor applies to.
        source: crate::event::EventSource,
        /// Required minimum.
        floor: u64,
    },
    /// Require the prefix hash to match the allowlisted hex digest.
    AllowPrefixHashHex(String),
}

/// `CommitGuard` is a small rule chain executed before each projector
/// invocation.
#[derive(Debug, Default)]
pub struct CommitGuard {
    /// Ordered rule chain.
    rules: Vec<Rule>,
    /// Running counter of rejected events.
    rejected: AtomicU64,
}

impl Clone for CommitGuard {
    fn clone(&self) -> Self {
        Self {
            rules: self.rules.clone(),
            rejected: AtomicU64::new(self.rejected.load(Ordering::SeqCst)),
        }
    }
}

impl CommitGuard {
    /// Construct a guard with no rules (every event admitted).
    #[must_use]
    pub fn permissive() -> Self {
        Self::default()
    }

    /// Construct a guard from a `Vec<Rule>`.
    #[must_use]
    pub fn with_rules(rules: Vec<Rule>) -> Self {
        Self {
            rules,
            rejected: AtomicU64::new(0),
        }
    }

    /// Push a rule onto the chain. Returns `self` for fluent builders.
    #[must_use]
    pub fn push(mut self, rule: Rule) -> Self {
        self.rules.push(rule);
        self
    }

    /// Number of rules in the chain.
    #[must_use]
    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }

    /// Validate `snapshot` against the rule chain. On the first failing
    /// rule, increments the rejection counter and returns the
    /// corresponding error.
    pub fn validate(
        &self,
        snapshot: &StoredEventSnapshot,
    ) -> Result<(), CommitGuardError> {
        for rule in &self.rules {
            match rule {
                Rule::DisallowSource(src) => {
                    if snapshot.source == *src {
                        self.incr_rejected();
                        return Err(CommitGuardError::DisallowedSource(*src));
                    }
                }
                Rule::MinSequence { source, floor } => {
                    if snapshot.source == *source && snapshot.sequence < *floor
                    {
                        self.incr_rejected();
                        return Err(CommitGuardError::SequenceBelowFloor {
                            sequence: snapshot.sequence,
                            floor: *floor,
                        });
                    }
                }
                Rule::AllowPrefixHashHex(hex) => {
                    if snapshot.prefix_hash.hex() != *hex {
                        self.incr_rejected();
                        return Err(CommitGuardError::PrefixHashNotAllowed);
                    }
                }
            }
        }
        Ok(())
    }

    /// Total rejected events since construction.
    #[must_use]
    pub fn rejected_count(&self) -> u64 {
        self.rejected.load(Ordering::SeqCst)
    }

    fn incr_rejected(&self) {
        self.rejected.fetch_add(1, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::event::{EventSource, PrefixHash};

    fn snap(source: EventSource, sequence: u64) -> StoredEventSnapshot {
        StoredEventSnapshot {
            source,
            sequence,
            prefix_hash: PrefixHash::default(),
            created_at_ms: 0,
            payload: json!({}),
        }
    }

    #[test]
    fn permissive_admits_everything() {
        let g = CommitGuard::permissive();
        assert_eq!(g.rule_count(), 0);
        assert!(g.validate(&snap(EventSource::Agent, 1)).is_ok());
        assert_eq!(g.rejected_count(), 0);
    }

    #[test]
    fn disallow_source_rejects_matching_event() {
        let g = CommitGuard::with_rules(vec![Rule::DisallowSource(
            EventSource::Tool,
        )]);
        assert!(g.validate(&snap(EventSource::Tool, 1)).is_err());
        assert!(g.validate(&snap(EventSource::Agent, 1)).is_ok());
        assert_eq!(g.rejected_count(), 1);
    }

    #[test]
    fn min_sequence_floor_per_source() {
        let g = CommitGuard::with_rules(vec![Rule::MinSequence {
            source: EventSource::Agent,
            floor: 10,
        }]);
        assert!(g.validate(&snap(EventSource::Agent, 9)).is_err());
        assert!(g.validate(&snap(EventSource::Agent, 10)).is_ok());
        assert!(g.validate(&snap(EventSource::Agent, 11)).is_ok());
        // Tool source has no floor — admitted unconditionally.
        assert!(g.validate(&snap(EventSource::Tool, 1)).is_ok());
    }

    #[test]
    fn rule_count_reflects_chain() {
        let g = CommitGuard::permissive()
            .push(Rule::DisallowSource(EventSource::User))
            .push(Rule::MinSequence {
                source: EventSource::System,
                floor: 0,
            });
        assert_eq!(g.rule_count(), 2);
    }
}
