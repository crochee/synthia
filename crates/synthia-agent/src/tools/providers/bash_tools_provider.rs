//! Built-in tool provider for bash-style tools.
//!
//! Wraps the `bash` (`BashTool`) and `Monitor` (`MonitorTool`) tools
//! defined in `synthia-tool-bash`, exposing their static metadata to
//! the dynamic provider framework.

use async_trait::async_trait;

use crate::tools::dynamic_provider::{SchemaRef, ToolDefinition, ToolProvider};

/// Provider for bash-style tools: shell command execution and background
/// output monitoring.
#[derive(Clone)]
pub struct BashToolsProvider;

impl BashToolsProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for BashToolsProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ToolProvider for BashToolsProvider {
    fn name(&self) -> &'static str {
        "bash_tools"
    }

    fn list_tools(&self) -> Vec<ToolDefinition> {
        vec![
            ToolDefinition {
                name: "bash".to_string(),
                description:
                    "Executes shell commands in your environment. Supports timeout, run_in_background, and output truncation."
                        .to_string(),
                parameters: SchemaRef::Inline(serde_json::json!({
                    "type": "object",
                    "properties": {
                        "command": {
                            "type": "string",
                            "description": "The shell command to execute."
                        },
                        "timeout": {
                            "type": "integer",
                            "description": "Timeout in seconds for this command. Default 120s, max configurable."
                        },
                        "run_in_background": {
                            "type": "boolean",
                            "description": "If true, start the command as a background task and continue working."
                        }
                    },
                    "required": ["command"]
                })),
                deprecated: None,
            },
            ToolDefinition {
                name: "Monitor".to_string(),
                description: "Runs a command in the background and feeds each output line back, so it can react to log entries, file changes, or polled status mid-conversation."
                    .to_string(),
                parameters: SchemaRef::Inline(serde_json::json!({
                    "type": "object",
                    "properties": {
                        "command": {
                            "type": "string",
                            "description": "The command to run and monitor."
                        }
                    },
                    "required": ["command"]
                })),
                deprecated: None,
            },
        ]
    }
}
