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
use synthia_core::tool::capability::CapabilityBroker;
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
    HashMapToolIdResolver,
    RetryPolicy,
    ToolCallRequest,
    ToolCallResult,
    ToolExecutionError,
    ToolIdResolver,
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
            tool_id: request.tool_id,
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
            tool_id: request.tool_id,
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
            tool_id: request.tool_id,
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
            tool_id: request.tool_id,
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
            tool_id: request.tool_id,
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
        tool_id: None,
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
        tool_id: None,
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

/// Mock tool that represents a bash-like tool (requires `command_invoke`).
struct BashLikeTool;

#[async_trait]
impl ExecutableTool for BashLikeTool {
    fn name(&self) -> &str {
        "bash"
    }

    fn is_concurrency_safe(&self) -> bool {
        false
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
            tool_id: request.tool_id,
        })
    }
}

#[tokio::test]
async fn capability_broker_denies_command_invoke() {
    let mut tools = HashMap::new();
    tools.insert(
        "bash".to_string(),
        Arc::new(BashLikeTool) as Arc<dyn ExecutableTool>,
    );
    let broker = Arc::new(CapabilityBroker::new(
        synthia_core::tool::capability::ToolCapabilities::default(),
    ));
    let orchestrator = DefaultToolOrchestrator::new(
        Arc::new(HashMapResolver::new(tools)),
        Arc::new(MockApprovingService),
        Arc::new(MockSandboxManager(SandboxAttempt::None)),
        RetryPolicy::default(),
    )
    .with_capability_broker(broker);

    let mut request = test_request("cap-deny-call", "bash");
    request.permission = Permission::AutoApprove;
    let result = orchestrator
        .execute(request, test_context(), CancellationToken::new())
        .await;

    assert!(
        matches!(result, Err(crate::ToolOrchestratorError::Denied { .. })),
        "expected Denied when command_invoke is not allowed"
    );
}

#[tokio::test]
async fn capability_broker_allows_when_capability_granted() {
    let mut tools = HashMap::new();
    tools.insert(
        "bash".to_string(),
        Arc::new(BashLikeTool) as Arc<dyn ExecutableTool>,
    );
    let caps = synthia_core::tool::capability::ToolCapabilities {
        command_invoke: true,
        ..Default::default()
    };
    let broker = Arc::new(CapabilityBroker::new(caps));
    let orchestrator = DefaultToolOrchestrator::new(
        Arc::new(HashMapResolver::new(tools)),
        Arc::new(MockApprovingService),
        Arc::new(MockSandboxManager(SandboxAttempt::None)),
        RetryPolicy::default(),
    )
    .with_capability_broker(broker);

    let mut request = test_request("cap-allow-call", "bash");
    request.permission = Permission::AutoApprove;
    let result = orchestrator
        .execute(request, test_context(), CancellationToken::new())
        .await;

    assert!(
        result.is_ok(),
        "expected success when command_invoke is allowed"
    );
}

#[tokio::test]
async fn capability_broker_allows_unknown_tool() {
    let mut tools = HashMap::new();
    tools.insert(
        "echo".to_string(),
        Arc::new(EchoTool) as Arc<dyn ExecutableTool>,
    );
    // "echo" has no capability mapping, so broker should not block it.
    let broker = Arc::new(CapabilityBroker::new(
        synthia_core::tool::capability::ToolCapabilities::default(),
    ));
    let orchestrator = DefaultToolOrchestrator::new(
        Arc::new(HashMapResolver::new(tools)),
        Arc::new(MockApprovingService),
        Arc::new(MockSandboxManager(SandboxAttempt::None)),
        RetryPolicy::default(),
    )
    .with_capability_broker(broker);

    let mut request = test_request("cap-unknown-call", "echo");
    request.permission = Permission::AutoApprove;
    let result = orchestrator
        .execute(request, test_context(), CancellationToken::new())
        .await;

    assert!(
        result.is_ok(),
        "tools without a capability mapping should pass through the broker"
    );
}

#[tokio::test]
async fn capability_broker_none_allows_all() {
    let mut tools = HashMap::new();
    tools.insert(
        "bash".to_string(),
        Arc::new(BashLikeTool) as Arc<dyn ExecutableTool>,
    );
    // No capability_broker set — all tools should be allowed.
    let orchestrator = DefaultToolOrchestrator::new(
        Arc::new(HashMapResolver::new(tools)),
        Arc::new(MockApprovingService),
        Arc::new(MockSandboxManager(SandboxAttempt::None)),
        RetryPolicy::default(),
    );

    let mut request = test_request("no-broker-call", "bash");
    request.permission = Permission::AutoApprove;
    let result = orchestrator
        .execute(request, test_context(), CancellationToken::new())
        .await;

    assert!(
        result.is_ok(),
        "without capability_broker, all tools should be allowed"
    );
}

