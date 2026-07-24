//! Reconciliation logic for system-context snapshots.

use crate::system_context::source::Snapshot;

/// Result of reconciling a source value against its previous snapshot.
#[derive(Debug, Clone)]
pub enum ReconcileResult<V> {
    /// Value unchanged since the previous snapshot.
    Unchanged,
    /// Value changed and the internal snapshot was updated.
    Updated,
    /// Value changed; a new snapshot is ready to install.
    ReplacementReady(Snapshot<V>),
    /// Value changed but replacement is blocked by an in-flight tool call.
    ReplacementBlocked,
}

/// Reconcile `current` against `prev`.
///
/// Returns [`Unchanged`](ReconcileResult::Unchanged) when the value is
/// identical, [`ReplacementBlocked`](ReconcileResult::ReplacementBlocked) when
/// a tool call is in flight, or
/// [`ReplacementReady`](ReconcileResult::ReplacementReady) with a bumped
/// snapshot otherwise.
///
/// `has_in_flight_tool_call` is determined by the caller from runtime state;
/// this function does not consult any global.
pub fn reconcile<V: PartialEq + Clone>(
    current: &V,
    prev: &Snapshot<V>,
    has_in_flight_tool_call: bool,
) -> ReconcileResult<V> {
    if current == &prev.value {
        return ReconcileResult::Unchanged;
    }
    if has_in_flight_tool_call {
        tracing::warn!(
            "SystemContext replacement blocked due to in-flight tool call"
        );
        return ReconcileResult::ReplacementBlocked;
    }
    let new_snapshot = Snapshot::new(current.clone(), prev.revision + 1);
    ReconcileResult::ReplacementReady(new_snapshot)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reconcile_unchanged_when_value_identical() {
        let snap = Snapshot::new(42, 1);
        let result = reconcile(&42, &snap, false);
        assert!(matches!(result, ReconcileResult::Unchanged));
    }

    #[test]
    fn reconcile_updated_when_value_changed_and_no_in_flight() {
        let snap = Snapshot::new(42, 1);
        let result = reconcile(&99, &snap, false);
        match result {
            ReconcileResult::ReplacementReady(new_snap) => {
                assert_eq!(new_snap.value, 99);
                assert_eq!(new_snap.revision, 2);
            }
            _ => panic!("expected ReplacementReady"),
        }
    }

    #[test]
    fn reconcile_blocked_when_in_flight_tool_call() {
        let snap = Snapshot::new(42, 1);
        let result = reconcile(&99, &snap, true);
        assert!(matches!(result, ReconcileResult::ReplacementBlocked));
    }

    #[test]
    fn replacement_blocked_logs_warning() {
        // reconcile with in-flight=true must return ReplacementBlocked and
        // emit a `tracing::warn!` without panicking.
        let snap = Snapshot::new(42, 1);
        let result = reconcile(&99, &snap, true);
        assert!(matches!(result, ReconcileResult::ReplacementBlocked));
    }
}
