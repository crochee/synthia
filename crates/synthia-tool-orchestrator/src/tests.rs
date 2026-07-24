use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        Arc,
        Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use async_trait::async_trait;
use synthia_permission::{
    ApprovalError,
    ApprovalOutcome,
    ApprovalPolicy,
    ApprovalService,
    Permission,
    PermissionFuture,
};
use synthia_sandbox::{
    SandboxAttempt,
    SandboxError,
    SandboxManager,
    SandboxPolicy,
};
use tokio_util::sync::CancellationToken;

use crate::{
    DefaultToolOrchestrator,
    ExecutableTool,
    ExecutionContext,
    HashMapResolver,
    RetryPolicy,
    ToolCallRequest,
    ToolCallResult,
    ToolExecutionError,
    ToolOrchestrator,
    ToolOrchestratorEvent,
};

/// Mock tool that emits [`synthia_tool::FileChangeEvent`]s when executed
/// with the event callback.
struct FileChangeEmittingTool;

#[async_trait]
impl ExecutableTool for FileChangeEmittingTool {
    fn name(&self) -> &str {
        "file_change_emitter"
    }

    fn is_concurrency_safe(&self) -> bool {
        true
    }

    async fn execute(
        &self,
        request: &ToolCallRequest,
        _context: &ExecutionContext,
        _sandbox_attempt: &SandboxAttempt,
        _cancellation_token: CancellationToken,
    ) -> Result<ToolCallResult, ToolExecutionError> {
        Ok(ToolCallResult {
            call_id: request.call_id.clone(),
            tool_name: request.tool_name.clone(),
            outcome: serde_json::json!("ok"),
            is_error: false,
        })
    }

    async fn execute_with_events(
        &self,
        request: &ToolCallRequest,
        _context: &ExecutionContext,
        _sandbox_attempt: &SandboxAttempt,
        _cancellation_token: CancellationToken,
        on_event: Option<
            Box<dyn Fn(synthia_tool::FileChangeEvent) + Send + Sync>,
        >,
    ) -> Result<ToolCallResult, ToolExecutionError> {
        if let Some(emit) = on_event {
            emit(synthia_tool::FileChangeEvent::HunkApplied {
                path: "/tmp/f.txt".to_string(),
                hunk_index: 0,
            });
            emit(synthia_tool::FileChangeEvent::FileUpdated {
                path: "/tmp/f.txt".to_string(),
            });
        }
        Ok(ToolCallResult {
            call_id: request.call_id.clone(),
            tool_name: request.tool_name.clone(),
            outcome: serde_json::json!("ok"),
            is_error: false,
        })
    }
}

/// Mock approval service that unconditionally approves every request.
struct MockApprovingService;

#[async_trait]
impl ApprovalService for MockApprovingService {
    async fn request_approval(
        &self,
        _tool: &str,
        _args: &serde_json::Value,
        _policy: ApprovalPolicy,
        _timeout: Duration,
        _cancel: CancellationToken,
    ) -> Result<ApprovalOutcome, ApprovalError> {
        Ok(ApprovalOutcome::Approve)
    }
    fn ask(
        &self,
        request: synthia_permission::PermissionRequest,
    ) -> PermissionFuture {
        PermissionFuture::immediate_granted(
            request.outcome(Permission::AutoApprove),
        )
    }
}

/// Mock approval service that unconditionally denies every request.
struct MockDenyingService;

#[async_trait]
impl ApprovalService for MockDenyingService {
    async fn request_approval(
        &self,
        _tool: &str,
        _args: &serde_json::Value,
        _policy: ApprovalPolicy,
        _timeout: Duration,
        _cancel: CancellationToken,
    ) -> Result<ApprovalOutcome, ApprovalError> {
        Ok(ApprovalOutcome::Deny)
    }

    fn ask(
        &self,
        _request: synthia_permission::PermissionRequest,
    ) -> PermissionFuture {
        PermissionFuture::immediate_denied()
    }
}

/// Mock approval service that returns a fixed error.
struct MockErrorApprovalService(ApprovalError);

#[async_trait]
impl ApprovalService for MockErrorApprovalService {
    async fn request_approval(
        &self,
        _tool: &str,
        _args: &serde_json::Value,
        _policy: ApprovalPolicy,
        _timeout: Duration,
        _cancel: CancellationToken,
    ) -> Result<ApprovalOutcome, ApprovalError> {
        Err(self.0.clone())
    }

