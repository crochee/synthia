//! The [`BashTool::execute_command`] low-level helper.
//!
//! Runs `bash -c <command>` with a tokio timeout, returns
//! `(stdout, stderr, exit_code, truncated)`. stdout and
//! stderr are UTF-8 decoded with the
//! [`super::cap_to_char_boundary`] safety net on overflow
//! to ensure we never land on an invalid byte boundary.
//!
//! On non-unix platforms, returns a mock result so
//! downstream `Tool::call` logic can still be exercised in
//! cross-platform unit tests.

use std::time::Duration;

use synthia_core::Error;
use tokio::process::Command;

use super::Result;

impl super::BashTool {
    /// Execute a command and return (stdout, stderr, exit_code, truncated). This is a low-level helper exposed for testing; the `Tool::call`
    /// implementation in this file builds a `ToolOutput` on top of it.
    #[cfg(unix)]
    pub async fn execute_command(
        &self,
        command: &str,
        timeout_secs: u64,
        sandbox: &synthia_sandbox::SandboxAttempt,
    ) -> Result<(String, String, i32, bool)> {
        let mut cmd = Self::build_bash_command(command, sandbox)?;
        let timeout = tokio::time::timeout(
            Duration::from_secs(timeout_secs),
            cmd.output(),
        )
        .await;

        match timeout {
            Ok(Ok(output)) => {
                let stdout =
                    String::from_utf8_lossy(&output.stdout).to_string();
                let stderr =
                    String::from_utf8_lossy(&output.stderr).to_string();
                let exit_code = output.status.code().unwrap_or(-1);

                let stdout_truncated = stdout.len() > self.max_output_length;
                let stderr_truncated = stderr.len() > self.max_output_length;

                let stdout = if stdout_truncated {
                    let mut s = stdout;
                    super::cap_to_char_boundary(&mut s, self.max_output_length);
                    format!(
                        "{}\n\n[stdout truncated at {} bytes]",
                        s, self.max_output_length
                    )
                } else {
                    stdout
                };

                let stderr = if stderr_truncated {
                    let mut s = stderr;
                    super::cap_to_char_boundary(&mut s, self.max_output_length);
                    format!(
                        "{}\n\n[stderr truncated at {} bytes]",
                        s, self.max_output_length
                    )
                } else {
                    stderr
                };

                Ok((
                    stdout,
                    stderr,
                    exit_code,
                    stdout_truncated || stderr_truncated,
                ))
            }
            Ok(Err(e)) => Err(Error::ToolExecution(format!(
                "Failed to execute command: {}",
                e
            ))),
            Err(_) => Err(Error::ToolExecution(format!(
                "Command timed out after {} seconds",
                timeout_secs
            ))),
        }
    }

    #[cfg(not(unix))]
    pub async fn execute_command(
        &self,
        command: &str,
        _timeout_secs: u64,
        _sandbox: &synthia_sandbox::SandboxAttempt,
    ) -> Result<(String, String, i32, bool)> {
        Ok((
            format!("Command executed (mock): {}", command),
            String::new(),
            0,
            false,
        ))
    }

    /// Build a `bash -c <command>` [`Command`] and apply the orchestrator's
    /// selected sandbox via [`synthia_sandbox::SandboxAttempt::wrap`].
    ///
    /// Returns `Err` when the sandbox cannot be applied (e.g. the backend is
    /// `Unavailable`). The caller MUST surface that error as a deny rather
    /// than bare-running the command — bash is the only tool that executes
    /// arbitrary attacker-influenced code, so fail-closed (P6) is mandatory.
    ///
    /// Exposed as a separate seam so the "wrap was called" contract can be
    /// verified without executing the command (and without depending on
    /// `bwrap` being installed on the test host).
    #[cfg(unix)]
    pub fn build_bash_command(
        command: &str,
        sandbox: &synthia_sandbox::SandboxAttempt,
    ) -> Result<Command> {
        // Spec: bash-sandbox-application Scenario 3 — when the
        // orchestrator-selected sandbox is `None` (policy explicitly
        // allows bare execution, e.g. test environments), the bash
        // executor MAY run without a sandbox BUT MUST emit an audit
        // warning so the disable is never silent (P6: distrust by
        // default — surface the policy decision in logs).
        if matches!(sandbox, synthia_sandbox::SandboxAttempt::None) {
            tracing::warn!("sandbox disabled by policy");
        }
        let mut cmd = Command::new("bash");
        cmd.arg("-c").arg(command);
        // Apply the orchestrator-selected sandbox before the command is
        // executed. `wrap` rewrites the program to `bwrap` (or installs a
        // landlock ruleset via pre_exec) and returns `Err` when the backend
        // is `Unavailable`; the caller MUST surface that as a deny, not a
        // bare run — bash executes attacker-influenced code.
        sandbox.wrap(&mut cmd).map_err(|e| {
            Error::ToolExecution(format!(
                "sandbox unavailable ({}): bash execution denied to prevent bare-run",
                e.code
            ))
        })?;
        Ok(cmd)
    }

    /// Internal: build the human-readable content string from raw
    /// execution results. Mirrors the format the old
    /// `BashCallResult::text` produced, but as a plain String.
    pub(super) fn format_output(
        stdout: String,
        stderr: String,
        exit_code: i32,
    ) -> String {
        let mut content = String::new();
        if !stdout.is_empty() {
            content.push_str(&stdout);
        }
        if !stderr.is_empty() {
            if !content.is_empty() {
                content.push_str("\n--- stderr ---\n");
            }
            content.push_str(&stderr);
        }

        if content.is_empty() {
            content =
                format!("Command completed with exit code: {}", exit_code);
        }

        if exit_code != 0 {
            content = format!("Exit code: {}\n{}", exit_code, content);
        }

        content
    }
}
