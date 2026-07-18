//! GoalService — per-session goal tracking for the agent loop.
//!
//! The agent loop consults `GoalService::status()` at step 1a to
//! decide whether to continue iterating. `NoopGoalService` always
//! returns `Active` so the loop behaves identically to the legacy
//! path.

use async_trait::async_trait;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

/// Per-session goal tracking.
#[async_trait]
pub trait GoalService: Send + Sync + 'static {
    /// Return the current goal, if set.
    async fn current(&self) -> Option<Goal>;

    /// Set a new goal for this session.
    async fn set(&self, goal: Goal);

    /// Return the current status of the goal.
    async fn status(&self) -> GoalStatus;

    /// Return the remaining budget (tokens, iterations, or wall-clock).
    async fn budget(&self) -> GoalBudget;
}

/// A goal the agent is pursuing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Goal {
    /// Human-readable description.
    pub description: String,
    /// Current status.
    pub status: GoalStatus,
    /// Budget constraints.
    pub budget: GoalBudget,
}

/// Goal lifecycle status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GoalStatus {
    /// Goal is active; the loop should continue.
    Active,
    /// Goal has been achieved; the loop should stop.
    Achieved,
    /// Goal is blocked and cannot make progress.
    Blocked,
    /// Goal was abandoned by the user.
    Abandoned,
}

/// Budget constraints for goal pursuit.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GoalBudget {
    /// Maximum iterations allowed (0 = unlimited).
    pub max_iterations: u64,
    /// Maximum tokens allowed (0 = unlimited).
    pub max_tokens: u64,
    /// Iterations consumed so far.
    pub iterations_used: u64,
    /// Tokens consumed so far.
    pub tokens_used: u64,
}

impl GoalBudget {
    /// Whether the budget is exhausted.
    pub fn is_exhausted(&self) -> bool {
        (self.max_iterations > 0 && self.iterations_used >= self.max_iterations)
            || (self.max_tokens > 0 && self.tokens_used >= self.max_tokens)
    }
}

// ── Implementations ──────────────────────────────────────

/// In-memory per-session goal service.
pub struct DefaultGoalService {
    goal: Mutex<Option<Goal>>,
}

impl DefaultGoalService {
    pub fn new() -> Self {
        Self {
            goal: Mutex::new(None),
        }
    }
}

impl Default for DefaultGoalService {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl GoalService for DefaultGoalService {
    async fn current(&self) -> Option<Goal> {
        self.goal.lock().clone()
    }

    async fn set(&self, goal: Goal) {
        *self.goal.lock() = Some(goal);
    }

    async fn status(&self) -> GoalStatus {
        self.goal
            .lock()
            .as_ref()
            .map_or(GoalStatus::Active, |g| g.status)
    }

    async fn budget(&self) -> GoalBudget {
        self.goal
            .lock()
            .as_ref()
            .map(|g| g.budget.clone())
            .unwrap_or_else(GoalBudget::default)
    }
}

/// No-op goal service — always `Active`, never blocks.
///
/// Used when no goal service is configured; the loop runs until
/// it hits `max_iterations` or `cancel_token`.
pub struct NoopGoalService;

#[async_trait]
impl GoalService for NoopGoalService {
    async fn current(&self) -> Option<Goal> {
        None
    }

    async fn set(&self, _goal: Goal) {
        // No-op: ignore goal setting.
    }

    async fn status(&self) -> GoalStatus {
        GoalStatus::Active
    }

    async fn budget(&self) -> GoalBudget {
        GoalBudget::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn default_goal_service_lifecycle() {
        let svc = DefaultGoalService::new();
        assert!(svc.current().await.is_none());
        assert_eq!(svc.status().await, GoalStatus::Active);

        svc.set(Goal {
            description: "fix the bug".to_string(),
            status: GoalStatus::Active,
            budget: GoalBudget {
                max_iterations: 10,
                max_tokens: 1000,
                iterations_used: 0,
                tokens_used: 0,
            },
        })
        .await;

        assert_eq!(svc.status().await, GoalStatus::Active);
        let budget = svc.budget().await;
        assert!(!budget.is_exhausted());
    }

    #[tokio::test]
    async fn noop_goal_service() {
        let svc = NoopGoalService;
        assert!(svc.current().await.is_none());
        assert_eq!(svc.status().await, GoalStatus::Active);
        assert!(!svc.budget().await.is_exhausted());
    }

    #[tokio::test]
    async fn budget_exhaustion() {
        let budget = GoalBudget {
            max_iterations: 5,
            max_tokens: 0,
            iterations_used: 5,
            tokens_used: 0,
        };
        assert!(budget.is_exhausted());
    }
}
