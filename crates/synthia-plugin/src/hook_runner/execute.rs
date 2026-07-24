//! The actual hook execution:
//!
//! - [`execute_hook`]: dispatches on [`HookHandler`] (Command vs
//!   Prompt). `Prompt` handlers currently log and return
//!   `HookResult::Continue` (a future LLM integration will
//!   replace this stub).
//! - [`execute_command`]: parses the command into program + args
//!   (preventing shell injection), blocks known dangerous
//!   interpreters (sh, bash, rm, dd, mkfs, …), and runs with
//!   `tokio::time::timeout`.

use std::{
    path::{Path, PathBuf},
    process::Command,
};

use tokio::time;

use super::{
    core::HookRunner,
    types::{HookMetadata, HookRunnerError},
};
use crate::types::{HookHandler, HookResult, HookSpec};

pub(super) async fn execute_hook(
    runner: &HookRunner,
    config: &HookSpec,
    metadata: &HookMetadata,
) -> Result<HookResult, HookRunnerError> {
    match &config.handler {
        HookHandler::Command(cmd) => execute_command(runner, cmd, config).await,
        HookHandler::Prompt(prompt) => {
            // For now, Prompt handlers just log/return Continue
            // In a full implementation, this would call an LLM
            tracing::debug!(
                "Prompt hook triggered: target={:?}, prompt={}",
                metadata.target,
                prompt
            );
            Ok(HookResult::Continue)
        }
    }
}

pub(super) async fn execute_command(
    runner: &HookRunner,
    cmd: &str,
    _config: &HookSpec,
) -> Result<HookResult, HookRunnerError> {
    let timeout_secs = runner.default_timeout;

    // Parse command string into program and arguments
    // SECURITY: This prevents shell injection by executing the command directly
    // instead of through "sh -c". The command string is split into program + args.
    let parts: Vec<&str> = cmd.split_whitespace().collect();
    if parts.is_empty() {
        return Err(HookRunnerError::ExecutionFailed(
            "Empty command".to_string(),
        ));
    }

    let program = parts[0].to_string();
    let args: Vec<String> = parts[1..].iter().map(|s| s.to_string()).collect();

    // Block known shell interpreters and dangerous commands
    let blocked = [
        "sh", "bash", "zsh", "dash", "fish", "ksh", "csh", "rm", "dd", "mkfs",
        "fdisk", "sfdisk",
    ];
    let program_ref = &program;
    if blocked.iter().any(|b| program_ref == *b || cmd.contains(b)) {
        return Err(HookRunnerError::ExecutionFailed(format!(
            "Command '{program}' is not allowed for security reasons"
        )));
    }

    // Resolve absolute program path via `base_dir` for plugins
    // that ship their own binaries.
    let program_path = resolve_program_path(runner, &program);

    let output = time::timeout(
        std::time::Duration::from_secs(timeout_secs),
        async move {
            let handle = tokio::task::spawn_blocking(move || {
                let mut cmd_builder = Command::new(&program_path);
                cmd_builder.args(&args);
                cmd_builder.output()
            });
            match handle.await {
                Ok(Ok(output)) => Ok(output),
                Ok(Err(e)) => Err(HookRunnerError::ExecutionFailed(format!(
                    "Command failed: {e}"
                ))),
                Err(_) => Err(HookRunnerError::Timeout(timeout_secs)),
            }
        },
    )
    .await
    .map_err(|_| HookRunnerError::Timeout(timeout_secs))??;

    if output.status.success() {
        Ok(HookResult::Continue)
    } else {
        Ok(HookResult::Failed)
    }
}

/// Resolve the absolute path of a program using the runner's
/// `base_dir` (so plugin-shipped binaries can be invoked without
/// being on `$PATH`). If the user already passed an absolute path
/// or the file isn't found under `base_dir`, fall back to the raw
/// program name.
fn resolve_program_path(
    runner: &HookRunner,
    program: &str,
) -> std::path::PathBuf {
    let p = Path::new(program);
    if p.is_absolute() {
        return p.to_path_buf();
    }
    if !runner.base_dir.as_os_str().is_empty() {
        let candidate = runner.base_dir.join(program);
        if candidate.is_file() {
            return candidate;
        }
    }
    PathBuf::from(program)
}
