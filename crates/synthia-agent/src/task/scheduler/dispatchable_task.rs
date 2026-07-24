use std::time::Duration;

use crate::task::types::{DEFAULT_TASK_TIMEOUT, TaskContext, TaskPriority};

/// A pending task ready to be dispatched.
#[derive(Clone, Debug)]
pub struct DispatchableTask {
    pub id: String,
    pub priority: TaskPriority,
    pub context: TaskContext,
    pub timeout: Duration,
    pub workspace_root: std::path::PathBuf,
}

impl DispatchableTask {
    pub fn new(
        id: String,
        context: TaskContext,
        workspace_root: std::path::PathBuf,
    ) -> Self {
        Self {
            id,
            priority: TaskPriority::default(),
            context,
            timeout: DEFAULT_TASK_TIMEOUT,
            workspace_root,
        }
    }

    pub fn with_priority(mut self, priority: TaskPriority) -> Self {
        self.priority = priority;
        self
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
}
