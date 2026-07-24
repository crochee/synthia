//! Local shell executor implementation

use std::{collections::VecDeque, process::Stdio, time::Duration};

use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Command,
    time::timeout,
};

use super::{
    ChildHandle,
    MAX_OUTPUT_LINES,
    Result,
    ShellCommand,
    ShellError,
    ShellOutput,
};

/// 默认 Shell 超时（秒）
const DEFAULT_SHELL_TIMEOUT_SECS: u64 = 60;
/// 最大 Shell 超时上限（秒），任何超时都不能超过此值
const MAX_SHELL_TIMEOUT_SECS: u64 = 600;

pub struct LocalShellExecutor;

impl LocalShellExecutor {
    pub fn new() -> Self {
        Self
    }

    fn is_powershell() -> bool {
        std::env::consts::OS == "windows"
    }

    fn get_shell() -> (&'static str, &'static str) {
        if Self::is_powershell() {
            ("powershell", "-command")
        } else {
            ("bash", "-c")
        }
    }

    fn build_command(cmd: &ShellCommand) -> tokio::process::Command {
        let (shell, arg_flag) = Self::get_shell();
        let cwd = cmd.cwd.to_string_lossy().to_string();

        let mut tokio_cmd = Command::new(shell);
        tokio_cmd
            .arg(arg_flag)
            .arg(&cmd.command)
            .current_dir(&cwd)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        tokio_cmd
    }

    pub async fn execute(&self, cmd: ShellCommand) -> Result<ShellOutput> {
        let mut child = Self::build_command(&cmd).spawn().map_err(|e| {
            ShellError::SpawnFailed(format!("Failed to spawn command: {e}"))
        })?;

        let stdout = child.stdout.take().ok_or_else(|| {
            ShellError::SpawnFailed("Failed to capture stdout".to_string())
        })?;
        let stderr = child.stderr.take().ok_or_else(|| {
            ShellError::SpawnFailed("Failed to capture stderr".to_string())
        })?;

        let mut stdout_reader = BufReader::new(stdout).lines();
        let mut stderr_reader = BufReader::new(stderr).lines();

        let mut stdout_lines: VecDeque<String> = VecDeque::new();
        let mut stderr_lines: VecDeque<String> = VecDeque::new();

        let timeout_duration = cmd
            .timeout
            .map(|t| {
                // 强制上限：任何超时不能超过 MAX_SHELL_TIMEOUT_SECS
                t.min(Duration::from_secs(MAX_SHELL_TIMEOUT_SECS))
            })
            .unwrap_or(Duration::from_secs(DEFAULT_SHELL_TIMEOUT_SECS));

        let result = timeout(timeout_duration, async {
            while let Some(line) =
                stdout_reader.next_line().await.map_err(|e| {
                    ShellError::ReadError(format!("Failed to read stdout: {e}"))
                })?
            {
                stdout_lines.push_back(line);
                if stdout_lines.len() > MAX_OUTPUT_LINES {
                    stdout_lines.pop_front();
                }
            }

            while let Some(line) =
                stderr_reader.next_line().await.map_err(|e| {
                    ShellError::ReadError(format!("Failed to read stderr: {e}"))
                })?
            {
                stderr_lines.push_back(line);
                if stderr_lines.len() > MAX_OUTPUT_LINES {
                    stderr_lines.pop_front();
                }
            }

            let status = child.wait().await.map_err(ShellError::IoError)?;

            Ok::<_, ShellError>(status)
        })
        .await;

        let exit_code = match result {
            Ok(Ok(status)) => status.code().unwrap_or(-1),
            Ok(Err(e)) => return Err(e),
            Err(_) => {
                return Err(ShellError::Timeout(timeout_duration.as_secs()));
            }
        };

        Ok(ShellOutput {
            exit_code,
            stdout: stdout_lines.into_iter().collect(),
            stderr: stderr_lines.into_iter().collect(),
        })
    }

    pub async fn spawn(&self, cmd: ShellCommand) -> Result<ChildHandle> {
        let child = Self::build_command(&cmd).spawn().map_err(|e| {
            ShellError::SpawnFailed(format!("Failed to spawn command: {e}"))
        })?;

        let pid = child.id().unwrap_or(0);

        Ok(ChildHandle { pid, inner: child })
    }
}

impl Default for LocalShellExecutor {
    fn default() -> Self {
        Self::new()
    }
}
