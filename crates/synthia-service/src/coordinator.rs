//! SessionRunCoordinator — multi-run arbitration for long-running sessions.

use std::{collections::HashMap, sync::Arc, time::Instant};

use parking_lot::Mutex;
use tokio_util::sync::CancellationToken;

use crate::traits::ServiceError;

/// Per-session run state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunState {
    Idle,
    Running { run_id: u64 },
    Interrupted { at: Instant },
}

/// Unique run identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RunId(pub u64);

/// Session key for coordinator lookups.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SessionKey(pub String);

/// Multi-run arbitration primitive. Holds the canonical
/// "who is running in this session" map so the loop can
/// reject duplicate runs and serialize wakeups.
pub struct SessionRunCoordinator {
    inner: Mutex<HashMap<SessionKey, RunState>>,
    next_run_id: Mutex<u64>,
    cancellation_tokens: Mutex<HashMap<SessionKey, Arc<CancellationToken>>>,
}

impl SessionRunCoordinator {
    /// Create an empty coordinator.
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
            next_run_id: Mutex::new(1),
            cancellation_tokens: Mutex::new(HashMap::new()),
        }
    }

    /// Start a run on `key`. Returns `Err(AlreadyRunning)` if a run is active.
    pub fn run(&self, key: SessionKey) -> Result<RunGuard<'_>, ServiceError> {
        let mut inner = self.inner.lock();
        match inner.get(&key) {
            Some(RunState::Running { .. }) => Err(ServiceError::AlreadyRunning),
            _ => {
                let run_id = {
                    let mut next = self.next_run_id.lock();
                    let id = *next;
                    *next += 1;
                    id
                };
                let cancel_token = Arc::new(CancellationToken::new());
                self.cancellation_tokens
                    .lock()
                    .insert(key.clone(), cancel_token);
                inner.insert(key.clone(), RunState::Running { run_id });
                Ok(RunGuard {
                    coordinator: self,
                    key,
                    run_id: RunId(run_id),
                })
            }
        }
    }

    /// Wake a sleeping session.
    pub fn wake(&self, key: &SessionKey) -> Result<RunId, ServiceError> {
        let inner = self.inner.lock();
        match inner.get(key) {
            Some(RunState::Running { run_id }) => Ok(RunId(*run_id)),
            Some(RunState::Idle)
            | Some(RunState::Interrupted { .. })
            | None => Err(ServiceError::NoSuchRun),
        }
    }

    /// Interrupt a running session cooperatively.
    pub fn interrupt(&self, key: &SessionKey) -> Result<(), ServiceError> {
        let tokens = self.cancellation_tokens.lock();
        if let Some(token) = tokens.get(key) {
            token.cancel();
            drop(tokens);
            let mut inner = self.inner.lock();
            inner.insert(
                key.clone(),
                RunState::Interrupted { at: Instant::now() },
            );
            Ok(())
        } else {
            Err(ServiceError::NoSuchRun)
        }
    }

    /// Block until the run for `key` reaches Idle.
    pub async fn await_idle(&self, key: &SessionKey) {
        // Spin-wait with yield. In practice, the loop transitions
        // to Idle quickly after cancellation or completion.
        loop {
            {
                let inner = self.inner.lock();
                if matches!(inner.get(key), Some(RunState::Idle) | None) {
                    return;
                }
            }
            tokio::task::yield_now().await;
        }
    }

    fn transition_to_idle(&self, key: &SessionKey) {
        self.inner.lock().insert(key.clone(), RunState::Idle);
        self.cancellation_tokens.lock().remove(key);
    }
}

impl Default for SessionRunCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

/// RAII guard that transitions the session to Idle on drop.
pub struct RunGuard<'a> {
    coordinator: &'a SessionRunCoordinator,
    key: SessionKey,
    run_id: RunId,
}

impl RunGuard<'_> {
    pub fn run_id(&self) -> RunId {
        self.run_id
    }

    pub fn cancellation_token(&self) -> Option<Arc<CancellationToken>> {
        self.coordinator
            .cancellation_tokens
            .lock()
            .get(&self.key)
            .cloned()
    }
}

impl Drop for RunGuard<'_> {
    fn drop(&mut self) {
        self.coordinator.transition_to_idle(&self.key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_transitions_to_idle_on_drop() {
        let coord = SessionRunCoordinator::new();
        let key = SessionKey("session-1".to_string());
        {
            let guard = coord.run(key.clone()).unwrap();
            assert_eq!(guard.run_id(), RunId(1));
            // State should be Running
            let inner = coord.inner.lock();
            assert!(matches!(
                inner.get(&key),
                Some(RunState::Running { run_id: 1 })
            ));
        }
        // After drop, state should be Idle
        let inner = coord.inner.lock();
        assert!(matches!(inner.get(&key), Some(RunState::Idle)));
    }

    #[test]
    fn duplicate_run_rejected() {
        let coord = SessionRunCoordinator::new();
        let key = SessionKey("session-1".to_string());
        let _guard = coord.run(key.clone()).unwrap();
        // Second run on same key should fail
        let result = coord.run(key.clone());
        assert!(matches!(result, Err(ServiceError::AlreadyRunning)));
    }

    #[test]
    fn parallel_subagent_runs() {
        let coord = SessionRunCoordinator::new();
        let key1 = SessionKey("session-1".to_string());
        let key2 = SessionKey("session-2".to_string());

        let guard1 = coord.run(key1.clone()).unwrap();
        let guard2 = coord.run(key2.clone()).unwrap();

        // Both should have unique run IDs
        assert_ne!(guard1.run_id(), guard2.run_id());

        // Wake should succeed for both
        assert!(coord.wake(&key1).is_ok());
        assert!(coord.wake(&key2).is_ok());

        drop(guard1);
        drop(guard2);

        // Both should be Idle now
        let inner = coord.inner.lock();
        assert!(matches!(inner.get(&key1), Some(RunState::Idle)));
        assert!(matches!(inner.get(&key2), Some(RunState::Idle)));
    }

    #[tokio::test]
    async fn interrupt_and_await_idle() {
        let coord = SessionRunCoordinator::new();
        let key = SessionKey("session-1".to_string());

        let guard = coord.run(key.clone()).unwrap();
        let token = guard.cancellation_token().unwrap();

        // Interrupt the run
        coord.interrupt(&key).unwrap();
        assert!(token.is_cancelled());

        // Drop the guard to transition to Idle
        drop(guard);

        // await_idle should return immediately
        coord.await_idle(&key).await;
    }

    #[test]
    fn wake_nonexistent_session() {
        let coord = SessionRunCoordinator::new();
        let key = SessionKey("nonexistent".to_string());
        assert!(matches!(coord.wake(&key), Err(ServiceError::NoSuchRun)));
    }
}
