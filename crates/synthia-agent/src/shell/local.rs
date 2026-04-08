//! Local shell executor implementation

use std::{collections::VecDeque, process::Stdio, time::Duration};

use async_trait::async_trait;
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
    ShellExecutor,
    ShellOutput,
};

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
}

impl Default for LocalShellExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ShellExecutor for LocalShellExecutor {
    async fn execute(&self, cmd: ShellCommand) -> Result<ShellOutput> {
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

        let timeout_duration = cmd.timeout.unwrap_or(Duration::from_secs(300));

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

    async fn spawn(&self, cmd: ShellCommand) -> Result<ChildHandle> {
        let child = Self::build_command(&cmd).spawn().map_err(|e| {
            ShellError::SpawnFailed(format!("Failed to spawn command: {e}"))
        })?;

        let pid = child.id().unwrap_or(0);

        Ok(ChildHandle { pid, inner: child })
    }
}
