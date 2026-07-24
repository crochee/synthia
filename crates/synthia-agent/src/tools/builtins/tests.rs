//! Integration tests for builtin tools wired through `ToolOrchestrator`.

use std::{collections::HashMap, path::PathBuf, sync::Arc};

use async_trait::async_trait;
use synthia_permission::{HeadlessApprovalService, Permission};
use synthia_sandbox::{
    SandboxAttempt,
    SandboxError,
    SandboxManager,
    SandboxPolicy,
};
use synthia_tool_orchestrator::{
    DefaultToolOrchestrator,
    ExecutionContext,
    HashMapResolver,
    ToolCallRequest,
    ToolOrchestrator,
};
use tokio_util::sync::CancellationToken;

use super::{bash_tool, read_file, write_file};

struct NoopSandboxManager;

#[async_trait]
impl SandboxManager for NoopSandboxManager {
    async fn select(
        &self,
        _policy: SandboxPolicy,
        _tool_type: &str,
        _platform: &str,
    ) -> Result<SandboxAttempt, SandboxError> {
        Ok(SandboxAttempt::None)
    }
}

fn test_orchestrator() -> DefaultToolOrchestrator {
    let mut tools = HashMap::new();
    tools.insert("read_file".to_string(), read_file());
    tools.insert("write_file".to_string(), write_file());
    DefaultToolOrchestrator::new(
        Arc::new(HashMapResolver::new(tools)),
        Arc::new(HeadlessApprovalService),
        Arc::new(NoopSandboxManager),
        Default::default(),
    )
}

fn test_context(workspace_root: PathBuf) -> ExecutionContext {
    ExecutionContext {
        session_id: "test-session".to_string(),
        workspace_root,
        caller_agent: "test-agent".to_string(),
    }
}

#[tokio::test]
async fn read_file_and_write_file_round_trip_through_orchestrator() {
    let dir = tempfile::tempdir().unwrap();
    let workspace = dir.path().to_path_buf();
    let orchestrator = test_orchestrator();

    let write_request = ToolCallRequest {
        call_id: "write-1".to_string(),
        tool_name: "write_file".to_string(),
        arguments: serde_json::json!({
            "path": "hello.txt",
            "content": "Hello, Synthia!"
        }),
        permission: Permission::AutoApprove,
    };
    let write_result = orchestrator
        .execute(
            write_request,
            test_context(workspace.clone()),
            CancellationToken::new(),
        )
        .await
        .expect("write should succeed");
    assert!(!write_result.is_error);

    let read_request = ToolCallRequest {
        call_id: "read-1".to_string(),
        tool_name: "read_file".to_string(),
        arguments: serde_json::json!({
            "file_path": workspace.join("hello.txt").to_str().unwrap()
        }),
        permission: Permission::AutoApprove,
    };
    let read_result = orchestrator
        .execute(
            read_request,
            test_context(workspace),
            CancellationToken::new(),
        )
        .await
        .expect("read should succeed");
    assert!(!read_result.is_error);

    let text = read_result
        .outcome
        .as_array()
        .and_then(|parts| {
            parts
                .iter()
                .find_map(|p| p.get("text").and_then(|t| t.as_str()))
        })
        .expect("read outcome contains text");
    assert!(text.contains("Hello, Synthia!"));
}

#[cfg(unix)]
fn bash_test_orchestrator(workspace_root: PathBuf) -> DefaultToolOrchestrator {
    let mut tools = HashMap::new();
    tools.insert(
        "bash".to_string(),
        bash_tool(workspace_root.clone()).expect("bash tool should build"),
    );
    DefaultToolOrchestrator::new(
        Arc::new(HashMapResolver::new(tools)),
        Arc::new(HeadlessApprovalService),
        Arc::new(
            synthia_sandbox::backends::bubblewrap::BubblewrapBackend::new(
                workspace_root,
            ),
        ),
        Default::default(),
    )
}

#[cfg(unix)]
fn bash_test_request(call_id: &str, command: &str) -> ToolCallRequest {
    ToolCallRequest {
        call_id: call_id.to_string(),
        tool_name: "bash".to_string(),
        arguments: serde_json::json!({ "command": command }),
        permission: Permission::AutoApprove,
    }
}

#[tokio::test]
#[cfg(unix)]
async fn sandboxed_bash_echo_hello() {
    let dir = tempfile::tempdir().unwrap();
    let workspace = dir.path().to_path_buf();

    let bwrap_available = tokio::process::Command::new("bwrap")
        .arg("--version")
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !bwrap_available {
        return;
    }

    let orchestrator = bash_test_orchestrator(workspace);
    let result = orchestrator
        .execute(
            bash_test_request("bash-echo-1", "echo hello"),
            test_context(dir.path().to_path_buf()),
            CancellationToken::new(),
        )
        .await
        .expect("bash should execute inside sandbox");

    assert!(!result.is_error);
    let text = result.outcome.as_str().expect("outcome should be a string");
    assert!(
        text.contains("hello"),
        "expected 'hello' in output: {}",
        text
    );
}

#[tokio::test]
#[cfg(unix)]
async fn sandboxed_bash_cannot_read_etc_passwd() {
    let dir = tempfile::tempdir().unwrap();
    let workspace = dir.path().to_path_buf();

    let bwrap_available = tokio::process::Command::new("bwrap")
        .arg("--version")
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !bwrap_available {
        return;
    }

    let orchestrator = bash_test_orchestrator(workspace);
    let result = orchestrator
        .execute(
            bash_test_request("bash-etc-1", "cat /etc/passwd"),
            test_context(dir.path().to_path_buf()),
            CancellationToken::new(),
        )
        .await
        .expect("bash should execute inside sandbox");

    let text = result.outcome.as_str().expect("outcome should be a string");
    assert!(
        result.is_error
            || text.is_empty()
            || text.contains("No such file")
            || text.contains("Exit code:"),
        "expected sandbox to block /etc/passwd, got: {}",
        text
    );
}