mod provenance_floor_tests {
    use synthia_permission::Permission;
    use synthia_tool_materialization::ToolProvenance;

    use crate::{apply_provenance_floor, permission_is_more_restrictive};

    #[test]
    fn builtin_floor_is_auto_approve() {
        let result = apply_provenance_floor(
            &ToolProvenance::Builtin,
            Permission::AutoApprove,
        );
        assert_eq!(result, Permission::AutoApprove);
    }

    #[test]
    fn plugin_floor_is_require_confirm() {
        let result = apply_provenance_floor(
            &ToolProvenance::Plugin {
                extension_id: "ext-1".into(),
            },
            Permission::AutoApprove,
        );
        assert_eq!(result, Permission::RequireConfirm);
    }

    #[test]
    fn ephemeral_floor_is_require_explicit() {
        let result = apply_provenance_floor(
            &ToolProvenance::Ephemeral {
                source_id: "src-1".into(),
            },
            Permission::AutoApprove,
        );
        assert_eq!(result, Permission::RequireExplicit);
    }

    #[test]
    fn policy_block_overrides_builtin_floor() {
        // Block is more restrictive than AutoApprove floor → policy wins.
        let result =
            apply_provenance_floor(&ToolProvenance::Builtin, Permission::Block);
        assert_eq!(result, Permission::Block);
    }

    #[test]
    fn builtin_auto_approve_floor_overrides_policy_require_confirm() {
        // AutoApprove floor is more restrictive than RequireConfirm? No.
        // RequireConfirm is MORE restrictive than AutoApprove.
        // So policy RequireConfirm is more restrictive than floor AutoApprove
        // → policy wins (RequireConfirm).
        let result = apply_provenance_floor(
            &ToolProvenance::Builtin,
            Permission::RequireConfirm,
        );
        assert_eq!(result, Permission::RequireConfirm);
    }

    #[test]
    fn plugin_floor_overrides_policy_auto_approve() {
        // Plugin floor is RequireConfirm; policy is AutoApprove.
        // RequireConfirm is more restrictive than AutoApprove,
        // so floor wins.
        let result = apply_provenance_floor(
            &ToolProvenance::Plugin {
                extension_id: "ext".into(),
            },
            Permission::AutoApprove,
        );
        assert_eq!(result, Permission::RequireConfirm);
    }

    #[test]
    fn policy_block_overrides_plugin_floor() {
        // Block is more restrictive than RequireConfirm → policy wins.
        let result = apply_provenance_floor(
            &ToolProvenance::Plugin {
                extension_id: "ext".into(),
            },
            Permission::Block,
        );
        assert_eq!(result, Permission::Block);
    }

    #[test]
    fn ephemeral_floor_overrides_policy_auto_approve() {
        let result = apply_provenance_floor(
            &ToolProvenance::Ephemeral {
                source_id: "src".into(),
            },
            Permission::AutoApprove,
        );
        assert_eq!(result, Permission::RequireExplicit);
    }

    #[test]
    fn ephemeral_floor_overrides_policy_require_confirm() {
        // RequireExplicit floor is more restrictive than RequireConfirm
        // → floor wins.
        let result = apply_provenance_floor(
            &ToolProvenance::Ephemeral {
                source_id: "src".into(),
            },
            Permission::RequireConfirm,
        );
        assert_eq!(result, Permission::RequireExplicit);
    }

    #[test]
    fn policy_deny_overrides_builtin_floor() {
        let result = apply_provenance_floor(
            &ToolProvenance::Builtin,
            Permission::Deny {
                reason: "policy".into(),
            },
        );
        assert!(matches!(result, Permission::Deny { .. }));
    }

    #[test]
    fn more_restrictive_ordering() {
        assert!(permission_is_more_restrictive(
            &Permission::Block,
            &Permission::AutoApprove,
        ));
        assert!(permission_is_more_restrictive(
            &Permission::Deny {
                reason: String::new(),
            },
            &Permission::RequireExplicit,
        ));
        assert!(permission_is_more_restrictive(
            &Permission::RequireExplicit,
            &Permission::RequireConfirm,
        ));
        assert!(permission_is_more_restrictive(
            &Permission::RequireConfirm,
            &Permission::AutoApprove,
        ));
        // Same level is not more restrictive.
        assert!(!permission_is_more_restrictive(
            &Permission::AutoApprove,
            &Permission::AutoApprove,
        ));
    }
}