    fn ask(
        &self,
        _request: synthia_permission::PermissionRequest,
    ) -> PermissionFuture {
        PermissionFuture::immediate_denied()
    }
}

/// Mock sandbox manager that returns a fixed [`SandboxAttempt`].
struct MockSandboxManager(SandboxAttempt);

#[async_trait]
impl SandboxManager for MockSandboxManager {
    async fn select(
        &self,
        _policy: SandboxPolicy,
        _tool_type: &str,
        _platform: &str,
    ) -> Result<SandboxAttempt, SandboxError> {
        Ok(self.0.clone())
    }
}

/// Mock tool that echoes its arguments back as the result outcome.
struct EchoTool;

#[async_trait]
impl ExecutableTool for EchoTool {
    fn name(&self) -> &str {
        "echo"
    }

    fn is_concurrency_safe(&self) -> bool {
        true
    }

    async fn execute(
        &self,
        request: &ToolCallRequest,
        _context: &ExecutionContext,
        _sandbox_attempt: &SandboxAttempt,
        _cancellation_token: CancellationToken,
    ) -> Result<ToolCallResult, ToolExecutionError> {
        Ok(ToolCallResult {
            call_id: request.call_id.clone(),
            tool_name: request.tool_name.clone(),
            outcome: request.arguments.clone(),
            is_error: false,
        })
    }
}

/// Mock tool that records the [`SandboxAttempt`] it receives.
struct SandboxRecordingTool {
    received: Arc<Mutex<Option<SandboxAttempt>>>,
}

#[async_trait]
impl ExecutableTool for SandboxRecordingTool {
    fn name(&self) -> &str {
        "sandbox_recorder"
    }

    async fn execute(
        &self,
        request: &ToolCallRequest,
        _context: &ExecutionContext,
        sandbox_attempt: &SandboxAttempt,
        _cancellation_token: CancellationToken,
    ) -> Result<ToolCallResult, ToolExecutionError> {
        {
            *self.received.lock().unwrap() = Some(sandbox_attempt.clone());
        }
        Ok(ToolCallResult {
            call_id: request.call_id.clone(),
            tool_name: request.tool_name.clone(),
            outcome: serde_json::json!("ok"),
            is_error: false,
        })
    }
}

/// Mock tool that fails a configurable number of times with transient errors
/// before succeeding.
struct FlakyTool {
    attempts: Arc<AtomicUsize>,
    fail_count: usize,
}

#[async_trait]
impl ExecutableTool for FlakyTool {
    fn name(&self) -> &str {
        "flaky"
    }

    async fn execute(
        &self,
        request: &ToolCallRequest,
        _context: &ExecutionContext,
        _sandbox_attempt: &SandboxAttempt,
        _cancellation_token: CancellationToken,
    ) -> Result<ToolCallResult, ToolExecutionError> {
        let count = self.attempts.fetch_add(1, Ordering::SeqCst);
        if count < self.fail_count {
            return Err(ToolExecutionError::Transient(format!(
                "attempt {} failed",
                count + 1
            )));
        }
        Ok(ToolCallResult {
            call_id: request.call_id.clone(),
            tool_name: request.tool_name.clone(),
            outcome: serde_json::json!("success"),
            is_error: false,
        })
    }
}

fn test_context() -> ExecutionContext {
    ExecutionContext {
        session_id: "test-session".to_string(),
        workspace_root: PathBuf::from("/tmp"),
        caller_agent: "test-agent".to_string(),
    }
}

fn test_request(call_id: &str, tool_name: &str) -> ToolCallRequest {
    ToolCallRequest {
        call_id: call_id.to_string(),
        tool_name: tool_name.to_string(),
        arguments: serde_json::json!({}),
        permission: Permission::RequireConfirm,
    }
}

#[tokio::test]
async fn mock_approval_service_approve_allows_execution() {
    let mut tools = HashMap::new();
    tools.insert(
        "echo".to_string(),
        Arc::new(EchoTool) as Arc<dyn ExecutableTool>,
    );
    let orchestrator = DefaultToolOrchestrator::new(
        Arc::new(HashMapResolver::new(tools)),
        Arc::new(MockApprovingService),
        Arc::new(MockSandboxManager(SandboxAttempt::None)),
        RetryPolicy::default(),
    );

    let result = orchestrator
        .execute(
            test_request("approve-call", "echo"),
            test_context(),
            CancellationToken::new(),
        )
        .await;

    assert!(result.is_ok());
    assert_eq!(result.unwrap().call_id, "approve-call");
}

