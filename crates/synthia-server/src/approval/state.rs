use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use synthia_permission::ApprovalOutcome;
use tokio::sync::{broadcast, oneshot};

/// An in-flight approval request waiting for operator resolution.
#[derive(Debug)]
pub struct PendingApproval {
    pub request_id: String,
    pub tool_name: String,
    pub arguments: serde_json::Value,
    pub requested_at: Instant,
    /// Guards against resolving the same request twice.
    pub resolved: Arc<AtomicBool>,
    /// Channel used to deliver the resolution back to the waiting
    /// [`ApprovalService::request_approval`] call.
    pub outcome_tx: oneshot::Sender<ApprovalOutcome>,
}

/// Serializable summary of a pending approval request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalListItem {
    pub request_id: String,
    pub tool_name: String,
    pub arguments: serde_json::Value,
    pub requested_at: String,
}

/// Events broadcast to WebSocket subscribers.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ApprovalEvent {
    Submitted {
        request_id: String,
        tool_name: String,
        arguments: serde_json::Value,
        requested_at: String,
    },
    Resolved {
        request_id: String,
        outcome: ApprovalOutcome,
    },
    Snapshot {
        approvals: Vec<ApprovalListItem>,
    },
}

/// Shared state holding pending approvals and a broadcast channel for updates.
#[derive(Clone)]
pub struct ApprovalState {
    pending: Arc<DashMap<String, PendingApproval>>,
    broadcast_tx: broadcast::Sender<ApprovalEvent>,
}

impl Default for ApprovalState {
    fn default() -> Self {
        Self::new()
    }
}

impl ApprovalState {
    /// Create an empty approval state with a bounded broadcast channel.
    pub fn new() -> Self {
        let (broadcast_tx, _) = broadcast::channel(256);
        Self {
            pending: Arc::new(DashMap::new()),
            broadcast_tx,
        }
    }

    /// Submit a new approval request and return its generated ID together
    /// with a oneshot receiver for the eventual outcome.
    pub fn submit(
        &self,
        tool_name: &str,
        arguments: serde_json::Value,
    ) -> (String, oneshot::Receiver<ApprovalOutcome>) {
        let request_id = uuid::Uuid::new_v4().to_string();
        let (outcome_tx, outcome_rx) = oneshot::channel();
        let requested_at = Instant::now();

        let pending = PendingApproval {
            request_id: request_id.clone(),
            tool_name: tool_name.to_string(),
            arguments: arguments.clone(),
            requested_at,
            resolved: Arc::new(AtomicBool::new(false)),
            outcome_tx,
        };
        self.pending.insert(request_id.clone(), pending);

        let event = ApprovalEvent::Submitted {
            request_id: request_id.clone(),
            tool_name: tool_name.to_string(),
            arguments,
            requested_at: format_instant(requested_at),
        };
        let _ = self.broadcast_tx.send(event);

        (request_id, outcome_rx)
    }

    /// Resolve a pending approval with the given outcome.
    ///
    /// Returns `true` if the request existed and was resolved by this call.
    pub fn resolve(&self, request_id: &str, outcome: ApprovalOutcome) -> bool {
        let Some((_, pending)) = self.pending.remove(request_id) else {
            return false;
        };
        if pending.resolved.swap(true, Ordering::SeqCst) {
            return false;
        }

        let _ = pending.outcome_tx.send(outcome);
        let _ = self.broadcast_tx.send(ApprovalEvent::Resolved {
            request_id: request_id.to_string(),
            outcome,
        });
        true
    }

    /// Cancel a pending approval, signalling denial to the waiter.
    ///
    /// Returns `true` if the request existed and was cancelled by this call.
    pub fn cancel(&self, request_id: &str) -> bool {
        let Some((_, pending)) = self.pending.remove(request_id) else {
            return false;
        };
        if pending.resolved.swap(true, Ordering::SeqCst) {
            return false;
        }

        let _ = pending.outcome_tx.send(ApprovalOutcome::Deny);
        let _ = self.broadcast_tx.send(ApprovalEvent::Resolved {
            request_id: request_id.to_string(),
            outcome: ApprovalOutcome::Deny,
        });
        true
    }

    /// List all currently pending approvals.
    pub fn list_pending(&self) -> Vec<ApprovalListItem> {
        self.pending
            .iter()
            .filter(|entry| !entry.resolved.load(Ordering::SeqCst))
            .map(|entry| ApprovalListItem {
                request_id: entry.request_id.clone(),
                tool_name: entry.tool_name.clone(),
                arguments: entry.arguments.clone(),
                requested_at: format_instant(entry.requested_at),
            })
            .collect()
    }

    /// Subscribe to approval events.
    pub fn subscribe(&self) -> broadcast::Receiver<ApprovalEvent> {
        self.broadcast_tx.subscribe()
    }
}

/// Best-effort conversion of an [`Instant`] to an RFC 3339 UTC timestamp.
fn format_instant(instant: Instant) -> String {
    let elapsed_ms = instant.elapsed().as_millis() as i64;
    let now_ms = chrono::Utc::now().timestamp_millis();
    chrono::DateTime::from_timestamp_millis(now_ms - elapsed_ms)
        .unwrap_or_else(chrono::Utc::now)
        .to_rfc3339()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn submit_creates_pending_entry() {
        let state = ApprovalState::new();
        let (request_id, rx) =
            state.submit("write_file", serde_json::json!({ "path": "x.txt" }));

        assert!(!request_id.is_empty());

        let pending = state.list_pending();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].request_id, request_id);
        assert_eq!(pending[0].tool_name, "write_file");

        // Avoid leaving the receiver dangling.
        drop(rx);
    }

    #[tokio::test]
    async fn resolve_sends_outcome_and_removes_pending() {
        let state = ApprovalState::new();
        let (request_id, rx) =
            state.submit("read_file", serde_json::json!({ "path": "y.txt" }));

        assert!(state.resolve(&request_id, ApprovalOutcome::Approve));

        let outcome = rx.await.expect("receiver should get outcome");
        assert_eq!(outcome, ApprovalOutcome::Approve);
        assert!(state.list_pending().is_empty());
    }

    #[test]
    fn resolve_unknown_request_returns_false() {
        let state = ApprovalState::new();
        assert!(!state.resolve("missing", ApprovalOutcome::Deny));
    }

    #[tokio::test]
    async fn cancel_signals_deny_and_removes_pending() {
        let state = ApprovalState::new();
        let (request_id, rx) = state
            .submit("run_command", serde_json::json!({ "cmd": "echo hi" }));

        assert!(state.cancel(&request_id));

        let outcome = rx.await.expect("receiver should get outcome");
        assert_eq!(outcome, ApprovalOutcome::Deny);
        assert!(state.list_pending().is_empty());
    }

    #[tokio::test]
    async fn double_resolve_is_noop() {
        let state = ApprovalState::new();
        let (request_id, rx) =
            state.submit("write_file", serde_json::json!({ "path": "z.txt" }));

        assert!(state.resolve(&request_id, ApprovalOutcome::Approve));
        assert!(!state.resolve(&request_id, ApprovalOutcome::Deny));

        let outcome = rx.await.expect("receiver should get outcome");
        assert_eq!(outcome, ApprovalOutcome::Approve);
    }
}
