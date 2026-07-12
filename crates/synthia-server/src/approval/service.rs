use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use synthia_permission::{
    ApprovalError,
    ApprovalOutcome,
    ApprovalPolicy,
    ApprovalService,
    PermissionFuture,
    PermissionRequest,
};
use tokio_util::sync::CancellationToken;

use super::state::ApprovalState;

/// An [`ApprovalService`] that blocks on an HTTP/WebSocket-mediated operator
/// decision managed by a shared [`ApprovalState`].
#[derive(Clone)]
pub struct HttpApprovalService {
    state: Arc<ApprovalState>,
}

impl HttpApprovalService {
    /// Create a new service backed by `state`.
    pub fn new(state: Arc<ApprovalState>) -> Self {
        Self { state }
    }

    /// Access the underlying approval state.
    pub fn state(&self) -> &ApprovalState {
        &self.state
    }
}

#[async_trait]
impl ApprovalService for HttpApprovalService {
    async fn request_approval(
        &self,
        tool: &str,
        args: &serde_json::Value,
        policy: ApprovalPolicy,
        timeout: Duration,
        cancel: CancellationToken,
    ) -> Result<ApprovalOutcome, ApprovalError> {
        if cancel.is_cancelled() {
            return Err(ApprovalError::Cancelled);
        }

        // A `Reject` policy is itself a decision to deny.
        if policy == ApprovalPolicy::Reject {
            return Ok(ApprovalOutcome::Deny);
        }

        let (request_id, mut outcome_rx) =
            self.state.submit(tool, args.clone());

        tokio::select! {
            _ = cancel.cancelled() => {
                let _ = self.state.cancel(&request_id);
                Err(ApprovalError::Cancelled)
            }
            _ = tokio::time::sleep(timeout) => {
                let _ = self.state.cancel(&request_id);
                Err(ApprovalError::Timeout)
            }
            result = &mut outcome_rx => {
                match result {
                    Ok(outcome) => Ok(outcome),
                    Err(_) => Err(ApprovalError::Unavailable),
                }
            }
        }
    }

    fn ask(&self, _request: PermissionRequest) -> PermissionFuture {
        // Phase 0 stub: the HTTP approval service is invoked via
        // `request_approval` (the sync path used by the orchestrator).
        // The deferred `ask` path is reserved for a follow-up change
        // that wires the operator UI into `PermissionFuture`.
        PermissionFuture::immediate_denied()
    }
}
