//! Background list tool

use async_trait::async_trait;
use rmcp::model::CallToolResult;
use serde_json::Value;

use super::file_store::BackgroundFileStore;
use crate::tools::Tool;

pub(crate) struct BackgroundListTool {
    store: BackgroundFileStore,
}

impl std::fmt::Debug for BackgroundListTool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BackgroundListTool").finish()
    }
}

impl Clone for BackgroundListTool {
    fn clone(&self) -> Self {
        Self {
            store: self.store.clone(),
        }
    }
}

impl BackgroundListTool {
    pub(crate) fn new() -> Self {
        Self {
            store: BackgroundFileStore::new(),
        }
    }
}

impl Default for BackgroundListTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for BackgroundListTool {
    fn name(&self) -> &str {
        "background_list"
    }

    fn description(&self) -> &str {
        "List background tasks."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }

    async fn call(&self, _args: Value) -> CallToolResult {
        let tasks = match self.store.list_tasks().await {
            Ok(t) => t,
            Err(e) => {
                return CallToolResult::error(vec![
                    rmcp::model::Content::text(format!(
                        "Failed to list tasks: {e}"
                    )),
                ]);
            }
        };

        let task_summaries: Vec<_> = tasks
            .into_iter()
            .map(|task| {
                serde_json::json!({
                    "task_id": task.id,
                    "command": task.command,
                    "status": task.status.to_string(),
                    "pid": task.pid,
                    "started_at": task.started_at,
                    "ended_at": task.ended_at,
                    "exit_code": task.exit_code,
                })
            })
            .collect();

        let result = serde_json::json!({
            "tasks": task_summaries,
            "count": task_summaries.len()
        });

        CallToolResult::success(vec![rmcp::model::Content::text(
            result.to_string(),
        )])
    }
}
