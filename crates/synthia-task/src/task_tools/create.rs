use std::sync::Arc;

use async_trait::async_trait;
use serde_json::json;
use synthia_tool::{
    traits::Tool,
    types::{ToolInput, ToolOutput},
};

use super::TaskManager;

pub struct TaskCreateTool {
    manager: Arc<TaskManager>,
}

impl TaskCreateTool {
    pub fn new(manager: Arc<TaskManager>) -> Self {
        Self { manager }
    }
}

#[async_trait]
impl Tool for TaskCreateTool {
    fn name(&self) -> &str {
        "TaskCreate"
    }

    fn description(&self) -> &str {
        "Creates a new task in the task list"
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "content": { "type": "string", "description": "The task content/description" },
                "dependencies": { "type": "array", "items": { "type": "string" }, "description": "List of task IDs this task depends on" },
                "owner": { "type": "string", "description": "The agent ID currently executing this task" }
            },
            "required": ["content"]
        })
    }

    async fn call(&self, input: ToolInput) -> ToolOutput {
        let content = match input.input.get("content").and_then(|v| v.as_str())
        {
            Some(c) => c,
            None => return ToolOutput::error("content is required"),
        };
        let deps: Vec<String> = input
            .input
            .get("dependencies")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        let owner = input
            .input
            .get("owner")
            .and_then(|v| v.as_str())
            .map(String::from);
        match self.manager.create(content, deps, owner).await {
            Some(task) => ToolOutput::text(format!(
                "Task created: {} - {}",
                task.id, task.description
            )),
            None => ToolOutput::error("Failed to create task"),
        }
    }
}
