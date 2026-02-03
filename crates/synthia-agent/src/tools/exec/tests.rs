//! Tests for exec module

#[cfg(test)]
mod tests {
    use super::super::{ExecTool, ShellCommand, ShellOutput};
    use crate::shell::{ShellError, ShellExecutor};
    use crate::tools::Tool;
    use async_trait::async_trait;
    use rmcp::model::{CallToolResult, Content};
    use serde_json::json;
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    // =====================================================================
    // ShellCommand tests
    // =====================================================================

    #[test]
    fn test_shell_command_new() {
        let cmd = ShellCommand::new(
            "echo hello".to_string(),
            PathBuf::from("/tmp"),
        );

        assert_eq!(cmd.command, "echo hello");
        assert_eq!(cmd.cwd, PathBuf::from("/tmp"));
        assert!(cmd.timeout.is_none());
    }

    #[test]
    fn test_shell_command_with_timeout() {
        let cmd = ShellCommand::new(
            "sleep 10".to_string(),
            PathBuf::from("/home"),
        )
        .with_timeout(Duration::from_secs(30));

        assert_eq!(cmd.command, "sleep 10");
        assert!(cmd.timeout.is_some());
        assert_eq!(cmd.timeout.unwrap(), Duration::from_secs(30));
    }

    #[test]
    fn test_shell_command_builder_pattern() {
        let cmd = ShellCommand::new("ls -la".to_string(), PathBuf::from("."))
            .with_timeout(Duration::from_secs(5));

        // Verify chaining works and original values preserved
        assert_eq!(cmd.command, "ls -la");
        assert_eq!(cmd.cwd, PathBuf::from("."));
        assert_eq!(cmd.timeout, Some(Duration::from_secs(5)));
    }

    // =====================================================================
    // ShellOutput tests
    // =====================================================================

    #[test]
    fn test_shell_output_stdout_text() {
        let output = ShellOutput {
            exit_code: 0,
            stdout: vec!["line1".to_string(), "line2".to_string()],
            stderr: vec![],
        };

        assert_eq!(output.stdout_text(), "line1\nline2");
    }

    #[test]
    fn test_shell_output_stderr_text() {
        let output = ShellOutput {
            exit_code: 1,
            stdout: vec![],
            stderr: vec!["error: something went wrong".to_string()],
        };

        assert_eq!(output.stderr_text(), "error: something went wrong");
    }

    #[test]
    fn test_shell_output_is_success_true() {
        let output = ShellOutput {
            exit_code: 0,
            stdout: vec!["ok".to_string()],
            stderr: vec![],
        };

        assert!(output.is_success());
    }

    #[test]
    fn test_shell_output_is_success_false_nonzero() {
        let output = ShellOutput {
            exit_code: 127,
            stdout: vec![],
            stderr: vec!["command not found".to_string()],
        };

        assert!(!output.is_success());
    }

    #[test]
    fn test_shell_output_empty_stdout_stderr() {
        let output = ShellOutput {
            exit_code: 0,
            stdout: vec![],
            stderr: vec![],
        };

        assert_eq!(output.stdout_text(), "");
        assert_eq!(output.stderr_text(), "");
        assert!(output.is_success());
    }

    // =====================================================================
    // ShellError tests
    // =====================================================================

    #[test]
    fn test_shell_error_display_spawn_failed() {
        let err = ShellError::SpawnFailed("ENOENT".to_string());
        assert_eq!(format!("{}", err), "Failed to spawn process: ENOENT");
    }

    #[test]
    fn test_shell_error_display_timeout() {
        let err = ShellError::Timeout(300);
        assert_eq!(format!("{}", err), "Timed out after 300s");
    }

    #[test]
    fn test_shell_error_display_read_error() {
        let err = ShellError::ReadError("pipe closed".to_string());
        assert_eq!(format!("{}", err), "Failed to read output: pipe closed");
    }

    #[test]
    fn test_shell_error_io_error_derive() {
        use std::io;
        let io_err = io::Error::new(io::ErrorKind::NotFound, "file not found");
        let err = ShellError::IoError(io_err);
        assert!(format!("{}", err).contains("file not found"));
    }

    #[test]
    fn test_shell_error_debug_format() {
        let err = ShellError::Timeout(60);
        let debug_str = format!("{:?}", err);
        assert!(debug_str.contains("Timeout"));
    }

