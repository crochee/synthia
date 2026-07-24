use std::sync::Arc;

use async_trait::async_trait;
use serde_json::json;
use synthia_tool::{
    traits::Tool,
    types::{ToolInput, ToolOutput},
};

use super::TaskManager;

pub struct TaskUpdateTool {
    manager: Arc<TaskManager>,
}

impl TaskUpdateTool {
    pub fn new(manager: Arc<TaskManager>) -> Self {
        Self { manager }
    }
}

#[async_trait]
impl Tool for TaskUpdateTool {
    fn name(&self) -> &str {
        "TaskUpdate"
    }

    fn description(&self) -> &str {
        "Updates task status, content, or deletes tasks"
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "task_id": { "type": "string", "description": "The task ID to update" },
                "status": { "type": "string", "description": "New status (pending, running, done, failed)" },
                "content": { "type": "string", "description": "New task content" },
                "dependencies": { "type": "array", "items": { "type": "string" }, "description": "New dependencies" },
                "owner": { "type": "string", "description": "The agent ID currently executing this task (set to empty string to clear)" },
                "delete": { "type": "boolean", "description": "If true, delete the task" }
            },
            "required": ["task_id"]
        })
    }

    async fn call(&self, input: ToolInput) -> ToolOutput {
        let task_id = match input.input.get("task_id").and_then(|v| v.as_str())
        {
            Some(id) => id,
            None => return ToolOutput::error("task_id is required"),
        };
        let delete = input
            .input
            .get("delete")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if delete {
            if self.manager.delete(task_id).await {
                return ToolOutput::text(format!("Task '{}' deleted", task_id));
            } else {
                return ToolOutput::text(format!(
                    "Task '{}' not found",
                    task_id
                ));
            }
        }
        let status = input.input.get("status").and_then(|v| v.as_str());
        let content = input.input.get("content").and_then(|v| v.as_str());
        let deps: Option<Vec<String>> = input
            .input
            .get("dependencies")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            });

        let owner: Option<Option<String>> = input.input.get("owner").map(|v| {
            if v.is_null() {
                None
            } else {
                v.as_str().map(String::from)
            }
        });

        let status_enum = status.map(|s| match s.to_lowercase().as_str() {
            "pending" => crate::types::TaskStatus::Pending,
            "running" => crate::types::TaskStatus::Running,
            "done" | "completed" => crate::types::TaskStatus::Done,
            "failed" => crate::types::TaskStatus::Failed,
            "blocked" => crate::types::TaskStatus::Blocked,
            _ => crate::types::TaskStatus::Pending,
        });

        match self
            .manager
            .update(task_id, status_enum, content, deps, owner)
            .await
        {
            Some(task) => ToolOutput::text(format!(
                "Task updated: {} - {:?} ({:.0}%)",
                task.id,
                task.status,
                task.completion_percentage()
            )),
            None => ToolOutput::text(format!("Task '{}' not found", task_id)),
        }
    }
}
