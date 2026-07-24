use serde::{Deserialize, Serialize};

use crate::types::{Task, TaskStatus};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TaskFilter {
    pub status: Option<TaskStatus>,
}

impl TaskFilter {
    pub fn accepts(&self, item: &Task) -> bool {
        if let Some(ref status) = self.status
            && item.status != *status
        {
            return false;
        }
        true
    }
}
