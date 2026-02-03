//! Background status tool

use async_trait::async_trait;
use rmcp::model::CallToolResult;
use serde::Deserialize;
use serde_json::Value;

use super::file_store::BackgroundFileStore;
use crate::tools::Tool;

#[derive(Debug, Clone, Deserialize)]
struct StatusRequest {
    task_id: String,
}

pub(crate) struct BackgroundStatusTool {
    store: BackgroundFileStore,
}

impl std::fmt::Debug for BackgroundStatusTool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BackgroundStatusTool").finish()
    }
}

impl Clone for BackgroundStatusTool {
    fn clone(&self) -> Self {
        Self {
            store: self.store.clone(),
        }
    }
}

impl BackgroundStatusTool {
    pub(crate) fn new() -> Self {
        Self {
            store: BackgroundFileStore::new(),
        }
    }
}

impl Default for BackgroundStatusTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for BackgroundStatusTool {
    fn name(&self) -> &str {
        "background_status"
    }

    fn description(&self) -> &str {
        "Check background task status."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "task_id": {
                    "type": "string",
                    "description": "Task ID"
                }
            },
            "required": ["task_id"]
        })
    }

    async fn call(&self, args: Value) -> CallToolResult {
        let request: StatusRequest = match serde_json::from_value(args) {
            Ok(r) => r,
            Err(e) => {
                return CallToolResult::error(vec![
                    rmcp::model::Content::text(format!(
                        "Invalid arguments: {e}"
                    )),
                ]);
            }
        };

        let task = match self.store.get_task(&request.task_id).await {
            Ok(Some(t)) => t,
            Ok(None) => {
                return CallToolResult::error(vec![
                    rmcp::model::Content::text(format!(
                        "Task '{}' not found",
                        request.task_id
                    )),
                ]);
            }
            Err(e) => {
                return CallToolResult::error(vec![
                    rmcp::model::Content::text(format!(
                        "Failed to get task: {e}"
                    )),
                ]);
            }
        };

        let result = serde_json::json!({
            "task_id": task.id,
            "command": task.command,
            "status": task.status.to_string(),
            "pid": task.pid,
            "started_at": task.started_at,
            "ended_at": task.ended_at,
            "exit_code": task.exit_code,
            "output_lines": task.output.len(),
            "error_lines": task.error.len(),
        });

        CallToolResult::success(vec![rmcp::model::Content::text(
            result.to_string(),
        )])
    }
}