mod tool_id_resolver_tests {
    use std::collections::HashMap;

    use synthia_tool_materialization::ToolId;

    use super::*;

    #[tokio::test]
    async fn tool_id_populated_when_resolver_has_materialization() {
        let mut tools = HashMap::new();
        tools.insert(
            "echo".to_string(),
            Arc::new(EchoTool) as Arc<dyn ExecutableTool>,
        );
        let tool_id = ToolId::new();
        let mut id_map = HashMap::new();
        id_map.insert("echo".to_string(), tool_id);
        let orchestrator = DefaultToolOrchestrator::new(
            Arc::new(HashMapResolver::new(tools)),
            Arc::new(MockApprovingService),
            Arc::new(MockSandboxManager(SandboxAttempt::None)),
            RetryPolicy::default(),
        )
        .with_tool_id_resolver(Arc::new(HashMapToolIdResolver::new(id_map)));

        let mut request = test_request("tid-1", "echo");
        request.permission = Permission::AutoApprove;
        let result = orchestrator
            .execute(request, test_context(), CancellationToken::new())
            .await
            .expect("execute should succeed");

        assert_eq!(result.tool_id, Some(tool_id));
    }

    #[tokio::test]
    async fn tool_id_none_when_no_materialization() {
        let mut tools = HashMap::new();
        tools.insert(
            "echo".to_string(),
            Arc::new(EchoTool) as Arc<dyn ExecutableTool>,
        );
        // No ToolIdResolver configured
        let orchestrator = DefaultToolOrchestrator::new(
            Arc::new(HashMapResolver::new(tools)),
            Arc::new(MockApprovingService),
            Arc::new(MockSandboxManager(SandboxAttempt::None)),
            RetryPolicy::default(),
        );

        let mut request = test_request("tid-2", "echo");
        request.permission = Permission::AutoApprove;
        let result = orchestrator
            .execute(request, test_context(), CancellationToken::new())
            .await
            .expect("execute should succeed");

        assert!(result.tool_id.is_none());
    }

    #[tokio::test]
    async fn tool_id_none_when_resolver_has_no_entry_for_tool() {
        let mut tools = HashMap::new();
        tools.insert(
            "echo".to_string(),
            Arc::new(EchoTool) as Arc<dyn ExecutableTool>,
        );
        // Resolver exists but has no entry for "echo"
        let orchestrator = DefaultToolOrchestrator::new(
            Arc::new(HashMapResolver::new(tools)),
            Arc::new(MockApprovingService),
            Arc::new(MockSandboxManager(SandboxAttempt::None)),
            RetryPolicy::default(),
        )
        .with_tool_id_resolver(Arc::new(HashMapToolIdResolver::new(
            HashMap::new(),
        )));

        let mut request = test_request("tid-3", "echo");
        request.permission = Permission::AutoApprove;
        let result = orchestrator
            .execute(request, test_context(), CancellationToken::new())
            .await
            .expect("execute should succeed");

        assert!(result.tool_id.is_none());
    }

    #[tokio::test]
    async fn completed_event_carries_tool_id() {
        let mut tools = HashMap::new();
        tools.insert(
            "echo".to_string(),
            Arc::new(EchoTool) as Arc<dyn ExecutableTool>,
        );
        let tool_id = ToolId::new();
        let mut id_map = HashMap::new();
        id_map.insert("echo".to_string(), tool_id);
        let orchestrator = DefaultToolOrchestrator::new(
            Arc::new(HashMapResolver::new(tools)),
            Arc::new(MockApprovingService),
            Arc::new(MockSandboxManager(SandboxAttempt::None)),
            RetryPolicy::default(),
        )
        .with_tool_id_resolver(Arc::new(HashMapToolIdResolver::new(id_map)));
        let mut rx = orchestrator.event_stream();

        let mut request = test_request("tid-event-1", "echo");
        request.permission = Permission::AutoApprove;
        orchestrator
            .execute(request, test_context(), CancellationToken::new())
            .await
            .expect("execute should succeed");

        let mut found_completed = false;
        while let Ok(event) = rx.try_recv() {
            if let ToolOrchestratorEvent::Completed {
                call_id,
                tool_id: event_tool_id,
                ..
            } = event
                && call_id == "tid-event-1"
            {
                assert_eq!(event_tool_id, Some(tool_id));
                found_completed = true;
            }
        }
        assert!(found_completed, "should have seen Completed event");
    }

