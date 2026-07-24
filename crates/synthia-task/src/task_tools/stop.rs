use std::sync::Arc;

use async_trait::async_trait;
use serde_json::json;
use synthia_tool::{
    traits::Tool,
    types::{ToolInput, ToolOutput},
};

use super::TaskManager;

pub struct TaskStopTool {
    manager: Arc<TaskManager>,
}

impl TaskStopTool {
    pub fn new(manager: Arc<TaskManager>) -> Self {
        Self { manager }
    }
}

#[async_trait]
impl Tool for TaskStopTool {
    fn name(&self) -> &str {
        "TaskStop"
    }

    fn description(&self) -> &str {
        "Stops a running task by ID"
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "task_id": { "type": "string", "description": "The task ID to stop" }
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
        match self.manager.stop(task_id).await {
            Some(task) => {
                ToolOutput::text(format!("Task '{}' stopped", task.id))
            }
            None => ToolOutput::text(format!("Task '{}' not found", task_id)),
        }
    }
}
