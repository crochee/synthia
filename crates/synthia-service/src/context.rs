//! OperationContext — cancellation + deadline propagation.

use std::time::Instant;

use tokio_util::sync::CancellationToken;

/// Carries cancellation, deadline, and identity through every
/// tool / permission / hook / provider call.
#[derive(Debug, Clone)]
pub struct OperationContext {
    pub cancellation: CancellationToken,
    pub deadline: Instant,
    pub session_id: String,
    pub turn_id: String,
    pub user_id: String,
    pub agent_id: String,
}

impl OperationContext {
    /// Create an OperationContext for a new session run.
    pub fn for_session(
        session_id: impl Into<String>,
        user_id: impl Into<String>,
        agent_id: impl Into<String>,
    ) -> Self {
        Self {
            cancellation: CancellationToken::new(),
            deadline: Instant::now() + std::time::Duration::from_secs(3600), // 1h default
            session_id: session_id.into(),
            turn_id: String::new(),
            user_id: user_id.into(),
            agent_id: agent_id.into(),
        }
    }

    /// Create a child context for a sub-turn or subagent.
    pub fn child(
        &self,
        session_id: impl Into<String>,
        turn_id: impl Into<String>,
    ) -> Self {
        Self {
            cancellation: self.cancellation.child_token(),
            deadline: self.deadline,
            session_id: session_id.into(),
            turn_id: turn_id.into(),
            user_id: self.user_id.clone(),
            agent_id: self.agent_id.clone(),
        }
    }

    /// Check if the operation has been cancelled.
    pub fn is_cancelled(&self) -> bool {
        self.cancellation.is_cancelled()
    }

    /// Check if the deadline has been exceeded.
    pub fn is_expired(&self) -> bool {
        Instant::now() >= self.deadline
    }
}