    #[tokio::test]
    async fn completed_event_tool_id_none_without_resolver() {
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

        let mut request = test_request("tid-event-2", "echo");
        request.permission = Permission::AutoApprove;
        orchestrator
            .execute(request, test_context(), CancellationToken::new())
            .await
            .expect("execute should succeed");

        let mut found_completed = false;
        while let Ok(event) = rx.try_recv() {
            if let ToolOrchestratorEvent::Completed {
                call_id,
                tool_id: event_tool_id,
                ..
            } = event
                && call_id == "tid-event-2"
            {
                assert!(event_tool_id.is_none());
                found_completed = true;
            }
        }
        assert!(found_completed, "should have seen Completed event");
    }

    #[tokio::test]
    async fn caller_provided_tool_id_not_overwritten() {
        let mut tools = HashMap::new();
        tools.insert(
            "echo".to_string(),
            Arc::new(EchoTool) as Arc<dyn ExecutableTool>,
        );
        let resolver_tool_id = ToolId::new();
        let caller_tool_id = ToolId::new();
        let mut id_map = HashMap::new();
        id_map.insert("echo".to_string(), resolver_tool_id);
        let orchestrator = DefaultToolOrchestrator::new(
            Arc::new(HashMapResolver::new(tools)),
            Arc::new(MockApprovingService),
            Arc::new(MockSandboxManager(SandboxAttempt::None)),
            RetryPolicy::default(),
        )
        .with_tool_id_resolver(Arc::new(HashMapToolIdResolver::new(id_map)));

        let mut request = test_request("tid-preserve", "echo");
        request.permission = Permission::AutoApprove;
        request.tool_id = Some(caller_tool_id);
        let result = orchestrator
            .execute(request, test_context(), CancellationToken::new())
            .await
            .expect("execute should succeed");

        // The caller-provided tool_id should be preserved, not
        // overwritten by the resolver.
        assert_eq!(result.tool_id, Some(caller_tool_id));
        assert_ne!(result.tool_id, Some(resolver_tool_id));
    }

    #[test]
    fn hash_map_tool_id_resolver_returns_id_for_known_tool() {
        let id = ToolId::new();
        let mut map = HashMap::new();
        map.insert("bash".to_string(), id);
        let resolver = HashMapToolIdResolver::new(map);
        assert_eq!(resolver.resolve_id("bash"), Some(id));
    }

    #[test]
    fn hash_map_tool_id_resolver_returns_none_for_unknown() {
        let resolver = HashMapToolIdResolver::new(HashMap::new());
        assert!(resolver.resolve_id("unknown").is_none());
    }
}

mod provenance_capability_integration_tests {
    use synthia_core::tool::capability::CapabilityBroker;
    use synthia_permission::Permission;
    use synthia_tool_materialization::ToolProvenance;

    use super::*;
    use crate::ToolProvenanceResolver;

    /// A mock provenance resolver backed by a HashMap.
    struct MockProvenanceResolver {
        provenances: HashMap<String, ToolProvenance>,
    }

    impl MockProvenanceResolver {
        fn new(provenances: HashMap<String, ToolProvenance>) -> Self {
            Self { provenances }
        }
    }

    impl ToolProvenanceResolver for MockProvenanceResolver {
        fn resolve_provenance(
            &self,
            tool_name: &str,
        ) -> Option<ToolProvenance> {
            self.provenances.get(tool_name).cloned()
        }
    }

    /// A tool named "bash" that maps to the `command_invoke` capability.
    struct BashTool;

    #[async_trait]
    impl ExecutableTool for BashTool {
        fn name(&self) -> &str {
            "bash"
        }