#[tokio::test]
async fn mock_approval_service_deny_blocks_execution() {
    let mut tools = HashMap::new();
    tools.insert(
        "echo".to_string(),
        Arc::new(EchoTool) as Arc<dyn ExecutableTool>,
    );
    let orchestrator = DefaultToolOrchestrator::new(
        Arc::new(HashMapResolver::new(tools)),
        Arc::new(MockDenyingService),
        Arc::new(MockSandboxManager(SandboxAttempt::None)),
        RetryPolicy::default(),
    );

    let result = orchestrator
        .execute(
            test_request("deny-call", "echo"),
            test_context(),
            CancellationToken::new(),
        )
        .await;

    assert!(matches!(
        result,
        Err(crate::ToolOrchestratorError::Denied { .. })
    ));
}

fn test_request_with_permission(
    call_id: &str,
    tool_name: &str,
    permission: Permission,
) -> ToolCallRequest {
    ToolCallRequest {
        call_id: call_id.to_string(),
        tool_name: tool_name.to_string(),
        arguments: serde_json::json!({}),
        permission,
    }
}

#[tokio::test]
async fn require_confirm_routes_through_approval_service() {
    let mut tools = HashMap::new();
    tools.insert(
        "echo".to_string(),
        Arc::new(EchoTool) as Arc<dyn ExecutableTool>,
    );
    let orchestrator = DefaultToolOrchestrator::new(
        Arc::new(HashMapResolver::new(tools)),
        Arc::new(MockDenyingService),
        Arc::new(MockSandboxManager(SandboxAttempt::None)),
        RetryPolicy::default(),
    );

    let result = orchestrator
        .execute(
            test_request_with_permission(
                "require-confirm-call",
                "echo",
                Permission::RequireConfirm,
            ),
            test_context(),
            CancellationToken::new(),
        )
        .await;

    assert!(matches!(
        result,
        Err(crate::ToolOrchestratorError::Denied { .. })
    ));
}

#[tokio::test]
async fn require_explicit_routes_through_approval_service() {
    let mut tools = HashMap::new();
    tools.insert(
        "echo".to_string(),
        Arc::new(EchoTool) as Arc<dyn ExecutableTool>,
    );
    let orchestrator = DefaultToolOrchestrator::new(
        Arc::new(HashMapResolver::new(tools)),
        Arc::new(MockDenyingService),
        Arc::new(MockSandboxManager(SandboxAttempt::None)),
        RetryPolicy::default(),
    );

    let result = orchestrator
        .execute(
            test_request_with_permission(
                "require-explicit-call",
                "echo",
                Permission::RequireExplicit,
            ),
            test_context(),
            CancellationToken::new(),
        )
        .await;

    assert!(matches!(
        result,
        Err(crate::ToolOrchestratorError::Denied { .. })
    ));
}

#[tokio::test]
async fn approval_service_unavailable_maps_to_deny() {
    let mut tools = HashMap::new();
    tools.insert(
        "echo".to_string(),
        Arc::new(EchoTool) as Arc<dyn ExecutableTool>,
    );
    let orchestrator = DefaultToolOrchestrator::new(
        Arc::new(HashMapResolver::new(tools)),
        Arc::new(MockErrorApprovalService(ApprovalError::Unavailable)),
        Arc::new(MockSandboxManager(SandboxAttempt::None)),
        RetryPolicy::default(),
    );

    let result = orchestrator
        .execute(
            test_request("unavailable-call", "echo"),
            test_context(),
            CancellationToken::new(),
        )
        .await;

    assert!(matches!(
        result,
        Err(crate::ToolOrchestratorError::Denied { .. })
    ));
}

#[tokio::test]
async fn approval_service_cancelled_maps_to_deny() {
    let mut tools = HashMap::new();
    tools.insert(
        "echo".to_string(),
        Arc::new(EchoTool) as Arc<dyn ExecutableTool>,
    );
    let orchestrator = DefaultToolOrchestrator::new(
        Arc::new(HashMapResolver::new(tools)),
        Arc::new(MockErrorApprovalService(ApprovalError::Cancelled)),
        Arc::new(MockSandboxManager(SandboxAttempt::None)),
        RetryPolicy::default(),
    );

    let result = orchestrator
        .execute(
            test_request("cancelled-call", "echo"),
            test_context(),
            CancellationToken::new(),
        )
        .await;

    assert!(matches!(
        result,
        Err(crate::ToolOrchestratorError::Denied { .. })
    ));
}

