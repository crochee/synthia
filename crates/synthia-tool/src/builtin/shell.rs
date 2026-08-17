//! `shell` built-in tool — execute a shell command.
//!
//! Safety contract (per `mvp-shell-safety` spec):
//! - Default deny patterns block obviously destructive commands.
//! - The check can be disabled via [`ShellSafetyConfig::disabled`].

use std::time::Duration;

use async_trait::async_trait;
use schemars_derive::JsonSchema;
use serde::Deserialize;
use tokio::process::Command;

use crate::{
    traits::{ExecutionMode, Tool},
    types::{Context, ToolOutput},
};

/// Default deny-pattern list. Substring match (case-sensitive).
pub const DEFAULT_DENY_PATTERNS: &[&str] = &[
    "rm -rf",
    "mkfs",
    "dd if=",
    "chmod -R 777",
    "chmod -R 7777",
    "sudo ",
    "curl ",
    "wget ",
    ">/dev/sd",
    ":(){:|:&};:",
];

#[derive(Debug, Clone, Default)]
pub struct ShellSafetyConfig {
    pub disabled: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(extend("additionalProperties" = false))]
struct ShellArgs {
    #[schemars(
        description = "The shell command to execute (passed to `sh -c`)."
    )]
    command: String,
    #[serde(default)]
    #[schemars(
        range(min = 1),
        extend("default" = 30),
        description = "Maximum execution time in seconds. Default: 30."
    )]
    timeout_secs: Option<u64>,
}

#[derive(Debug)]
pub struct ShellTool {
    config: ShellSafetyConfig,
}

impl ShellTool {
    pub fn new() -> Self {
        Self {
            config: ShellSafetyConfig::default(),
        }
    }

    pub fn with_config(config: ShellSafetyConfig) -> Self {
        Self { config }
    }

    fn is_denied(&self, command: &str) -> bool {
        if self.config.disabled {
            return false;
        }
        DEFAULT_DENY_PATTERNS
            .iter()
            .any(|pattern| command.contains(pattern))
    }
}

impl Default for ShellTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for ShellTool {
    fn name(&self) -> &str {
        "shell"
    }

    fn description(&self) -> &str {
        "Execute a shell command and return its stdout (and stderr on non-zero exit)."
    }

    fn parameters(&self) -> serde_json::Value {
        // Schema is generated from `ShellArgs` via `schemars`,
        // so the type and the LLM-facing schema cannot drift —
        // including `additionalProperties: false` and the
        // `timeout_secs` default, all declared inline via
        // `#[schemars(extend(...))]`.
        serde_json::to_value(schemars::schema_for!(ShellArgs))
            .expect("ShellArgs schema is always serializable")
    }

    fn mode(&self) -> ExecutionMode {
        ExecutionMode::Sequential
    }

