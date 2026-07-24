//! The [`Tool`] trait impl for [`super::BashTool`].
//!
//! [`BashTool::call`] is the main entry point — it:
//!
//! 1. Rejects empty commands.
//! 2. Checks the in-tool `CommandBlacklist` (defense in
//!    depth — the registry's `PermissionChecker` has
//!    already allowed the call upstream).
//! 3. Dispatches to [`super::executor::BashTool::execute_command`]
//!    for foreground execution, or spawns a tracked
//!    background `Command` via the `CommandManager` for
//!    `run_in_background = true`.
//! 4. Translates raw `(stdout, stderr, exit_code,
//!    truncated)` into the final `ToolOutput` text or
//!    error.

use async_trait::async_trait;
use synthia_sandbox::SandboxAttempt;
use synthia_tool::{Tool, ToolInput, ToolOutput};
use tokio_util::sync::CancellationToken;

use super::{BashTool, TOOL_NAME};

#[async_trait]
impl Tool for BashTool {
    fn name(&self) -> &str {
        TOOL_NAME
    }

    fn description(&self) -> &str {
        "Executes shell commands in your environment. Supports timeout, run_in_background, and output truncation."
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
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
        })
    }

    fn requires_permission(&self) -> bool {
        true
    }

    fn is_concurrency_safe(&self) -> bool {
        // Two concurrent `bash` invocations from the same agent are
        // allowed; they target different processes and cannot race on
        // shared state. The decision is per-tool-instance, so any
        // resource contention is the caller's problem.
        true
    }

    async fn call(&self, input: ToolInput) -> ToolOutput {
        // Default delegation: callers that bypass the orchestrator's
        // sandbox-selection (tests, ad-hoc Tool::call) get the fail-closed
        // default `SandboxAttempt::None`. The orchestrator routes through
        // `call_with_sandbox` so a real `SandboxAttempt` (selected by
        // `SandboxPolicy`) reaches the bash spawn path.
        self.call_with_sandbox(
            input,
            &SandboxAttempt::None,
            &CancellationToken::new(),
        )
        .await
    }

    async fn call_with_sandbox(
        &self,
        input: ToolInput,
        sandbox: &SandboxAttempt,
        token: &CancellationToken,
    ) -> ToolOutput {
        // Use tokio::select! to allow cancellation mid-execution
        tokio::select! {
            result = self.execute_with_sandbox(input, sandbox) => result,
            _ = token.cancelled() => ToolOutput::error("operation cancelled"),
        }
    }
}

impl BashTool {
    async fn execute_with_sandbox(
        &self,
        input: ToolInput,
        sandbox: &SandboxAttempt,
    ) -> ToolOutput {
        let command = input
            .input
            .get("command")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if command.is_empty() {
            return ToolOutput::error("Empty command".to_string());
        }

        // Defense-in-depth: the `ToolRegistry` already routed the call
        // through `PermissionChecker` before reaching us, so the policy
        // decision is enforced upstream. The blacklist below is the
        // second gate: if a mis-configured policy accidentally allows
        // a destructive pattern, the blacklist still refuses it. A
        // block here is reported as an error (not a successful text
        // result) so the LLM cannot mistake a block for an execution.
        if !self.sandbox.is_command_allowed(command) {
            return ToolOutput::error(format!(
                "denied by security policy: {}",
                command
            ));
        }

        let run_in_background = input
            .input
            .get("run_in_background")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let requested_timeout = input
            .input
            .get("timeout")
            .and_then(|v| v.as_u64())
            .unwrap_or(self.default_timeout_secs);

        let timeout_secs = requested_timeout.min(self.max_timeout_secs);

        if run_in_background {
            #[cfg(unix)]
            {
                // Background spawn path MUST go through the sandbox too —
                // a detached command without `--unshare-all` /
                // `--die-with-parent` is the worst-case escape hatch (the
                // process survives the parent and keeps its full
                // filesystem write access). `build_bash_command` applies
                // `SandboxAttempt::wrap` and returns `Err` when the
                // backend is `Unavailable`; we surface that as a deny,
                // never a bare spawn.
                let mut child_cmd =
                    match Self::build_bash_command(command, sandbox) {
                        Ok(c) => c,
                        Err(e) => {
                            return ToolOutput::error(format!(
                                "denied by sandbox: {}",
                                e
                            ));
                        }
                    };
                child_cmd
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::piped());
                let child = child_cmd.spawn();
                let child = match child {
                    Ok(c) => c,
                    Err(e) => {
                        return ToolOutput::error(format!(
                            "Failed to spawn background command: {}",
                            e
                        ));
                    }
                };

                let pid = child.id();
                let id = self.command_manager.register(command, child);

                return ToolOutput::text(format!(
                    "Command started in background. ID: {}\nPID: {}",
                    id,
                    pid.unwrap_or(0)
                ));
            }

            #[cfg(not(unix))]
            {
                let _ = (command, timeout_secs, sandbox);
                return ToolOutput::error(
                    "Background execution not supported on this platform"
                        .to_string(),
                );
            }
        }

        match self.execute_command(command, timeout_secs, sandbox).await {
            Ok((stdout, stderr, exit_code, _truncated)) => {
                ToolOutput::text(Self::format_output(stdout, stderr, exit_code))
            }
            Err(e) => {
                let msg = e.to_string();
                if msg.contains("timed out") {
                    ToolOutput::error(format!(
                        "Command timed out after {} seconds",
                        timeout_secs
                    ))
                } else {
                    ToolOutput::error(format!("Command failed: {}", msg))
                }
            }
        }
    }
}
