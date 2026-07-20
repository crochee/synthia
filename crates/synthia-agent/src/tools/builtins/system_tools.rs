//! System tools (bash execution, etc.).
use std::{path::PathBuf, process::Stdio, sync::Arc, time::Duration};

use async_trait::async_trait;
use nix::{
    sys::signal::{Signal, killpg},
    unistd::Pid,
};
use synthia_tool_bash::CommandBlacklist;
use synthia_tool_orchestrator::{
    ExecutableTool,
    ExecutionContext,
    ToolCallRequest,
    ToolCallResult,
    ToolExecutionError,
};
use tokio::{io::AsyncReadExt, process::Command};
use tokio_util::sync::CancellationToken;

const DEFAULT_TIMEOUT_SECS: u64 = 60;
const MAX_TIMEOUT_SECS: u64 = 3600;
const MAX_OUTPUT_BYTES: usize = 1_048_576;
/// SIGTERM 后等待进程优雅退出的宽限期，超时后升级为 SIGKILL。
const SIGTERM_GRACE_PERIOD: Duration = Duration::from_secs(3);
/// 超时/取消后 drain 子进程 IO 的最大等待时间。
const IO_DRAIN_TIMEOUT: Duration = Duration::from_secs(2);

/// Returns an [`ExecutableTool`] for sandboxed bash execution.
///
/// The returned tool runs `bash -c <command>` inside the orchestrator's
/// selected sandbox profile via [`synthia_sandbox::SandboxAttempt::wrap`].
/// It also applies the `synthia_tool_bash::CommandBlacklist` as a
/// defense-in-depth layer.
pub fn bash_tool(
    workspace_root: impl Into<PathBuf>,
) -> Option<Arc<dyn ExecutableTool>> {
    let blacklist = CommandBlacklist::new(workspace_root.into());
    Some(Arc::new(SandboxedBashTool { blacklist }))
}

#[derive(Debug, Clone)]
struct SandboxedBashTool {
    blacklist: CommandBlacklist,
}

#[async_trait]
impl ExecutableTool for SandboxedBashTool {
    fn name(&self) -> &str {
        "bash"
    }

    fn is_concurrency_safe(&self) -> bool {
        true
    }

