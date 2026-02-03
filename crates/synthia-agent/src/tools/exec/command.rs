//! Exec command tool implementation
//!
//! Execute shell commands with timeout control.
//! Note: Approval is handled by the Guardian system externally,
//! not by this tool. The Guardian reviews commands before they reach execution.

use std::{path::PathBuf, sync::Arc, time::Duration};

use async_trait::async_trait;
use rmcp::model::{CallToolResult, Content};
use serde::Deserialize;
use serde_json::Value;

use crate::{
    shell::{ShellCommand, ShellExecutor},
    tools::Tool,
};

const MAX_TIMEOUT: u64 = 5 * 60;

#[derive(Debug, Clone, Deserialize)]
struct ExecRequest {
    command: String,
    #[serde(default, alias = "cwd")]
    current_dir: Option<String>,
    #[serde(default)]
    timeout: Option<u64>,
}

#[derive(Clone)]
pub struct ExecTool {
    executor: Arc<dyn ShellExecutor>,
}

impl ExecTool {
    pub fn new(executor: Arc<dyn ShellExecutor>) -> Self {
        Self { executor }
    }
}

impl std::fmt::Debug for ExecTool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExecTool").finish()
    }
}

#[async_trait]
impl Tool for ExecTool {
    fn name(&self) -> &str {
        "Bash"
    }

    fn description(&self) -> &str {
        "Execute shell commands."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "Command"
                },
                "current_dir": {
                    "type": "string",
                    "description": "Working directory"
                },
                "timeout": {
                    "type": "integer",
                    "description": "Timeout in seconds",
                    "default": 60,
                    "minimum": 1,
                    "maximum": 300
                }
            },
            "required": ["command"]
        })
    }

    async fn call(&self, args: Value) -> CallToolResult {
        let request: ExecRequest = match serde_json::from_value(args) {
            Ok(r) => r,
            Err(e) => {
                return CallToolResult::error(vec![Content::text(format!(
                    "Invalid arguments: {e}"
                ))]);
            }
        };

        let timeout_seconds = request.timeout.unwrap_or(60).min(MAX_TIMEOUT);
        let cwd = request.current_dir.unwrap_or_else(|| ".".to_string());
        let timeout = Duration::from_secs(timeout_seconds);

        let cmd = ShellCommand::new(request.command, PathBuf::from(cwd))
            .with_timeout(timeout);

        let output = match self.executor.execute(cmd).await {
            Ok(o) => o,
            Err(e) => {
                return CallToolResult::error(vec![Content::text(format!(
                    "Command execution failed: {e}"
                ))]);
            }
        };

        let exit_code = output.exit_code;
        let mut result_parts = vec![format!("Exit code: {exit_code}")];

        if !output.stdout.is_empty() {
            result_parts.push("\nStdout:".to_string());
            result_parts.push(output.stdout_text());
        }

        if !output.stderr.is_empty() {
            result_parts.push("\nStderr:".to_string());
            result_parts.push(output.stderr_text());
        }

        CallToolResult::success(vec![Content::text(result_parts.join("\n"))])
    }
}
