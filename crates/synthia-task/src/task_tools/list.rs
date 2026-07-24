use std::sync::Arc;

use async_trait::async_trait;
use serde_json::json;
use synthia_tool::{
    traits::Tool,
    types::{ToolInput, ToolOutput},
};

use super::TaskManager;

pub struct TaskListTool {
    manager: Arc<TaskManager>,
}

impl TaskListTool {
    pub fn new(manager: Arc<TaskManager>) -> Self {
        Self { manager }
    }
}

#[async_trait]
impl Tool for TaskListTool {
    fn name(&self) -> &str {
        "TaskList"
    }

    fn description(&self) -> &str {
        "Lists all tasks with their current status"
    }

    fn parameters(&self) -> serde_json::Value {
        json!({ "type": "object", "properties": {} })
    }

    async fn call(&self, _input: ToolInput) -> ToolOutput {
        let tasks = self.manager.list().await;
        if tasks.is_empty() {
            return ToolOutput::text("No tasks".to_string());
        }
        let mut output = String::new();
        for task in &tasks {
            output.push_str(&format!(
                "[{}] {:?} - {} ({:.0}%) @{}\n",
                task.id,
                task.status,
                task.description,
                task.completion_percentage(),
                task.owner.as_deref().unwrap_or("-")
            ));
        }
        ToolOutput::text(output)
    }
}