    async fn execute(
        &self,
        request: &ToolCallRequest,
        _context: &ExecutionContext,
        sandbox_attempt: &synthia_sandbox::SandboxAttempt,
        cancellation_token: CancellationToken,
    ) -> Result<ToolCallResult, ToolExecutionError> {
        let command = request
            .arguments
            .get("command")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if command.is_empty() {
            return Err(ToolExecutionError::Permanent(
                "Empty command".to_string(),
            ));
        }

        if !self.blacklist.is_command_allowed(command) {
            return Err(ToolExecutionError::Permanent(format!(
                "denied by security policy: {}",
                command
            )));
        }

        let run_in_background = request
            .arguments
            .get("run_in_background")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        // TODO: implement background execution via CommandManager with
        // sandbox support and cancellation propagation.
        if run_in_background {
            return Err(ToolExecutionError::Permanent(
                "Background execution is not supported by the sandboxed bash tool"
                    .to_string(),
            ));
        }

        let requested_timeout = request
            .arguments
            .get("timeout")
            .and_then(|v| v.as_u64())
            .unwrap_or(DEFAULT_TIMEOUT_SECS);

        let timeout_secs = requested_timeout.min(MAX_TIMEOUT_SECS);
        let timeout = Duration::from_secs(timeout_secs);

        let mut cmd = Command::new("bash");
        cmd.arg("-c").arg(command);
        // 创建新的进程组（pgid = 子进程 pid），便于超时/取消时
        // 用 killpg 杀掉整个进程树，避免孙子进程被 init 收养成为孤儿。
        cmd.process_group(0);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        cmd.stdin(Stdio::null());

        sandbox_attempt
            .wrap(&mut cmd)
            .map_err(|e| ToolExecutionError::Permanent(e.to_string()))?;

        let mut child = cmd.spawn().map_err(|e| {
            ToolExecutionError::Permanent(format!(
                "Failed to execute command: {}",
                e
            ))
        })?;
        let child_pid =
            child.id().expect("spawned child must have a pid") as i32;

        let wait_result = tokio::select! {
            biased;
            _ = cancellation_token.cancelled() => {
                kill_process_group(child_pid);
                let _ = drain_io(&mut child).await;
                return Err(ToolExecutionError::Cancelled);
            }
            result = tokio::time::timeout(timeout, child.wait()) => result,
        };

        let status = match wait_result {
            Ok(Ok(status)) => status,
            Ok(Err(e)) => {
                return Err(ToolExecutionError::Permanent(format!(
                    "Failed to execute command: {}",
                    e
                )));
            }
            Err(_) => {
                kill_process_group(child_pid);
                let _ = drain_io(&mut child).await;
                return Err(ToolExecutionError::Permanent(format!(
                    "Command timed out after {} seconds",
                    timeout_secs
                )));
            }
        };

        // wait() 返回后读取剩余的 stdout/stderr（管道已 EOF）。
        let mut stdout_buf = Vec::new();
        let mut stderr_buf = Vec::new();
        if let Some(mut stdout) = child.stdout.take() {
            let _ = stdout.read_to_end(&mut stdout_buf).await;
        }
        if let Some(mut stderr) = child.stderr.take() {
            let _ = stderr.read_to_end(&mut stderr_buf).await;
        }

        let exit_code = status.code().unwrap_or(-1);

        let mut stdout = String::from_utf8_lossy(&stdout_buf).to_string();
        let mut stderr = String::from_utf8_lossy(&stderr_buf).to_string();

        truncate_output(&mut stdout, MAX_OUTPUT_BYTES, "stdout");
        truncate_output(&mut stderr, MAX_OUTPUT_BYTES, "stderr");

        let content = format_output(stdout, stderr, exit_code);
        let outcome = serde_json::Value::String(content);

        Ok(ToolCallResult {
            call_id: request.call_id.clone(),
            tool_name: request.tool_name.clone(),
            outcome,
            is_error: false,
            tool_id: request.tool_id,
        })
    }
}

/// Cap `output` to at most `max_bytes` using UTF-8-safe truncation,
/// appending a human-readable truncation marker when truncation occurs.
///
/// No-op when `output.len() <= max_bytes`. The marker reports the
/// configured `max_bytes` (not the actual post-boundary length, which
/// may be slightly smaller when the cap lands inside a multi-byte
/// character).
fn truncate_output(output: &mut String, max_bytes: usize, stream_name: &str) {
    if output.len() > max_bytes {
        synthia_core::cap_to_char_boundary(output, max_bytes);
        output.push_str(&format!(
            "\n\n[{} truncated at {} bytes]",
            stream_name, max_bytes
        ));
    }
}

/// Build the human-readable content string from raw execution results.
/// Mirrors the format produced by `synthia_tool_bash::BashTool::format_output`.
fn format_output(stdout: String, stderr: String, exit_code: i32) -> String {
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
        content = format!("Command completed with exit code: {}", exit_code);
    }

    if exit_code != 0 {
        content = format!("Exit code: {}\n{}", exit_code, content);
    }

    content
}

/// 杀掉整个进程组：先 SIGTERM 给宽限期，再 SIGKILL 强杀。
///
/// `child_pid` 是 bash 子进程的 pid，由于设置了 `process_group(0)`，
/// 该 pid 同时也是进程组 leader 的 pgid。killpg 会向该组内所有进程
/// 发送信号，包括 bash 衍生出的孙子进程（如 `sleep 1000 &`）。
fn kill_process_group(child_pid: i32) {
    let pgid = Pid::from_raw(child_pid);
    // 先 SIGTERM 让进程有机会优雅退出。
    let _ = killpg(pgid, Signal::SIGTERM);
    // 等待宽限期。
    std::thread::sleep(SIGTERM_GRACE_PERIOD);
    // 强杀剩余进程，确保无孤儿。
    let _ = killpg(pgid, Signal::SIGKILL);
}

