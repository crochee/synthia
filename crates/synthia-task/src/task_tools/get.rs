use std::sync::Arc;

use async_trait::async_trait;
use serde_json::json;
use synthia_tool::{
    traits::Tool,
    types::{ToolInput, ToolOutput},
};

use super::TaskManager;

pub struct TaskGetTool {
    manager: Arc<TaskManager>,
}

impl TaskGetTool {
    pub fn new(manager: Arc<TaskManager>) -> Self {
        Self { manager }
    }
}

#[async_trait]
impl Tool for TaskGetTool {
    fn name(&self) -> &str {
        "TaskGet"
    }

    fn description(&self) -> &str {
        "Retrieves full details for a specific task"
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "task_id": { "type": "string", "description": "The task ID to retrieve" }
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
        match self.manager.get(task_id).await {
            Some(task) => ToolOutput::text(format!(
                "ID: {}\nDescription: {}\nStatus: {:?}\nProgress: {:.0}%\nOwner: {:?}",
                task.id,
                task.description,
                task.status,
                task.completion_percentage(),
                task.owner,
            )),
            None => ToolOutput::text(format!("Task '{}' not found", task_id)),
        }
    }
}
