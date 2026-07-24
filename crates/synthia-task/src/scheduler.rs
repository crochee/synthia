use std::collections::VecDeque;

use tokio::sync::RwLock;

use crate::types::{Task, TaskStatus};

/// Default iteration budget for a task.
pub const DEFAULT_BUDGET: usize = 90;
/// First warning threshold (70%).
pub const WARN_AT_70: usize = 63;
/// Second warning threshold (90%).
pub const WARN_AT_90: usize = 81;

/// Scheduling result with optional warnings.
pub enum ScheduleOutcome {
    /// Task may continue executing.
    Continue,
    /// Budget exhausted, task must stop.
    BudgetExhausted,
    /// Task completed (done or failed).
    Completed(TaskStatus),
}

/// Manages task scheduling with iteration budget enforcement.
///
/// Budget:
/// - Default: 90 iterations
/// - 63 (70%): first warning
/// - 81 (90%): second warning
/// - 90 (100%): forced stop
pub struct TaskScheduler {
    default_budget: usize,
    task_budgets: RwLock<VecDeque<usize>>,
}

impl TaskScheduler {
    pub fn new(default_budget: usize) -> Self {
        Self {
            default_budget,
            task_budgets: RwLock::new(VecDeque::new()),
        }
    }

    pub fn with_budget(budget: usize) -> Self {
        Self::new(budget)
    }

    /// Push a new budget onto the stack (for nested/sub-tasks).
    pub async fn push_budget(&self, budget: usize) {
        self.task_budgets.write().await.push_back(budget);
    }

    /// Pop the current budget (when a sub-task completes).
    pub async fn pop_budget(&self) {
        self.task_budgets.write().await.pop_back();
    }

    /// Get the current active budget.
    async fn current_budget(&self) -> usize {
        self.task_budgets
            .read()
            .await
            .back()
            .copied()
            .unwrap_or(self.default_budget)
    }

    /// Check the budget for a task and return the scheduling outcome.
    pub async fn check_budget(
        &self,
        task: &Task,
        iterations: usize,
    ) -> ScheduleOutcome {
        // If the task is already done or failed, no need to check budget
        match task.status {
            TaskStatus::Done | TaskStatus::Failed => {
                return ScheduleOutcome::Completed(task.status);
            }
            _ => {}
        }

        let budget = self.current_budget().await;

        if iterations >= budget {
            tracing::warn!(
                task_id = task.id,
                iterations,
                budget,
                "Task budget exhausted, forcing stop"
            );
            ScheduleOutcome::BudgetExhausted
        } else {
            // Emit warnings at thresholds
            if iterations >= WARN_AT_90 {
                tracing::warn!(
                    task_id = task.id,
                    iterations,
                    budget,
                    "Task at 90% budget (second warning)"
                );
            } else if iterations >= WARN_AT_70 {
                tracing::info!(
                    task_id = task.id,
                    iterations,
                    budget,
                    "Task at 70% budget (first warning)"
                );
            }
            ScheduleOutcome::Continue
        }
    }
}

impl Default for TaskScheduler {
    fn default() -> Self {
        Self::new(DEFAULT_BUDGET)
    }
}
