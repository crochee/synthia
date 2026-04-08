use async_trait::async_trait;
use rmcp::model::CallToolResult;
use schemars::schema_for;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{
    file_store::TaskFileStore,
    shared::{err_result, ok_result, parse_args},
};
use crate::tools::Tool;

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
struct DeleteRequest {
    task_id: String,
}

#[derive(Clone)]
pub struct TaskDeleteTool {
    store: TaskFileStore,
}

impl TaskDeleteTool {
    pub fn new() -> Self {
        Self {
            store: TaskFileStore::new(),
        }
    }
}

impl Default for TaskDeleteTool {
    fn default() -> Self {
        Self::new()
    }
}

/// Alias tool for TaskDeleteTool with name "task_stop"
#[derive(Clone)]
pub struct TaskStopTool {
    inner: TaskDeleteTool,
}

impl TaskStopTool {
    pub fn new() -> Self {
        Self {
            inner: TaskDeleteTool::new(),
        }
    }
}

impl Default for TaskStopTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for TaskStopTool {
    fn name(&self) -> &str {
        "task_stop"
    }

    fn description(&self) -> &str {
        "Stop and delete a task by ID."
    }

    fn parameters(&self) -> Value {
        self.inner.parameters()
    }

    async fn call(&self, args: Value) -> CallToolResult {
        self.inner.call(args).await
    }
}

#[async_trait]
impl Tool for TaskDeleteTool {
    fn name(&self) -> &str {
        "task_delete"
    }

    fn description(&self) -> &str {
        "Delete a task by ID."
    }

    fn parameters(&self) -> Value {
        serde_json::to_value(schema_for!(DeleteRequest)).unwrap_or_default()
    }

    async fn call(&self, args: Value) -> CallToolResult {
        let req: DeleteRequest = match parse_args(args) {
            Ok(r) => r,
            Err(e) => return e,
        };

        match self.store.delete_task(&req.task_id).await {
            Ok(_) => ok_result(&serde_json::json!({ "deleted": req.task_id })),
            Err(e) => err_result(format!("failed to delete task: {e}")),
        }
    }
}