#[tokio::test]
async fn mock_sandbox_attempt_is_propagated_to_tool() {
    let received = Arc::new(Mutex::new(None));
    let tool = Arc::new(SandboxRecordingTool {
        received: received.clone(),
    });
    let mut tools = HashMap::new();
    tools.insert(tool.name().to_string(), tool as Arc<dyn ExecutableTool>);

    let expected = SandboxAttempt::Bubblewrap {
        workspace: PathBuf::from("/tmp/workspace"),
        args: vec!["--die-with-parent".to_string()],
    };
    let orchestrator = DefaultToolOrchestrator::new(
        Arc::new(HashMapResolver::new(tools)),
        Arc::new(MockApprovingService),
        Arc::new(MockSandboxManager(expected.clone())),
        RetryPolicy::default(),
    );

    let result = orchestrator
        .execute(
            test_request("sandbox-call", "sandbox_recorder"),
            test_context(),
            CancellationToken::new(),
        )
        .await;

    assert!(result.is_ok());
    let guard = received.lock().unwrap();
    let received = guard.as_ref().unwrap();
    assert!(matches!(
        received,
        SandboxAttempt::Bubblewrap { workspace, args }
        if workspace.as_path() == std::path::Path::new("/tmp/workspace")
            && args == &["--die-with-parent".to_string()]
    ));
}

#[tokio::test]
async fn batch_execution_with_mock_services() {
    let mut tools = HashMap::new();
    tools.insert(
        "echo".to_string(),
        Arc::new(EchoTool) as Arc<dyn ExecutableTool>,
    );
    let orchestrator = DefaultToolOrchestrator::new(
        Arc::new(HashMapResolver::new(tools)),
        Arc::new(MockApprovingService),
        Arc::new(MockSandboxManager(SandboxAttempt::None)),
        RetryPolicy::default(),
    );

    let requests = vec![
        {
            let mut r = test_request("batch-a", "echo");
            r.permission = Permission::AutoApprove;
            r.arguments = serde_json::json!({"key": "a"});
            r
        },
        {
            let mut r = test_request("batch-b", "echo");
            r.permission = Permission::AutoApprove;
            r.arguments = serde_json::json!({"key": "b"});
            r
        },
    ];

    let results = orchestrator
        .execute_batch(requests, test_context(), CancellationToken::new())
        .await
        .unwrap();

    assert_eq!(results.len(), 2);
    let values: Vec<_> = results
        .into_iter()
        .map(|r| r.outcome.get("key").unwrap().as_str().unwrap().to_string())
        .collect();
    assert!(values.contains(&"a".to_string()));
    assert!(values.contains(&"b".to_string()));
}