        fn is_concurrency_safe(&self) -> bool {
            false
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
                tool_id: request.tool_id,
            })
        }
    }

    /// When provenance is Builtin (floor = AutoApprove) and capability is
    /// denied, the capability denial overrides and the call is denied.
    #[tokio::test]
    async fn provenance_builtin_plus_capability_denied_results_in_deny() {
        let mut tools = HashMap::new();
        tools.insert(
            "bash".to_string(),
            Arc::new(BashTool) as Arc<dyn ExecutableTool>,
        );
        let mut provenances = HashMap::new();
        provenances.insert("bash".to_string(), ToolProvenance::Builtin);
        let broker = Arc::new(CapabilityBroker::new(
            synthia_core::tool::capability::ToolCapabilities::default(),
        ));
        let orchestrator = DefaultToolOrchestrator::new(
            Arc::new(HashMapResolver::new(tools)),
            Arc::new(MockApprovingService),
            Arc::new(MockSandboxManager(SandboxAttempt::None)),
            RetryPolicy::default(),
        )
        .with_provenance_resolver(Arc::new(MockProvenanceResolver::new(
            provenances,
        )))
        .with_capability_broker(broker);

        let mut request = test_request("builtin-cap-deny", "bash");
        request.permission = Permission::AutoApprove;
        let result = orchestrator
            .execute(request, test_context(), CancellationToken::new())
            .await;

        assert!(
            matches!(result, Err(crate::ToolOrchestratorError::Denied { .. })),
            "builtin provenance + denied capability → Deny"
        );
    }

    /// When provenance is Builtin (floor = AutoApprove) and capability is
    /// allowed, the call proceeds without approval (AutoApprove).
    #[tokio::test]
    async fn provenance_builtin_plus_allowed_capability_auto_approves() {
        let mut tools = HashMap::new();
        tools.insert(
            "bash".to_string(),
            Arc::new(BashTool) as Arc<dyn ExecutableTool>,
        );
        let mut provenances = HashMap::new();
        provenances.insert("bash".to_string(), ToolProvenance::Builtin);
        let caps = synthia_core::tool::capability::ToolCapabilities {
            command_invoke: true,
            ..Default::default()
        };
        let broker = Arc::new(CapabilityBroker::new(caps));
        let orchestrator = DefaultToolOrchestrator::new(
            Arc::new(HashMapResolver::new(tools)),
            Arc::new(MockApprovingService),
            Arc::new(MockSandboxManager(SandboxAttempt::None)),
            RetryPolicy::default(),
        )
        .with_provenance_resolver(Arc::new(MockProvenanceResolver::new(
            provenances,
        )))
        .with_capability_broker(broker);

        let mut request = test_request("builtin-cap-allow", "bash");
        request.permission = Permission::AutoApprove;
        let result = orchestrator
            .execute(request, test_context(), CancellationToken::new())
            .await;

        assert!(
            result.is_ok(),
            "builtin provenance + allowed capability → AutoApprove"
        );
    }

    /// When provenance is Plugin (floor = RequireConfirm) and capability is
    /// denied, the capability denial overrides the RequireConfirm floor and
    /// the call is denied outright.
    #[tokio::test]
    async fn provenance_plugin_plus_denied_capability_overrides_to_deny() {
        let mut tools = HashMap::new();
        tools.insert(
            "bash".to_string(),
            Arc::new(BashTool) as Arc<dyn ExecutableTool>,
        );
        let mut provenances = HashMap::new();
        provenances.insert(
            "bash".to_string(),
            ToolProvenance::Plugin {
                extension_id: "ext-1".into(),
            },
        );
        let broker = Arc::new(CapabilityBroker::new(
            synthia_core::tool::capability::ToolCapabilities::default(),
        ));
        let orchestrator = DefaultToolOrchestrator::new(
            Arc::new(HashMapResolver::new(tools)),
            Arc::new(MockApprovingService),
            Arc::new(MockSandboxManager(SandboxAttempt::None)),
            RetryPolicy::default(),
        )
        .with_provenance_resolver(Arc::new(MockProvenanceResolver::new(
            provenances,
        )))
        .with_capability_broker(broker);

        let mut request = test_request("plugin-cap-deny", "bash");
        request.permission = Permission::AutoApprove;
        let result = orchestrator
            .execute(request, test_context(), CancellationToken::new())
            .await;

        assert!(
            matches!(result, Err(crate::ToolOrchestratorError::Denied { .. })),
            "plugin provenance + denied capability → Deny (capability overrides)"
        );
    }

    /// When no provenance is resolved, the capability check still applies
    /// and can deny the tool.
    #[tokio::test]
    async fn no_provenance_with_denied_capability_results_in_deny() {
        let mut tools = HashMap::new();
        tools.insert(
            "bash".to_string(),
            Arc::new(BashTool) as Arc<dyn ExecutableTool>,
        );
        // Empty provenance resolver — no provenance for "bash".
        let broker = Arc::new(CapabilityBroker::new(
            synthia_core::tool::capability::ToolCapabilities::default(),
        ));
        let orchestrator = DefaultToolOrchestrator::new(
            Arc::new(HashMapResolver::new(tools)),
            Arc::new(MockApprovingService),
            Arc::new(MockSandboxManager(SandboxAttempt::None)),
            RetryPolicy::default(),
        )
        .with_provenance_resolver(Arc::new(MockProvenanceResolver::new(
            HashMap::new(),
        )))
        .with_capability_broker(broker);

        let mut request = test_request("no-prov-cap-deny", "bash");
        request.permission = Permission::AutoApprove;
        let result = orchestrator
            .execute(request, test_context(), CancellationToken::new())
            .await;

        assert!(
            matches!(result, Err(crate::ToolOrchestratorError::Denied { .. })),
            "no provenance + denied capability → Deny"
        );
    }
}