/// 超时/取消后 drain 子进程 IO，避免管道阻塞和僵尸进程。
///
/// 依次读取 stdout/stderr（各受 `IO_DRAIN_TIMEOUT` 限制），最后
/// wait 子进程以回收资源。
async fn drain_io(child: &mut tokio::process::Child) -> Result<(), ()> {
    if let Some(mut stdout) = child.stdout.take() {
        let mut buf = Vec::new();
        let _ = tokio::time::timeout(
            IO_DRAIN_TIMEOUT,
            stdout.read_to_end(&mut buf),
        )
        .await;
    }
    if let Some(mut stderr) = child.stderr.take() {
        let mut buf = Vec::new();
        let _ = tokio::time::timeout(
            IO_DRAIN_TIMEOUT,
            stderr.read_to_end(&mut buf),
        )
        .await;
    }
    let _ = child.wait().await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timeout_constants_are_reasonable() {
        // 宽限期必须为正，否则 SIGTERM 没有机会生效。
        assert!(SIGTERM_GRACE_PERIOD.as_secs() > 0);
        // IO drain 超时必须为正，否则无法读取残留输出。
        assert!(IO_DRAIN_TIMEOUT.as_secs() > 0);
        // 宽限期应短于默认超时，避免拖长取消路径。
        assert!(
            SIGTERM_GRACE_PERIOD < Duration::from_secs(DEFAULT_TIMEOUT_SECS)
        );
    }

    /// 回归测试：1MB 上限以下的输出不应被截断。
    #[test]
    fn test_output_under_1mb_not_truncated() {
        // 1_000_000 字节 < MAX_OUTPUT_BYTES (1_048_576)。
        let mut output = "a".repeat(1_000_000);
        let original = output.clone();
        truncate_output(&mut output, MAX_OUTPUT_BYTES, "stdout");
        assert_eq!(output, original, "output under 1MB must not be truncated");
    }

    /// 回归测试：超过 1MB 上限的输出应被截断，保留 head 并追加截断标记。
    #[test]
    fn test_output_over_1mb_truncated() {
        // 1_100_000 字节 > MAX_OUTPUT_BYTES (1_048_576)。
        let head = "a".repeat(MAX_OUTPUT_BYTES);
        let mut output = head.clone();
        output.push_str(&"b".repeat(51_424)); // 1_048_576 + 51_424 = 1_100_000
        assert!(output.len() > MAX_OUTPUT_BYTES);

        truncate_output(&mut output, MAX_OUTPUT_BYTES, "stdout");

        // 截断后总长度应小于原始长度（head + marker < 原始）。
        assert!(
            output.len() < 1_100_000,
            "output must be truncated, got len={}",
            output.len()
        );
        // 必须包含截断标记。
        assert!(
            output.contains("[stdout truncated at 1048576 bytes]"),
            "truncation marker must be present, got: {}",
            &output[output.len().saturating_sub(100)..]
        );
        // ASCII 下字符边界 == 字节边界，head 必须完整保留。
        assert!(
            output.starts_with(&head),
            "head of output (first {} bytes) must be preserved",
            MAX_OUTPUT_BYTES
        );
    }

    /// 回归测试：1MB 边界恰好落在多字节 UTF-8 字符中间时，
    /// 截断后仍必须是有效 UTF-8，且多字节字符不能被拆分。
    #[test]
    fn test_utf8_safety_at_1mb_boundary() {
        // 构造 (MAX_OUTPUT_BYTES - 1) 个 ASCII + 一个 4 字节 emoji，
        // 使 1MB 边界落在 emoji 的第 2 个字节上。
        //   bytes 0..1_048_575         : ASCII 'a'
        //   bytes 1_048_575..1_048_579 : '😀' (4 字节, F0 9F 98 80)
        //   boundary 1_048_576         : emoji 第 2 字节 (continuation)
        let mut output = String::with_capacity(1_100_000);
        output.push_str(&"a".repeat(MAX_OUTPUT_BYTES - 1));
        output.push('😀');
        output.push_str(&"b".repeat(50_000));
        assert!(output.len() > MAX_OUTPUT_BYTES);

        truncate_output(&mut output, MAX_OUTPUT_BYTES, "stdout");

        // 必须是有效 UTF-8（核心断言：不会 panic / 不会产生乱码）。
        assert!(
            std::str::from_utf8(output.as_bytes()).is_ok(),
            "truncated output must be valid UTF-8"
        );
        // 多字节字符必须被完整丢弃，不能残留半个字符。
        assert!(
            !output.contains('😀'),
            "multi-byte char straddling the boundary must be fully dropped"
        );
        // 截断标记必须存在。
        assert!(
            output.contains("[stdout truncated at 1048576 bytes]"),
            "truncation marker must be present"
        );
        // ASCII head 必须完整保留。
        assert!(
            output.starts_with(&"a".repeat(MAX_OUTPUT_BYTES - 1)),
            "ASCII head before the multi-byte char must be preserved"
        );
    }

    /// 集成测试：验证超时杀进程组后不留下孤儿 `sleep` 进程。
    ///
    /// 标记 `#[ignore]` 是因为它依赖系统 `pgrep` 工具且会真实
    /// 创建/杀进程，不适合在 CI 中默认运行。
    #[tokio::test]
    #[ignore]
    async fn kill_process_group_leaves_no_orphan() {
        use std::process::Stdio as StdStdio;

        use tokio::process::Command as TokioCommand;

        // 使用一个极不常见的 sleep 时长作为进程标记，避免误匹配。
        let marker = "sleep 9876543";
        let mut cmd = TokioCommand::new("bash");
        cmd.arg("-c").arg(format!("{} &", marker));
        cmd.process_group(0);
        cmd.stdout(StdStdio::piped());
        cmd.stderr(StdStdio::piped());
        cmd.stdin(StdStdio::null());

        let mut child = cmd.spawn().expect("failed to spawn bash");
        let child_pid =
            child.id().expect("spawned child must have a pid") as i32;

        // 等待 bash 启动 sleep 子进程。
        tokio::time::sleep(Duration::from_millis(500)).await;

        // 杀掉整个进程组并 drain IO。
        kill_process_group(child_pid);
        let _ = drain_io(&mut child).await;

        // 给内核一点时间回收进程。
        tokio::time::sleep(Duration::from_millis(500)).await;

        // 用 pgrep 检查是否还有匹配的 sleep 进程。
        let output = std::process::Command::new("pgrep")
            .arg("-f")
            .arg(marker)
            .output()
            .expect("pgrep failed to execute");
        assert!(
            output.stdout.is_empty(),
            "orphan sleep process still running: {}",
            String::from_utf8_lossy(&output.stdout)
        );
    }

    /// 验证正常完成的命令不会触发进程组杀逻辑。
    #[tokio::test]
    async fn normal_command_completes_without_kill() {
        use std::process::Stdio as StdStdio;

        use tokio::process::Command as TokioCommand;

        let mut cmd = TokioCommand::new("bash");
        cmd.arg("-c").arg("echo hello");
        cmd.process_group(0);
        cmd.stdout(StdStdio::piped());
        cmd.stderr(StdStdio::piped());
        cmd.stdin(StdStdio::null());

        let mut child = cmd.spawn().expect("failed to spawn bash");
        let _pid = child.id();
        let status = child.wait().await.expect("wait failed");
        assert!(status.success());

        let mut buf = Vec::new();
        if let Some(mut stdout) = child.stdout.take() {
            let _ = stdout.read_to_end(&mut buf).await;
        }
        assert_eq!(String::from_utf8_lossy(&buf).trim(), "hello");
    }
}