#[tokio::test]
async fn retry_succeeds_after_transient_failures() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let tool = Arc::new(FlakyTool {
        attempts: attempts.clone(),
        fail_count: 2,
    });
    let mut tools = HashMap::new();
    tools.insert(tool.name().to_string(), tool as Arc<dyn ExecutableTool>);

    let orchestrator = DefaultToolOrchestrator::new(
        Arc::new(HashMapResolver::new(tools)),
        Arc::new(MockApprovingService),
        Arc::new(MockSandboxManager(SandboxAttempt::None)),
        RetryPolicy {
            max_attempts: 3,
            base_delay_ms: 1,
        },
    );

    let mut request = test_request("retry-call", "flaky");
    request.permission = Permission::AutoApprove;
    let result = orchestrator
        .execute(request, test_context(), CancellationToken::new())
        .await;

    assert!(result.is_ok());
    assert_eq!(attempts.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn sandbox_unavailable_with_deny_policy_fails_closed() {
    let mut tools = HashMap::new();
    tools.insert(
        "echo".to_string(),
        Arc::new(EchoTool) as Arc<dyn ExecutableTool>,
    );
    let orchestrator = DefaultToolOrchestrator::with_sandbox_policy(
        Arc::new(HashMapResolver::new(tools)),
        Arc::new(MockApprovingService),
        Arc::new(MockSandboxManager(SandboxAttempt::Unavailable)),
        RetryPolicy::default(),
        Default::default(),
        SandboxPolicy::Standard,
    );
    let mut request = test_request("deny-unavailable", "echo");
    request.permission = Permission::AutoApprove;

    let result = orchestrator
        .execute(request, test_context(), CancellationToken::new())
        .await;

    assert!(matches!(
        result,
        Err(crate::ToolOrchestratorError::Sandbox { .. })
    ));
}

#[tokio::test]
async fn sandbox_unavailable_with_prompt_policy_continues_unsandboxed() {
    let mut tools = HashMap::new();
    tools.insert(
        "echo".to_string(),
        Arc::new(EchoTool) as Arc<dyn ExecutableTool>,
    );
    let orchestrator = DefaultToolOrchestrator::with_sandbox_policy(
        Arc::new(HashMapResolver::new(tools)),
        Arc::new(MockApprovingService),
        Arc::new(MockSandboxManager(SandboxAttempt::Unavailable)),
        RetryPolicy::default(),
        Default::default(),
        SandboxPolicy::None,
    );
    let mut request = test_request("prompt-unavailable", "echo");
    request.permission = Permission::AutoApprove;

    let result = orchestrator
        .execute(request, test_context(), CancellationToken::new())
        .await;

    assert!(result.is_ok());
    assert_eq!(result.unwrap().call_id, "prompt-unavailable");
}

#[tokio::test]
async fn events_start_and_complete_are_emitted() {
    let mut tools = HashMap::new();
    tools.insert(
        "echo".to_string(),
        Arc::new(EchoTool) as Arc<dyn ExecutableTool>,
    );
    let orchestrator = DefaultToolOrchestrator::new(
        Arc::new(HashMapResolver::new(tools)),
        Arc::new(MockApprovingService),
        Arc::new(MockSandboxManager(SandboxAttempt::None)),
        RetryPolicy::default(),
    );
    let mut rx = orchestrator.event_stream();

    let mut request = test_request("event-call", "echo");
    request.permission = Permission::AutoApprove;
    orchestrator
        .execute(request, test_context(), CancellationToken::new())
        .await
        .unwrap();

    let mut started = false;
    let mut completed = false;
    while let Ok(event) = rx.try_recv() {
        match event {
            ToolOrchestratorEvent::Started { call_id, .. }
                if call_id == "event-call" =>
            {
                started = true;
            }
            ToolOrchestratorEvent::Completed { call_id, .. }
                if call_id == "event-call" =>
            {
                completed = true;
            }
            _ => {}
        }
    }

    assert!(started);
    assert!(completed);
}

#[tokio::test]
async fn file_change_events_are_forwarded_to_event_stream() {
    let mut tools = HashMap::new();
    tools.insert(
        "file_change_emitter".to_string(),
        Arc::new(FileChangeEmittingTool) as Arc<dyn ExecutableTool>,
    );
    let orchestrator = DefaultToolOrchestrator::new(
        Arc::new(HashMapResolver::new(tools)),
        Arc::new(MockApprovingService),
        Arc::new(MockSandboxManager(SandboxAttempt::None)),
        RetryPolicy::default(),
    );
    let mut rx = orchestrator.event_stream();

    let mut request = test_request("file-change-call", "file_change_emitter");
    request.permission = Permission::AutoApprove;
    orchestrator
        .execute(request, test_context(), CancellationToken::new())
        .await
        .unwrap();

    let mut saw_hunk = false;
    let mut saw_updated = false;
    while let Ok(event) = rx.try_recv() {
        match event {
            ToolOrchestratorEvent::FileChange {
                call_id,
                tool_name,
                event:
                    synthia_tool::FileChangeEvent::HunkApplied { path, hunk_index },
            } if call_id == "file-change-call"
                && tool_name == "file_change_emitter"
                && path == "/tmp/f.txt"
                && hunk_index == 0 =>
            {
                saw_hunk = true;
            }
            ToolOrchestratorEvent::FileChange {
                call_id,
                tool_name,
                event: synthia_tool::FileChangeEvent::FileUpdated { path },
            } if call_id == "file-change-call"
                && tool_name == "file_change_emitter"
                && path == "/tmp/f.txt" =>
            {
                saw_updated = true;
            }
            _ => {}
        }
    }

    assert!(saw_hunk, "expected HunkApplied event");
    assert!(saw_updated, "expected FileUpdated event");
}
