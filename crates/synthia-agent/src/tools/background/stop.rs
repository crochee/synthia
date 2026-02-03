//! Background stop tool

use async_trait::async_trait;
use rmcp::model::CallToolResult;
use serde::Deserialize;
use serde_json::Value;

use super::{data::BackgroundTaskStatus, file_store::BackgroundFileStore};
use crate::tools::Tool;

#[derive(Debug, Clone, Deserialize)]
struct StopRequest {
    task_id: String,
}

pub(crate) struct BackgroundStopTool {
    store: BackgroundFileStore,
}

impl std::fmt::Debug for BackgroundStopTool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BackgroundStopTool").finish()
    }
}

impl Clone for BackgroundStopTool {
    fn clone(&self) -> Self {
        Self {
            store: self.store.clone(),
        }
    }
}

impl BackgroundStopTool {
    pub(crate) fn new() -> Self {
        Self {
            store: BackgroundFileStore::new(),
        }
    }
}

impl Default for BackgroundStopTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for BackgroundStopTool {
    fn name(&self) -> &str {
        "background_stop"
    }

    fn description(&self) -> &str {
        "Stop background task."
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
        let request: StopRequest = match serde_json::from_value(args) {
            Ok(r) => r,
            Err(e) => {
                return CallToolResult::error(vec![
                    rmcp::model::Content::text(format!(
                        "Invalid arguments: {e}"
                    )),
                ]);
            }
        };

        let mut task = match self.store.get_task(&request.task_id).await {
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

        if !task.is_running() {
            return CallToolResult::success(vec![rmcp::model::Content::text(
                serde_json::json!({
                    "success": false,
                    "message": format!("Task '{}' is not running (status: {})", request.task_id, task.status)
                })
                .to_string(),
            )]);
        }

        if task.pid.is_some() {
            tracing::info!(
                "Task {} will be stopped (PID: {:?})",
                task.id,
                task.pid
            );
        }

        task.status = BackgroundTaskStatus::Stopped;
        task.ended_at = Some(chrono::Utc::now().timestamp());
        if let Err(e) = self.store.update_task(&task).await {
            return CallToolResult::error(vec![rmcp::model::Content::text(
                format!("Failed to update task: {e}"),
            )]);
        }

        let result = serde_json::json!({
            "success": true,
            "task_id": task.id,
            "message": format!("Task '{}' has been stopped", request.task_id)
        });

        CallToolResult::success(vec![rmcp::model::Content::text(
            result.to_string(),
        )])
    }
}
