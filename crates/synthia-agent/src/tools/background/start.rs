//! Background start tool

use std::{path::PathBuf, sync::Arc};

use async_trait::async_trait;
use rmcp::model::CallToolResult;
use serde::Deserialize;
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, BufReader};
use uuid::Uuid;

use super::{
    data::{BackgroundTask, BackgroundTaskStatus},
    file_store::BackgroundFileStore,
};
use crate::{
    shell::{MAX_OUTPUT_LINES, ShellCommand, ShellExecutor},
    tools::Tool,
};

#[derive(Debug, Clone, Deserialize)]
struct StartRequest {
    command: String,
    #[serde(default)]
    cwd: Option<String>,
}

pub(crate) struct BackgroundStartTool {
    store: BackgroundFileStore,
    executor: Arc<dyn ShellExecutor>,
}

impl std::fmt::Debug for BackgroundStartTool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BackgroundStartTool").finish()
    }
}

impl Clone for BackgroundStartTool {
    fn clone(&self) -> Self {
        Self {
            store: self.store.clone(),
            executor: Arc::clone(&self.executor),
        }
    }
}

impl BackgroundStartTool {
    pub(crate) fn new(executor: Arc<dyn ShellExecutor>) -> Self {
        Self {
            store: BackgroundFileStore::new(),
            executor,
        }
    }
}

#[async_trait]
impl Tool for BackgroundStartTool {
    fn name(&self) -> &str {
        "background_start"
    }

    fn description(&self) -> &str {
        "Start command in background."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "Command"
                },
                "cwd": {
                    "type": "string",
                    "description": "Working directory"
                }
            },
            "required": ["command"]
        })
    }

    async fn call(&self, args: Value) -> CallToolResult {
        let request: StartRequest = match serde_json::from_value(args) {
            Ok(r) => r,
            Err(e) => {
                return CallToolResult::error(vec![
                    rmcp::model::Content::text(format!(
                        "Invalid arguments: {e}"
                    )),
                ]);
            }
        };

        let task_id = Uuid::new_v4().to_string();
        let cwd = request.cwd.unwrap_or_else(|| ".".to_string());
        let cwd_path = PathBuf::from(&cwd);

        let task = BackgroundTask::new(
            task_id.clone(),
            request.command.clone(),
            cwd.clone(),
        );
        if let Err(e) = self.store.create_task(task).await {
            return CallToolResult::error(vec![rmcp::model::Content::text(
                format!("Failed to create task: {e}"),
            )]);
        }

        let cmd = ShellCommand::new(request.command.clone(), cwd_path);
        let mut child_handle = match self.executor.spawn(cmd).await {
            Ok(h) => h,
            Err(e) => {
                return CallToolResult::error(vec![
                    rmcp::model::Content::text(format!(
                        "Failed to spawn command: {e}"
                    )),
                ]);
            }
        };

        let pid = child_handle.pid();

        let mut task = match self.store.get_task(&task_id).await {
            Ok(Some(t)) => t,
            Ok(None) => {
                return CallToolResult::error(vec![
                    rmcp::model::Content::text("Task not found".to_string()),
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
        task.pid = Some(pid);
        if let Err(e) = self.store.update_task(&task).await {
            return CallToolResult::error(vec![rmcp::model::Content::text(
                format!("Failed to update task: {e}"),
            )]);
        }

        let store = self.store.clone();
        let task_id_clone = task_id.clone();
        tokio::spawn(async move {
            let stdout = child_handle.inner().stdout.take();
            let stderr = child_handle.inner().stderr.take();

            let mut output_lines = Vec::new();
            let mut error_lines = Vec::new();

            if let Some(stdout) = stdout {
                let mut reader = BufReader::new(stdout).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    output_lines.push(line);
                    if output_lines.len() > MAX_OUTPUT_LINES {
                        output_lines.remove(0);
                    }
                }
            }

            if let Some(stderr) = stderr {
                let mut reader = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    error_lines.push(line);
                    if error_lines.len() > MAX_OUTPUT_LINES {
                        error_lines.remove(0);
                    }
                }
            }

            match child_handle.wait().await {
                Ok(Some(exit_code)) => {
                    if let Ok(Some(mut task)) =
                        store.get_task(&task_id_clone).await
                    {
                        task.output = output_lines;
                        task.error = error_lines;
                        task.complete(exit_code);
                        let _ = store.update_task(&task).await;
                    }
                }
                Ok(None) => {
                    if let Ok(Some(mut task)) =
                        store.get_task(&task_id_clone).await
                    {
                        task.output = output_lines;
                        task.error = error_lines;
                        task.status = BackgroundTaskStatus::Failed;
                        let _ = store.update_task(&task).await;
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to wait for process: {}", e);
                    if let Ok(Some(mut task)) =
                        store.get_task(&task_id_clone).await
                    {
                        task.output = output_lines;
                        task.error = error_lines;
                        task.status = BackgroundTaskStatus::Failed;
                        let _ = store.update_task(&task).await;
                    }
                }
            }
        });

        let result = serde_json::json!({
            "task_id": task_id,
            "command": request.command,
            "cwd": cwd,
            "pid": pid,
            "status": "started"
        });

        CallToolResult::success(vec![rmcp::model::Content::text(
            result.to_string(),
        )])
    }
}