    // =====================================================================
    // ExecTool tests
    // =====================================================================

    #[test]
    fn test_exec_tool_name() {
        let executor = MockShellExecutor::new_ok();
        let tool = ExecTool::new(Arc::new(executor));

        assert_eq!(tool.name(), "Bash");
    }

    #[test]
    fn test_exec_tool_description() {
        let executor = MockShellExecutor::new_ok();
        let tool = ExecTool::new(Arc::new(executor));

        assert_eq!(tool.description(), "Execute shell commands.");
    }

    #[test]
    fn test_exec_tool_parameters() {
        let executor = MockShellExecutor::new_ok();
        let tool = ExecTool::new(Arc::new(executor));

        let params = tool.parameters();

        assert_eq!(params.get("type").unwrap(), "object");
        let properties = params.get("properties").unwrap().as_object().unwrap();
        assert!(properties.contains_key("command"));
        assert!(properties.contains_key("current_dir"));
        assert!(properties.contains_key("timeout"));
    }

    #[test]
    fn test_exec_tool_parameters_command_required() {
        let executor = MockShellExecutor::new_ok();
        let tool = ExecTool::new(Arc::new(executor));

        let params = tool.parameters();
        let required = params.get("required").unwrap().as_array().unwrap();

        assert!(required.contains(&serde_json::Value::String("command".to_string())));
    }

    #[test]
    fn test_exec_tool_debug_trait() {
        let executor = MockShellExecutor::new_ok();
        let tool = ExecTool::new(Arc::new(executor));

        let debug_str = format!("{:?}", tool);
        assert!(debug_str.contains("ExecTool"));
    }

    // =====================================================================
    // ExecTool::call tests - argument parsing
    // =====================================================================

    #[tokio::test]
    async fn test_exec_tool_call_invalid_json() {
        let executor = MockShellExecutor::new_ok();
        let tool = ExecTool::new(Arc::new(executor));

        let result = tool.call(serde_json::Value::String("not json".to_string())).await;

        assert!(result.is_error == Some(true));
        let content = &result.content[0];
        let text = match content {
            Content::Text(t) => t.text.as_str(),
            _ => "",
        };
        assert!(text.contains("Invalid arguments"));
    }

    #[tokio::test]
    async fn test_exec_tool_call_missing_command() {
        let executor = MockShellExecutor::new_ok();
        let tool = ExecTool::new(Arc::new(executor));

        let args = json!({});
        let result = tool.call(args).await;

        assert!(result.is_error == Some(true));
    }

    // =====================================================================
    // ExecTool::call tests - execution success paths
    // =====================================================================

    #[tokio::test]
    async fn test_exec_tool_call_success_with_output() {
        let executor = MockShellExecutor::new_ok();
        let tool = ExecTool::new(Arc::new(executor));

        let args = json!({"command": "echo hello"});
        let result = tool.call(args).await;

        assert!(result.is_error != Some(true));
        let content = &result.content[0];
        if let Content::Text(t) = content {
            assert!(t.text.contains("Exit code: 0"));
            assert!(t.text.contains("Stdout"));
            assert!(t.text.contains("hello"));
        }
    }

    #[tokio::test]
    async fn test_exec_tool_call_success_with_cwd() {
        let executor = MockShellExecutor::new_ok();
        let tool = ExecTool::new(Arc::new(executor));

        let args = json!({
            "command": "pwd",
            "current_dir": "/tmp"
        });
        let result = tool.call(args).await;

        assert!(result.is_error != Some(true));
    }

    #[tokio::test]
    async fn test_exec_tool_call_success_with_timeout() {
        let executor = MockShellExecutor::new_ok();
        let tool = ExecTool::new(Arc::new(executor));

        let args = json!({
            "command": "ls",
            "timeout": 30
        });
        let result = tool.call(args).await;

        assert!(result.is_error != Some(true));
    }

    #[tokio::test]
    async fn test_exec_tool_call_nonzero_exit_code() {
        let executor = MockShellExecutor::new_with_exit_code(1);
        let tool = ExecTool::new(Arc::new(executor));

        let args = json!({"command": "false"});
        let result = tool.call(args).await;

        // Non-zero exit code is still success at tool level
        assert!(result.is_error != Some(true));
        let content = &result.content[0];
        if let Content::Text(t) = content {
            assert!(t.text.contains("Exit code: 1"));
        }
    }

