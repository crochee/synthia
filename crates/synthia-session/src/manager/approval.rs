//! Approval-timeout timer. Each `WaitingForApproval` state needs a
//! deadline; the state machine asks the manager to start/cancel
//! timers via `StateEnterEffect`.

use crate::manager::SessionManager;

impl SessionManager {
    pub async fn start_approval_timer(&self, session_id: &str) {
        let deadline = tokio::time::Instant::now() + self.approval_timeout;
        let mut timers = self.approval_timers.write().expect("RwLock poisoned");
        timers.insert(session_id.to_string(), deadline);
    }

    pub async fn cancel_approval_timer(&self, session_id: &str) {
        let mut timers = self.approval_timers.write().expect("RwLock poisoned");
        timers.remove(session_id);
    }

    pub async fn check_approval_timeout(&self, session_id: &str) -> bool {
        let timers = self.approval_timers.read().expect("RwLock poisoned");
        if let Some(deadline) = timers.get(session_id) {
            tokio::time::Instant::now() >= *deadline
        } else {
            false
        }
    }
}
