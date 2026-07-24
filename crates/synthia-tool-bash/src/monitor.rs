use synthia_core::Error;

use crate::command_manager::CommandManager;

pub type Result<T> = std::result::Result<T, Error>;

pub struct MonitorTool {
    command_manager: std::sync::Arc<CommandManager>,
}

impl MonitorTool {
    pub fn name() -> &'static str {
        "Monitor"
    }

    pub fn description() -> &'static str {
        "Runs a command in the background and feeds each output line back, so it can react to log entries, file changes, or polled status mid-conversation."
    }

    pub fn parameters() -> serde_json::Value {
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

    pub fn requires_permission() -> bool {
        true
    }

    pub fn new(command_manager: std::sync::Arc<CommandManager>) -> Self {
        Self { command_manager }
    }

    pub fn command_manager(&self) -> &std::sync::Arc<CommandManager> {
        &self.command_manager
    }

    pub async fn call(&self, command: &str) -> Result<String> {
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