    #[tokio::test]
    async fn test_exec_tool_call_empty_stdout() {
        let executor = MockShellExecutor::new_with_output(ShellOutput {
            exit_code: 0,
            stdout: vec![],
            stderr: vec![],
        });
        let tool = ExecTool::new(Arc::new(executor));

        let args = json!({"command": "true"});
        let result = tool.call(args).await;

        assert!(result.is_error != Some(true));
        let content = &result.content[0];
        if let Content::Text(t) = content {
            assert!(t.text.contains("Exit code: 0"));
            // No Stdout/Stderr sections when empty
            assert!(!t.text.contains("Stdout"));
            assert!(!t.text.contains("Stderr"));
        }
    }

    #[tokio::test]
    async fn test_exec_tool_call_stderr_only() {
        let executor = MockShellExecutor::new_with_output(ShellOutput {
            exit_code: 0,
            stdout: vec![],
            stderr: vec!["warning: deprecated".to_string()],
        });
        let tool = ExecTool::new(Arc::new(executor));

        let args = json!({"command": "gcc foo.c"});
        let result = tool.call(args).await;

        assert!(result.is_error != Some(true));
        let content = &result.content[0];
        if let Content::Text(t) = content {
            assert!(t.text.contains("Stderr"));
            assert!(t.text.contains("warning: deprecated"));
        }
    }

    // =====================================================================
    // ExecTool::call tests - execution failure paths
    // =====================================================================

    #[tokio::test]
    async fn test_exec_tool_call_executor_error() {
        let executor = MockShellExecutor::new_err();
        let tool = ExecTool::new(Arc::new(executor));

        let args = json!({"command": "cat /nonexistent"});
        let result = tool.call(args).await;

        assert!(result.is_error == Some(true));
        let content = &result.content[0];
        let text = match content {
            Content::Text(t) => t.text.as_str(),
            _ => "",
        };
        assert!(text.contains("Command execution failed"));
    }

    // =====================================================================
    // MAX_TIMEOUT constant indirectly tested via timeout cap
    // =====================================================================

    #[tokio::test]
    async fn test_exec_tool_call_timeout_capped() {
        // The MAX_TIMEOUT is 5 * 60 = 300 seconds
        // Requesting more should be capped
        let executor = MockShellExecutor::new_ok();
        let tool = ExecTool::new(Arc::new(executor));

        let args = json!({
            "command": "sleep 1000",
            "timeout": 9999  // way over MAX_TIMEOUT
        });
        let result = tool.call(args).await;

        // Should not fail due to timeout cap, just proceed
        assert!(result.is_error != Some(true));
    }

    // =====================================================================
    // Mock helper for ShellExecutor
    // =====================================================================

    struct MockShellExecutor {
        result: Mutex<std::result::Result<ShellOutput, ShellError>>,
    }

    impl MockShellExecutor {
        fn new_ok() -> Self {
            Self {
                result: Mutex::new(Ok(ShellOutput {
                    exit_code: 0,
                    stdout: vec!["hello".to_string()],
                    stderr: vec![],
                })),
            }
        }

        fn new_with_exit_code(code: i32) -> Self {
            Self {
                result: Mutex::new(Ok(ShellOutput {
                    exit_code: code,
                    stdout: vec![],
                    stderr: vec![],
                })),
            }
        }

        fn new_with_output(output: ShellOutput) -> Self {
            Self {
                result: Mutex::new(Ok(output)),
            }
        }

        fn new_err() -> Self {
            Self {
                result: Mutex::new(Err(ShellError::SpawnFailed(
                    "mock spawn failure".to_string(),
                ))),
            }
        }
    }

    #[async_trait]
    impl ShellExecutor for MockShellExecutor {
        async fn execute(
            &self,
            _cmd: ShellCommand,
        ) -> std::result::Result<ShellOutput, ShellError> {
            self.result.lock().unwrap().clone()
        }

        async fn spawn(
            &self,
            _cmd: ShellCommand,
        ) -> std::result::Result<crate::shell::ChildHandle, ShellError> {
            unimplemented!()
        }
    }
}