    async fn call(
        &self,
        input: serde_json::Value,
        _context: &Context,
    ) -> ToolOutput {
        let args: ShellArgs = match serde_json::from_value(input) {
            Ok(a) => a,
            Err(e) => {
                return ToolOutput::error(format!("Invalid arguments: {}", e));
            }
        };

        if self.is_denied(&args.command) {
            return ToolOutput::error(format!(
                "Command denied by safety policy (matches default deny pattern): {}",
                args.command
            ));
        }

        let timeout = Duration::from_secs(args.timeout_secs.unwrap_or(30));
        let result = tokio::time::timeout(
            timeout,
            Command::new("sh").arg("-c").arg(&args.command).output(),
        )
        .await;

        match result {
            Ok(Ok(output)) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);
                let status = output.status.code().unwrap_or(-1);
                let mut body =
                    format!("exit: {}\n\nstdout:\n{}", status, stdout);
                if !stderr.is_empty() {
                    body.push_str(&format!("\n\nstderr:\n{}", stderr));
                }
                if status != 0 {
                    ToolOutput::error(body)
                } else {
                    ToolOutput::text(body)
                }
            }
            Ok(Err(e)) => {
                ToolOutput::error(format!("Failed to spawn shell: {}", e))
            }
            Err(_) => ToolOutput::error(format!(
                "Command timed out after {}s",
                timeout.as_secs()
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use serde_json::json;

    use super::*;

    fn make_context() -> Context {
        Context::new("s1".to_string(), PathBuf::from("/tmp"))
    }

    #[test]
    fn is_denied_blocks_rm_rf() {
        let tool = ShellTool::new();
        assert!(tool.is_denied("rm -rf /tmp/foo"));
    }

    #[test]
    fn is_denied_blocks_mkfs() {
        let tool = ShellTool::new();
        assert!(tool.is_denied("mkfs.ext4 /dev/sda1"));
    }

    #[test]
    fn is_denied_blocks_fork_bomb() {
        let tool = ShellTool::new();
        assert!(tool.is_denied(":(){:|:&};:"));
    }

    #[test]
    fn is_denied_allows_safe_commands() {
        let tool = ShellTool::new();
        assert!(!tool.is_denied("ls -la /tmp"));
        assert!(!tool.is_denied("cat /etc/hostname"));
        assert!(!tool.is_denied("echo hello"));
    }

    #[test]
    fn is_denied_disabled_allows_dangerous_commands() {
        let tool = ShellTool::with_config(ShellSafetyConfig { disabled: true });
        assert!(!tool.is_denied("rm -rf /"));
    }

    #[tokio::test]
    async fn safe_command_returns_text_output() {
        let tool = ShellTool::new();
        let out = tool
            .call(json!({"command": "echo hello"}), &make_context())
            .await;
        let text = out
            .content
            .iter()
            .find_map(|c| match c {
                synthia_provider::types::ContentPart::Text(t) => {
                    Some(t.text.clone())
                }
                _ => None,
            })
            .unwrap_or_default();
        assert!(text.contains("hello"));
        assert!(out.is_error.is_none() || !out.is_error.unwrap_or(false));
    }

    #[tokio::test]
    async fn denied_command_returns_error() {
        let tool = ShellTool::new();
        let out = tool
            .call(json!({"command": "rm -rf /tmp"}), &make_context())
            .await;
        assert!(out.is_error.unwrap_or(false));
    }

    #[tokio::test]
    async fn non_zero_exit_returns_error() {
        let tool = ShellTool::new();
        let out = tool
            .call(json!({"command": "false"}), &make_context())
            .await;
        assert!(out.is_error.unwrap_or(false));
    }

    /// Pin the JSON-Schema shape for `shell` so future drift in
    /// either the schema, the typed `ShellArgs`, or the runtime
    /// `#[serde(default)]` semantics breaks here instead of at
    /// the LLM boundary.
    #[test]
    fn parameters_schema_is_self_consistent() {
        let tool = ShellTool::new();
        let params = tool.parameters();
        assert_eq!(params["type"], "object");
        let required: Vec<&str> = params["required"]
            .as_array()
            .expect("required")
            .iter()
            .filter_map(|v| v.as_str())
            .collect();
        assert_eq!(required, vec!["command"]);

        let props = params["properties"].as_object().expect("properties");
        let cmd = props["command"].as_object().expect("command");
        assert_eq!(cmd["type"], "string");
        assert!(cmd["description"].as_str().is_some());

        let timeout = props["timeout_secs"].as_object().expect("timeout_secs");
        let ty = &timeout["type"];
        assert!(
            ty == "integer"
                || ty.as_array().is_some_and(|arr| {
                    arr.iter().any(|v| v == "integer")
                        && arr.iter().any(|v| v == "null")
                }),
            "timeout_secs type should be integer or [integer, null], got: {ty}"
        );
        assert_eq!(
            timeout["minimum"].as_f64().unwrap() as u64,
            1,
            "timeout_secs must require a positive integer"
        );
        assert_eq!(
            timeout["default"], 30,
            "timeout_secs schema default must match runtime default"
        );

        assert_eq!(
            params["additionalProperties"], false,
            "additional fields must be rejected to match serde_json::from_value"
        );
    }
}
