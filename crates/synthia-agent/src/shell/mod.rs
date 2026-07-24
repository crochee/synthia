//! Shell execution module
//!
//! Provides unified shell command execution with support for
//! different runtimes (local, sandbox, etc.).
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────┐
//! │   Tool Layer    │  (exec, background_start, etc.)
//! └────────┬────────┘
//!          │
//!          ▼
//! ┌─────────────────┐
//! │ ShellExecutor   │  Trait for shell execution
//! └────────┬────────┘
//!          │
//!          ▼
//! ┌─────────────────┐
//! │LocalShellExecutor│  Local process execution
//! └─────────────────┘
//! ```

mod local;
mod security;

use std::{path::PathBuf, time::Duration};

pub use local::LocalShellExecutor;
pub use security::{SecurityCheckResult, check_command_safety};

pub const MAX_OUTPUT_LINES: usize = 100;

/// Shell 超时上限（秒），任何超时都不能超过此值
pub const MAX_SHELL_TIMEOUT_SECS: u64 = 600;

#[derive(Debug, Clone)]
pub struct ShellCommand {
    pub command: String,
    pub cwd: PathBuf,
    pub timeout: Option<Duration>,
}

impl ShellCommand {
    pub fn new(command: String, cwd: PathBuf) -> Self {
        Self {
            command,
            cwd,
            timeout: None,
        }
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        // Enforce upper bound: no timeout can exceed MAX_SHELL_TIMEOUT_SECS
        let capped = timeout.min(Duration::from_secs(MAX_SHELL_TIMEOUT_SECS));
        self.timeout = Some(capped);
        self
    }
}

#[derive(Debug, Clone)]
pub struct ShellOutput {
    pub exit_code: i32,
    pub stdout: Vec<String>,
    pub stderr: Vec<String>,
}

impl ShellOutput {
    pub fn stdout_text(&self) -> String {
        self.stdout.join("\n")
    }

    pub fn stderr_text(&self) -> String {
        self.stderr.join("\n")
    }

    pub fn is_success(&self) -> bool {
        self.exit_code == 0
    }
}

// `ShellExecutor` trait REMOVED 2026-06-15 in change
// `2026-06-15-p2-trait-cleanup` because it had 0 trait-bound usage,
// 0 dyn dispatch, and exactly 1 real implementation (`LocalShellExecutor`).
// The `execute` and `spawn` methods are now inherent on `LocalShellExecutor`.

#[derive(Debug)]
pub struct ChildHandle {
    pid: u32,
    inner: tokio::process::Child,
}

impl ChildHandle {
    pub fn pid(&self) -> u32 {
        self.pid
    }

    pub fn inner(&mut self) -> &mut tokio::process::Child {
        &mut self.inner
    }

    pub async fn wait(mut self) -> Result<Option<i32>> {
        let status = self.inner.wait().await?;
        Ok(status.code())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ShellError {
    #[error("Failed to spawn process: {0}")]
    SpawnFailed(String),
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Timed out after {0}s")]
    Timeout(u64),
    #[error("Failed to read output: {0}")]
    ReadError(String),
}

pub type Result<T> = std::result::Result<T, ShellError>;
