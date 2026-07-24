use std::sync::{Arc, atomic::Ordering};

use super::{agent_path::AgentPath, registry::AgentRegistry};

/// RAII guard for a spawn reservation.
/// Two-phase commit:
/// 1. Constructor reserves a slot (increments thread count)
/// 2. On drop without `confirm()`, the slot is released (decremented)
pub struct SpawnReservation {
    registry: Arc<AgentRegistry>,
    path: AgentPath,
    confirmed: bool,
}

impl SpawnReservation {
    /// Reserve a spawn slot for the given path.
    pub fn new(registry: Arc<AgentRegistry>, path: AgentPath) -> Self {
        let metadata = registry.ensure(&path);
        metadata.thread_count.fetch_add(1, Ordering::SeqCst);
        Self {
            registry,
            path,
            confirmed: false,
        }
    }

    /// Confirm the spawn — slot is now permanent, don't release on drop.
    pub fn confirm(&mut self) {
        self.confirmed = true;
    }

    /// Path this reservation was issued for.
    pub fn path(&self) -> &AgentPath {
        &self.path
    }

    /// Whether `confirm()` has been called.
    pub fn is_confirmed(&self) -> bool {
        self.confirmed
    }
}

impl Drop for SpawnReservation {
    fn drop(&mut self) {
        if !self.confirmed
            && let Some(metadata) = self.registry.get(&self.path)
        {
            metadata.thread_count.fetch_sub(1, Ordering::SeqCst);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registry() -> Arc<AgentRegistry> {
        Arc::new(AgentRegistry::new())
    }

    fn path() -> AgentPath {
        AgentPath::new("/root/worker").unwrap()
    }

    #[test]
    fn new_reservation_increments_thread_count() {
        let registry = registry();
        let path = path();
        let _reservation =
            SpawnReservation::new(registry.clone(), path.clone());
        let metadata = registry.get(&path).expect("metadata should exist");
        assert_eq!(metadata.thread_count(), 1);
    }

    #[test]
    fn drop_without_confirm_releases_slot() {
        let registry = registry();
        let path = path();
        {
            let _reservation =
                SpawnReservation::new(registry.clone(), path.clone());
        }
        let metadata = registry.get(&path).expect("metadata should exist");
        assert_eq!(metadata.thread_count(), 0);
    }

    #[test]
    fn confirm_marks_reservation_as_committed() {
        let registry = registry();
        let path = path();
        let mut reservation =
            SpawnReservation::new(registry.clone(), path.clone());
        assert!(!reservation.is_confirmed());
        reservation.confirm();
        assert!(reservation.is_confirmed());
    }

    #[test]
    fn confirmed_drop_keeps_slot() {
        let registry = registry();
        let path = path();
        {
            let mut reservation =
                SpawnReservation::new(registry.clone(), path.clone());
            reservation.confirm();
        }
        let metadata = registry.get(&path).expect("metadata should exist");
        assert_eq!(metadata.thread_count(), 1);
    }

    #[test]
    fn multiple_unconfirmed_reservations_release_independently() {
        let registry = registry();
        let path = path();
        {
            let _r1 = SpawnReservation::new(registry.clone(), path.clone());
            let _r2 = SpawnReservation::new(registry.clone(), path.clone());
            {
                let _r3 = SpawnReservation::new(registry.clone(), path.clone());
            }
            let metadata = registry.get(&path).expect("metadata should exist");
            assert_eq!(metadata.thread_count(), 2);
        }
        let metadata = registry.get(&path).expect("metadata should exist");
        assert_eq!(metadata.thread_count(), 0);
    }

    #[test]
    fn path_accessor_returns_reserved_path() {
        let registry = registry();
        let path = path();
        let reservation = SpawnReservation::new(registry.clone(), path.clone());
        assert_eq!(reservation.path(), &path);
    }

    #[test]
    fn unknown_path_still_tracks_metadata() {
        let registry = registry();
        let fresh = AgentPath::new("/root/fresh").unwrap();
        {
            let _reservation =
                SpawnReservation::new(registry.clone(), fresh.clone());
            let metadata = registry.get(&fresh).expect("metadata should exist");
            assert_eq!(metadata.thread_count(), 1);
        }
        let metadata = registry.get(&fresh).expect("metadata should exist");
        assert_eq!(metadata.thread_count(), 0);
    }
}
