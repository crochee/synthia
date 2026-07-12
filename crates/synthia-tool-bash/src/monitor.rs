//! LLM-callable `Monitor` tool — spawns a shell command in the
//! background and returns its process id so the agent can react to
//! later log lines.
//!
//! Implements [`Tool`] so the orchestrator can route the call through
//! the standard `ToolRegistry` + `PermissionChecker` pipeline.

use async_trait::async_trait;
use synthia_core::Error;
use synthia_tool::{Tool, ToolInput, ToolOutput, traits::ExecutionMode};

use crate::command_manager::CommandManager;

pub type Result<T> = std::result::Result<T, Error>;

/// Name exposed to the LLM for the monitor tool.
pub const MONITOR_TOOL_NAME: &str = "Monitor";

pub struct MonitorTool {
    command_manager: std::sync::Arc<CommandManager>,
}

impl MonitorTool {
    pub fn new(command_manager: std::sync::Arc<CommandManager>) -> Self {
        Self { command_manager }
    }

    pub fn command_manager(&self) -> &std::sync::Arc<CommandManager> {
        &self.command_manager
    }

    /// Internal entry point used by [`Tool::call`]. Kept as a separate
    /// helper so tests and the trait impl can share one path.
    pub async fn start_monitor(&self, command: &str) -> Result<String> {
        let child = tokio::process::Command::new("bash")
            .arg("-c")
            .arg(command)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| {
                Error::ToolExecution(format!("Failed to spawn command: {}", e))
            })?;

        let pid = child.id();
        let id = self.command_manager.register(command, child);

        Ok(format!(
            "Monitoring command (ID: {}, PID: {})",
            id,
            pid.unwrap_or(0)
        ))
    }
}

#[async_trait]
impl Tool for MonitorTool {
    fn name(&self) -> &str {
        MONITOR_TOOL_NAME
    }

    fn description(&self) -> &str {
        "Runs a command in the background and feeds each output line back, so it can react to log entries, file changes, or polled status mid-conversation."
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The command to run and monitor."
                }
            },
            "required": ["command"]
        })
    }

    fn requires_permission(&self) -> bool {
        // Spawning a background process is a side effect the user
        // should approve; the registry's `PermissionChecker` will
        // surface this to the approval flow.
        true
    }

    fn is_concurrency_safe(&self) -> bool {
        // Two concurrent monitors with different commands do not share
        // state; let the orchestrator run them in parallel. Two
        // monitors with the same command are not safe (they would race
        // on the same `CommandManager` slot), so the orchestrator must
        // enforce uniqueness at a higher layer.
        true
    }

    /// The monitor tool spawns a subprocess; running two monitors in
    /// the same batch can produce surprising interleaving with bash
    /// env / cwd changes. Stay sequential at the orchestrator level.
    fn execution_mode(&self) -> ExecutionMode {
        ExecutionMode::Sequential
    }

    async fn call(&self, input: ToolInput) -> ToolOutput {
        let command = match input.input.get("command").and_then(|v| v.as_str())
        {
            Some(c) => c,
            None => {
                return ToolOutput::error(
                    "Missing required 'command' parameter".to_string(),
                );
            }
        };

        match self.start_monitor(command).await {
            Ok(text) => ToolOutput::text(text),
            Err(e) => ToolOutput::error(format!("Monitor failed: {}", e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use synthia_tool::types::ToolExecutionContext;

    use super::*;
    use crate::command_manager::CommandManager;

    fn make_input(command: Option<&str>) -> ToolInput {
        let value = match command {
            Some(c) => serde_json::json!({ "command": c }),
            None => serde_json::json!({}),
        };
        ToolInput {
            name: MONITOR_TOOL_NAME.to_string(),
            input: value,
            context: ToolExecutionContext::new(
                "session-1".to_string(),
                std::path::PathBuf::from("/tmp"),
            ),
        }
    }

    #[test]
    fn monitor_tool_exposes_expected_name_and_schema() {
        let tool = MonitorTool::new(Arc::new(CommandManager::new()));
        assert_eq!(tool.name(), MONITOR_TOOL_NAME);
        let schema = tool.parameters();
        let properties = schema.get("properties").unwrap().as_object().unwrap();
        assert!(properties.contains_key("command"));
        let required = schema.get("required").unwrap().as_array().unwrap();
        assert_eq!(required.len(), 1);
    }

    #[test]
    fn monitor_tool_requires_permission_and_is_sequential() {
        let tool = MonitorTool::new(Arc::new(CommandManager::new()));
        assert!(tool.requires_permission());
        assert_eq!(tool.execution_mode(), ExecutionMode::Sequential);
    }

    #[tokio::test]
    async fn monitor_tool_call_returns_error_for_missing_command() {
        let tool = MonitorTool::new(Arc::new(CommandManager::new()));
        let output = tool.call(make_input(None)).await;
        assert_eq!(output.is_error, Some(true));
    }
}
