// Legacy Tool trait usage during deprecation window (v3 toolification).
#![allow(deprecated)]

use std::{collections::HashMap, path::PathBuf, sync::Arc};

use dashmap::DashMap;
use synthia_core::tool::capability::CapabilityBroker;
use synthia_permission::ApprovalService;
use synthia_sandbox::{SandboxManager, SandboxPolicy};
use tokio::sync::Mutex;

mod edit_conflict;
mod execution;
mod permission;
mod traits;
mod types;

pub use edit_conflict::{
    ConflictInfo,
    FileSnapshot,
    check_conflict,
    record_read,
};
pub use execution::{
    adapter,
    default_tool_resolver,
    default_tool_resolver_with_file_queue,
    needs_serial_routing,
};
pub use permission::{
    ToolProvenanceResolver,
    apply_provenance_floor,
    permission_is_more_restrictive,
};
pub(crate) use permission::{
    capability_for_tool_name,
    extract_file_path,
    is_read_tool,
    is_write_tool,
};
pub use traits::{ExecutableTool, ToolOrchestrator, ToolResolver};
pub(crate) use types::{ActiveCall, ActiveCallGuard};
pub use types::{
    ConcurrencyPolicy,
    DynamicResolver,
    ExecutionContext,
    HashMapResolver,
    HashMapToolIdResolver,
    RetryPolicy,
    ToolCallRequest,
    ToolCallResult,
    ToolExecutionError,
    ToolIdResolver,
    ToolOrchestratorError,
    ToolOrchestratorEvent,
};

/// Default implementation of [`ToolOrchestrator`].
#[derive(Clone)]
pub struct DefaultToolOrchestrator {
    pub(crate) tool_resolver: Arc<dyn ToolResolver>,
    pub(crate) approval_service: Arc<dyn ApprovalService>,
    pub(crate) sandbox_manager: Arc<dyn SandboxManager>,
    pub(crate) retry_policy: RetryPolicy,
    pub(crate) concurrency_policy: ConcurrencyPolicy,
    /// Session-level sandbox policy applied to every tool call unless a
    /// per-request override is added in the future.
    pub(crate) sandbox_policy: SandboxPolicy,
    pub(crate) event_sender:
        tokio::sync::broadcast::Sender<ToolOrchestratorEvent>,
    /// Active call IDs to the [`ActiveCall`] (tool_name + cancellation
    /// token) that controls them.
    pub(crate) active_calls: Arc<DashMap<String, ActiveCall>>,
    /// Per-tool serialization locks for tools that are not concurrency-safe.
    pub(crate) per_tool_locks: Arc<DashMap<String, Arc<Mutex<()>>>>,
    /// Snapshot store for edit conflict detection.
    pub(crate) snapshot_store:
        Arc<tokio::sync::RwLock<HashMap<PathBuf, FileSnapshot>>>,
    /// Optional capability broker that gates tool execution based on
    /// declared capabilities (security B5). When set, the orchestrator
    /// checks the tool's primary capability against the broker before
    /// the approval service is consulted.
    pub(crate) capability_broker: Option<Arc<CapabilityBroker>>,
    /// Optional provenance resolver that looks up a tool's origin so
    /// the orchestrator can enforce minimum permission levels by
    /// provenance (Task 5.1).
    pub(crate) tool_provenance_resolver:
        Option<Arc<dyn ToolProvenanceResolver>>,
    /// Optional resolver that maps tool names to materialized ToolIds.
    /// After `ToolResolver::resolve()` succeeds, the orchestrator
    /// consults this to populate `request.tool_id` for audit
    /// traceability.
    pub(crate) tool_id_resolver: Option<Arc<dyn ToolIdResolver>>,
}

impl DefaultToolOrchestrator {
    /// Create a new orchestrator with the injected services and policies.
    pub fn new(
        tool_resolver: Arc<dyn ToolResolver>,
        approval_service: Arc<dyn ApprovalService>,
        sandbox_manager: Arc<dyn SandboxManager>,
        retry_policy: RetryPolicy,
    ) -> Self {
        Self::with_concurrency_policy(
            tool_resolver,
            approval_service,
            sandbox_manager,
            retry_policy,
            ConcurrencyPolicy::default(),
        )
    }

    /// Create a new orchestrator with an explicit concurrency policy.
    pub fn with_concurrency_policy(
        tool_resolver: Arc<dyn ToolResolver>,
        approval_service: Arc<dyn ApprovalService>,
        sandbox_manager: Arc<dyn SandboxManager>,
        retry_policy: RetryPolicy,
        concurrency_policy: ConcurrencyPolicy,
    ) -> Self {
        Self::with_sandbox_policy(
            tool_resolver,
            approval_service,
            sandbox_manager,
            retry_policy,
            concurrency_policy,
            SandboxPolicy::Standard,
        )
    }

    /// Create a new orchestrator with an explicit session-level sandbox policy.
    pub fn with_sandbox_policy(
        tool_resolver: Arc<dyn ToolResolver>,
        approval_service: Arc<dyn ApprovalService>,
        sandbox_manager: Arc<dyn SandboxManager>,
        retry_policy: RetryPolicy,
        concurrency_policy: ConcurrencyPolicy,
        sandbox_policy: SandboxPolicy,
    ) -> Self {
        let (event_sender, _) = tokio::sync::broadcast::channel(256);
        Self {
            tool_resolver,
            approval_service,
            sandbox_manager,
            retry_policy,
            concurrency_policy,
            sandbox_policy,
            event_sender,
            active_calls: Arc::new(DashMap::new()),
            per_tool_locks: Arc::new(DashMap::new()),
            snapshot_store: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
            capability_broker: None,
            tool_provenance_resolver: None,
            tool_id_resolver: None,
        }
    }

    /// Set the capability broker for this orchestrator.
    pub fn with_capability_broker(
        mut self,
        broker: Arc<CapabilityBroker>,
    ) -> Self {
        self.capability_broker = Some(broker);
        self
    }

    /// Set the provenance resolver for this orchestrator.
    pub fn with_provenance_resolver(
        mut self,
        resolver: Arc<dyn ToolProvenanceResolver>,
    ) -> Self {
        self.tool_provenance_resolver = Some(resolver);
        self
    }

    /// Set the ToolId resolver for this orchestrator.
    ///
    /// When set, the orchestrator populates `request.tool_id` after a
    /// successful `ToolResolver::resolve()` call, enabling audit
    /// traceability of materialized tool instances.
    pub fn with_tool_id_resolver(
        mut self,
        resolver: Arc<dyn ToolIdResolver>,
    ) -> Self {
        self.tool_id_resolver = Some(resolver);
        self
    }
}

#[cfg(test)]
impl DefaultToolOrchestrator {
    pub(crate) fn has_active_call(&self, call_id: &str) -> bool {
        self.active_calls.contains_key(call_id)
    }

    /// Register a synthetic active call for testing `fail_interrupted_tools`.
    ///
    /// Returns the call's `CancellationToken` so tests can assert that it
    /// was cancelled.
    pub(crate) fn register_test_active_call(
        &self,
        call_id: &str,
        tool_name: &str,
    ) -> tokio_util::sync::CancellationToken {
        let token = tokio_util::sync::CancellationToken::new();
        self.active_calls.insert(
            call_id.to_string(),
            ActiveCall {
                tool_name: tool_name.to_string(),
                token: token.clone(),
            },
        );
        token
    }
}

#[cfg(test)]
mod inline_tests {
    use std::{
        path::PathBuf,
        sync::atomic::{AtomicBool, AtomicUsize, Ordering},
        time::Duration,
    };

    use synthia_permission::{ApprovalError, HeadlessApprovalService};
    use synthia_sandbox::{
        SandboxAttempt,
        SandboxError,
        SandboxManager,
        SandboxPolicy,
    };

    use super::*;

    struct NoopSandboxManager;

    #[async_trait::async_trait]
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

    struct UnavailableSandboxManager;

    #[async_trait::async_trait]
    impl SandboxManager for UnavailableSandboxManager {
        async fn select(
            &self,
            _policy: SandboxPolicy,
            _tool_type: &str,
            _platform: &str,
        ) -> Result<SandboxAttempt, SandboxError> {
            Ok(SandboxAttempt::Unavailable)
        }
    }

    struct EchoTool;

    #[async_trait::async_trait]
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
            _cancellation_token: tokio_util::sync::CancellationToken,
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

    struct FailingTool {
        calls: Arc<AtomicUsize>,
        fail_transient_until: usize,
    }

    #[async_trait::async_trait]
    impl ExecutableTool for FailingTool {
        fn name(&self) -> &str {
            "flaky"
        }

        async fn execute(
            &self,
            request: &ToolCallRequest,
            _context: &ExecutionContext,
            _sandbox_attempt: &SandboxAttempt,
            _cancellation_token: tokio_util::sync::CancellationToken,
        ) -> Result<ToolCallResult, ToolExecutionError> {
            let count = self.calls.fetch_add(1, Ordering::SeqCst);
            if count < self.fail_transient_until {
                return Err(ToolExecutionError::Transient(format!(
                    "attempt {} failed",
                    count + 1
                )));
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

    struct SlowTool {
        delay: Duration,
    }

    #[async_trait::async_trait]
    impl ExecutableTool for SlowTool {
        fn name(&self) -> &str {
            "slow"
        }

        async fn execute(
            &self,
            request: &ToolCallRequest,
            _context: &ExecutionContext,
            _sandbox_attempt: &SandboxAttempt,
            cancellation_token: tokio_util::sync::CancellationToken,
        ) -> Result<ToolCallResult, ToolExecutionError> {
            tokio::select! {
                _ = tokio::time::sleep(self.delay) => {}
                _ = cancellation_token.cancelled() => {
                    return Err(ToolExecutionError::Cancelled);
                }
            }
            Ok(ToolCallResult {
                call_id: request.call_id.clone(),
                tool_name: request.tool_name.clone(),
                outcome: serde_json::json!("done"),
                is_error: false,
                tool_id: request.tool_id,
            })
        }
    }

    struct RecordingApprovalService {
        calls: AtomicUsize,
    }

    #[async_trait::async_trait]
    impl synthia_permission::ApprovalService for RecordingApprovalService {
        async fn request_approval(
            &self,
            _tool: &str,
            _args: &serde_json::Value,
            _policy: synthia_permission::ApprovalPolicy,
            _timeout: Duration,
            _cancel: tokio_util::sync::CancellationToken,
        ) -> Result<synthia_permission::ApprovalOutcome, ApprovalError>
        {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(synthia_permission::ApprovalOutcome::Deny)
        }

        fn ask(
            &self,
            _request: synthia_permission::PermissionRequest,
        ) -> synthia_permission::PermissionFuture {
            synthia_permission::PermissionFuture::immediate_denied()
        }
    }

    struct TimeoutApprovalService;

    #[async_trait::async_trait]
    impl synthia_permission::ApprovalService for TimeoutApprovalService {
        async fn request_approval(
            &self,
            _tool: &str,
            _args: &serde_json::Value,
            _policy: synthia_permission::ApprovalPolicy,
            _timeout: Duration,
            _cancel: tokio_util::sync::CancellationToken,
        ) -> Result<synthia_permission::ApprovalOutcome, ApprovalError>
        {
            Err(ApprovalError::Timeout)
        }

        fn ask(
            &self,
            _request: synthia_permission::PermissionRequest,
        ) -> synthia_permission::PermissionFuture {
            synthia_permission::PermissionFuture::immediate_denied()
        }
    }

    fn test_orchestrator_with_tool(
        tool: Arc<dyn ExecutableTool>,
    ) -> DefaultToolOrchestrator {
        let mut tools = HashMap::new();
        tools.insert(tool.name().to_string(), tool);
        DefaultToolOrchestrator::new(
            Arc::new(HashMapResolver::new(tools)),
            Arc::new(HeadlessApprovalService),
            Arc::new(NoopSandboxManager),
            RetryPolicy::default(),
        )
    }

    fn test_request(call_id: &str, tool_name: &str) -> ToolCallRequest {
        ToolCallRequest {
            call_id: call_id.to_string(),
            tool_name: tool_name.to_string(),
            arguments: serde_json::json!({}),
            permission: synthia_permission::Permission::RequireConfirm,
            tool_id: None,
        }
    }

    fn test_context() -> ExecutionContext {
        ExecutionContext {
            session_id: "session-1".to_string(),
            workspace_root: PathBuf::from("/tmp"),
            caller_agent: "agent-1".to_string(),
        }
    }

    #[tokio::test]
    async fn execute_returns_denied_with_headless_service() {
        let orchestrator = test_orchestrator_with_tool(Arc::new(EchoTool));
        let result = orchestrator
            .execute(
                test_request("call-1", "echo"),
                test_context(),
                tokio_util::sync::CancellationToken::new(),
            )
            .await;

        assert!(matches!(result, Err(ToolOrchestratorError::Denied { .. })));
    }

    #[tokio::test]
    async fn autoapprove_skips_approval_service() {
        let service = Arc::new(RecordingApprovalService {
            calls: AtomicUsize::new(0),
        });
        let mut tools = HashMap::new();
        tools.insert(
            "echo".to_string(),
            Arc::new(EchoTool) as Arc<dyn ExecutableTool>,
        );
        let orchestrator = DefaultToolOrchestrator::new(
            Arc::new(HashMapResolver::new(tools)),
            service.clone(),
            Arc::new(NoopSandboxManager),
            RetryPolicy::default(),
        );
        let mut request = test_request("auto-1", "echo");
        request.permission = synthia_permission::Permission::AutoApprove;

        let result = orchestrator
            .execute(
                request,
                test_context(),
                tokio_util::sync::CancellationToken::new(),
            )
            .await;

        assert!(result.is_ok());
        assert_eq!(service.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn approval_timeout_is_treated_as_deny() {
        let mut tools = HashMap::new();
        tools.insert(
            "echo".to_string(),
            Arc::new(EchoTool) as Arc<dyn ExecutableTool>,
        );
        let orchestrator = DefaultToolOrchestrator::new(
            Arc::new(HashMapResolver::new(tools)),
            Arc::new(TimeoutApprovalService),
            Arc::new(NoopSandboxManager),
            RetryPolicy::default(),
        );

        let result = orchestrator
            .execute(
                test_request("timeout-1", "echo"),
                test_context(),
                tokio_util::sync::CancellationToken::new(),
            )
            .await;

        assert!(matches!(result, Err(ToolOrchestratorError::Denied { .. })));
    }

    #[tokio::test]
    async fn execute_respects_cancellation_token() {
        let orchestrator = test_orchestrator_with_tool(Arc::new(EchoTool));
        let cancel = tokio_util::sync::CancellationToken::new();
        cancel.cancel();

        let result = orchestrator
            .execute(test_request("call-2", "echo"), test_context(), cancel)
            .await;

        assert!(matches!(
            result,
            Err(ToolOrchestratorError::Cancelled { .. })
        ));
    }

    #[tokio::test]
    async fn execute_returns_not_found_for_unknown_tool() {
        let orchestrator = test_orchestrator_with_tool(Arc::new(EchoTool));
        let result = orchestrator
            .execute(
                test_request("call-3", "missing"),
                test_context(),
                tokio_util::sync::CancellationToken::new(),
            )
            .await;

        assert!(matches!(
            result,
            Err(ToolOrchestratorError::NotFound { .. })
        ));
    }

    #[tokio::test]
    async fn execute_retries_transient_errors_and_succeeds() {
        let calls = Arc::new(AtomicUsize::new(0));
        let mut tools = HashMap::new();
        tools.insert(
            "flaky".to_string(),
            Arc::new(FailingTool {
                calls: calls.clone(),
                fail_transient_until: 2,
            }) as Arc<dyn ExecutableTool>,
        );
        let orchestrator = DefaultToolOrchestrator::new(
            Arc::new(HashMapResolver::new(tools)),
            Arc::new(HeadlessApprovalService),
            Arc::new(NoopSandboxManager),
            RetryPolicy {
                max_attempts: 3,
                base_delay_ms: 1,
            },
        );
        let mut request = test_request("retry-1", "flaky");
        request.permission = synthia_permission::Permission::AutoApprove;

        let result = orchestrator
            .execute(
                request,
                test_context(),
                tokio_util::sync::CancellationToken::new(),
            )
            .await;

        assert!(result.is_ok());
        assert_eq!(calls.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn execute_reports_permanent_error_without_retry() {
        struct PermanentFailTool;

        #[async_trait::async_trait]
        impl ExecutableTool for PermanentFailTool {
            fn name(&self) -> &str {
                "permanent"
            }

            async fn execute(
                &self,
                request: &ToolCallRequest,
                _context: &ExecutionContext,
                _sandbox_attempt: &SandboxAttempt,
                _cancellation_token: tokio_util::sync::CancellationToken,
            ) -> Result<ToolCallResult, ToolExecutionError> {
                Err(ToolExecutionError::Permanent(format!(
                    "permanent failure for {}",
                    request.call_id
                )))
            }
        }

        let mut tools = HashMap::new();
        tools.insert(
            "permanent".to_string(),
            Arc::new(PermanentFailTool) as Arc<dyn ExecutableTool>,
        );
        let orchestrator = DefaultToolOrchestrator::new(
            Arc::new(HashMapResolver::new(tools)),
            Arc::new(HeadlessApprovalService),
            Arc::new(NoopSandboxManager),
            RetryPolicy {
                max_attempts: 3,
                base_delay_ms: 1,
            },
        );
        let mut request = test_request("perm-1", "permanent");
        request.permission = synthia_permission::Permission::AutoApprove;

        let result = orchestrator
            .execute(
                request,
                test_context(),
                tokio_util::sync::CancellationToken::new(),
            )
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn sandbox_unavailable_fails_closed() {
        let mut tools = HashMap::new();
        tools.insert(
            "echo".to_string(),
            Arc::new(EchoTool) as Arc<dyn ExecutableTool>,
        );
        let orchestrator = DefaultToolOrchestrator::new(
            Arc::new(HashMapResolver::new(tools)),
            Arc::new(HeadlessApprovalService),
            Arc::new(UnavailableSandboxManager),
            RetryPolicy::default(),
        );
        let mut request = test_request("sandbox-1", "echo");
        request.permission = synthia_permission::Permission::AutoApprove;

        let result = orchestrator
            .execute(
                request,
                test_context(),
                tokio_util::sync::CancellationToken::new(),
            )
            .await;

        assert!(matches!(result, Err(ToolOrchestratorError::Sandbox { .. })));
    }

    #[tokio::test]
    async fn execute_batch_runs_multiple_requests() {
        let orchestrator = test_orchestrator_with_tool(Arc::new(EchoTool));
        let requests =
            vec![test_request("a", "echo"), test_request("b", "echo")];
        let requests: Vec<_> = requests
            .into_iter()
            .map(|mut r| {
                r.permission = synthia_permission::Permission::AutoApprove;
                r
            })
            .collect();

        let result = orchestrator
            .execute_batch(
                requests,
                test_context(),
                tokio_util::sync::CancellationToken::new(),
            )
            .await;

        assert!(result.is_ok());
        let results = result.unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].call_id, "a");
        assert_eq!(results[1].call_id, "b");
    }

    #[tokio::test]
    async fn event_stream_receives_events() {
        let orchestrator = test_orchestrator_with_tool(Arc::new(EchoTool));
        let mut rx = orchestrator.event_stream();

        orchestrator.cancel("call-x").await.unwrap();

        let event = rx.try_recv().expect("event received");
        assert!(
            matches!(event, ToolOrchestratorEvent::Cancelled { call_id, .. } if call_id == "call-x")
        );
    }

    #[tokio::test]
    async fn cancel_stops_in_flight_call() {
        let orchestrator = test_orchestrator_with_tool(Arc::new(SlowTool {
            delay: Duration::from_secs(60),
        }));
        let cancel = tokio_util::sync::CancellationToken::new();
        let mut request = test_request("slow-1", "slow");
        request.permission = synthia_permission::Permission::AutoApprove;

        let orchestrator_clone = orchestrator.clone();
        let handle = tokio::spawn(async move {
            orchestrator_clone
                .execute(request, test_context(), cancel.clone())
                .await
        });

        let deadline = tokio::time::Instant::now() + Duration::from_secs(1);
        while !orchestrator.has_active_call("slow-1")
            && tokio::time::Instant::now() < deadline
        {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        orchestrator.cancel("slow-1").await.unwrap();
        let result = handle.await.unwrap();

        assert!(matches!(
            result,
            Err(ToolOrchestratorError::Cancelled { .. })
        ));
    }

    #[tokio::test]
    async fn test_fail_interrupted_multiple_tools() {
        let orchestrator = test_orchestrator_with_tool(Arc::new(EchoTool));
        let mut rx = orchestrator.event_stream();

        let token_a = orchestrator.register_test_active_call("call-a", "echo");
        let token_b = orchestrator.register_test_active_call("call-b", "read");
        let token_c = orchestrator.register_test_active_call("call-c", "grep");

        assert!(orchestrator.has_active_call("call-a"));
        assert!(orchestrator.has_active_call("call-b"));
        assert!(orchestrator.has_active_call("call-c"));

        let count = orchestrator.fail_interrupted_tools();

        assert_eq!(count, 3);
        assert!(!orchestrator.has_active_call("call-a"));
        assert!(!orchestrator.has_active_call("call-b"));
        assert!(!orchestrator.has_active_call("call-c"));
        assert!(token_a.is_cancelled());
        assert!(token_b.is_cancelled());
        assert!(token_c.is_cancelled());

        let mut failed_events = Vec::new();
        while let Ok(event) = rx.try_recv() {
            if let ToolOrchestratorEvent::Failed {
                call_id,
                tool_name,
                error,
            } = event
            {
                failed_events.push((call_id, tool_name, error));
            }
        }
        assert_eq!(failed_events.len(), 3);
        for (_, _, error) in &failed_events {
            assert_eq!(error, "Tool execution interrupted");
        }
        let mut names: Vec<String> =
            failed_events.into_iter().map(|(_, name, _)| name).collect();
        names.sort();
        assert_eq!(names, vec!["echo", "grep", "read"]);
    }

    #[tokio::test]
    async fn test_fail_interrupted_no_active_tools() {
        let orchestrator = test_orchestrator_with_tool(Arc::new(EchoTool));
        let mut rx = orchestrator.event_stream();

        let count = orchestrator.fail_interrupted_tools();

        assert_eq!(count, 0);
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn test_fail_interrupted_skips_already_completed_calls() {
        let orchestrator = test_orchestrator_with_tool(Arc::new(EchoTool));
        let mut rx = orchestrator.event_stream();

        let _token_a = orchestrator.register_test_active_call("call-a", "echo");
        let _token_b = orchestrator.register_test_active_call("call-b", "read");

        assert!(orchestrator.active_calls.remove("call-b").is_some());

        let count = orchestrator.fail_interrupted_tools();

        assert_eq!(count, 1);
        assert!(!orchestrator.has_active_call("call-a"));
        assert!(!orchestrator.has_active_call("call-b"));

        let mut failed = 0;
        while let Ok(event) = rx.try_recv() {
            if matches!(event, ToolOrchestratorEvent::Failed { .. }) {
                failed += 1;
            }
        }
        assert_eq!(failed, 1);
    }

    #[tokio::test]
    async fn test_interrupted_events_persisted() {
        let orchestrator = test_orchestrator_with_tool(Arc::new(EchoTool));
        let mut rx = orchestrator.event_stream();

        orchestrator.register_test_active_call("call-x", "write");
        orchestrator.register_test_active_call("call-y", "bash");

        let count = orchestrator.fail_interrupted_tools();
        assert_eq!(count, 2);

        let mut events: Vec<(String, String, String)> = Vec::new();
        while let Ok(event) = rx.try_recv() {
            if let ToolOrchestratorEvent::Failed {
                call_id,
                tool_name,
                error,
            } = event
            {
                events.push((call_id, tool_name, error));
            }
        }
        assert_eq!(events.len(), 2);
        for (call_id, tool_name, error) in &events {
            assert_eq!(error, "Tool execution interrupted");
            let valid = (call_id == "call-x" && tool_name == "write")
                || (call_id == "call-y" && tool_name == "bash");
            assert!(valid, "unexpected (call_id, tool_name) pair");
        }
    }

    mod dynamic_resolver_tests {
        use async_trait::async_trait;
        use synthia_tool::{Tool, ToolInput, ToolOutput};

        use super::*;
        use crate::adapter::ToolAdapter;

        struct McpStyleTool;

        #[async_trait]
        impl Tool for McpStyleTool {
            fn name(&self) -> &str {
                "mcp-style-echo"
            }

            fn description(&self) -> &str {
                "mock MCP-style tool"
            }

            fn parameters(&self) -> serde_json::Value {
                serde_json::json!({})
            }

            fn is_concurrency_safe(&self) -> bool {
                true
            }

            async fn call(&self, input: ToolInput) -> ToolOutput {
                ToolOutput::text(format!("mcp echo: {}", input.input))
            }
        }

        fn test_context() -> ExecutionContext {
            ExecutionContext {
                session_id: "s1".to_string(),
                workspace_root: std::path::PathBuf::from("/tmp"),
                caller_agent: "agent-1".to_string(),
            }
        }

        #[tokio::test]
        async fn dynamic_resolver_registers_and_executes_mcp_style_tool() {
            let dynamic_resolver = DynamicResolver::new();
            let resolver: Arc<dyn ToolResolver> =
                Arc::new(dynamic_resolver.clone());
            let orchestrator = DefaultToolOrchestrator::new(
                resolver,
                Arc::new(HeadlessApprovalService),
                Arc::new(NoopSandboxManager),
                RetryPolicy::default(),
            );

            let tool = Arc::new(McpStyleTool);
            let name = tool.name().to_string();
            dynamic_resolver.register(name, Arc::new(ToolAdapter::new(tool)));

            let mut request = ToolCallRequest {
                call_id: "c1".to_string(),
                tool_name: "mcp-style-echo".to_string(),
                arguments: serde_json::json!({"k": "v"}),
                permission: synthia_permission::Permission::RequireConfirm,
                tool_id: None,
            };
            request.permission = synthia_permission::Permission::AutoApprove;

            let result = orchestrator
                .execute(
                    request,
                    test_context(),
                    tokio_util::sync::CancellationToken::new(),
                )
                .await
                .expect("execute succeeds");

            assert_eq!(result.tool_name, "mcp-style-echo");
            let text = result
                .outcome
                .as_array()
                .and_then(|a| a.first())
                .and_then(|p| p.get("text"))
                .and_then(|t| t.as_str())
                .expect("text outcome");
            assert!(text.contains("mcp echo:"));
        }
    }

    mod execution_mode_routing_tests {
        use async_trait::async_trait;
        use synthia_tool::{Tool, ToolInput, ToolOutput};

        use super::*;
        use crate::{adapter::ToolAdapter, needs_serial_routing};

        struct ParallelTool;
        #[async_trait]
        impl Tool for ParallelTool {
            fn name(&self) -> &str {
                "parallel_tool"
            }

            fn description(&self) -> &str {
                "parallel"
            }

            fn parameters(&self) -> serde_json::Value {
                serde_json::json!({})
            }

            fn execution_mode(&self) -> synthia_tool::traits::ExecutionMode {
                synthia_tool::traits::ExecutionMode::Parallel
            }

            fn is_concurrency_safe(&self) -> bool {
                true
            }

            async fn call(&self, _input: ToolInput) -> ToolOutput {
                ToolOutput::text("ok")
            }
        }

        struct SequentialTool;
        #[async_trait]
        impl Tool for SequentialTool {
            fn name(&self) -> &str {
                "sequential_tool"
            }

            fn description(&self) -> &str {
                "sequential"
            }

            fn parameters(&self) -> serde_json::Value {
                serde_json::json!({})
            }

            fn execution_mode(&self) -> synthia_tool::traits::ExecutionMode {
                synthia_tool::traits::ExecutionMode::Sequential
            }

            fn is_concurrency_safe(&self) -> bool {
                false
            }

            async fn call(&self, _input: ToolInput) -> ToolOutput {
                ToolOutput::text("ok")
            }
        }

        #[test]
        fn needs_serial_routing_returns_false_for_parallel_batch() {
            let mut tools = HashMap::new();
            tools.insert(
                "parallel_tool".to_string(),
                Arc::new(ToolAdapter::new(Arc::new(ParallelTool)))
                    as Arc<dyn ExecutableTool>,
            );
            let resolver = HashMapResolver::new(tools);
            let requests = vec![
                test_request("c1", "parallel_tool"),
                test_request("c2", "parallel_tool"),
            ];
            assert!(!needs_serial_routing(&requests, &resolver));
        }

        #[test]
        fn needs_serial_routing_returns_true_when_any_sequential() {
            let mut tools = HashMap::new();
            tools.insert(
                "parallel_tool".to_string(),
                Arc::new(ToolAdapter::new(Arc::new(ParallelTool)))
                    as Arc<dyn ExecutableTool>,
            );
            tools.insert(
                "sequential_tool".to_string(),
                Arc::new(ToolAdapter::new(Arc::new(SequentialTool)))
                    as Arc<dyn ExecutableTool>,
            );
            let resolver = HashMapResolver::new(tools);
            let requests = vec![
                test_request("c1", "parallel_tool"),
                test_request("c2", "sequential_tool"),
            ];
            assert!(needs_serial_routing(&requests, &resolver));
        }

        #[test]
        fn needs_serial_routing_fails_closed_for_unknown_tool() {
            let resolver = HashMapResolver::new(HashMap::new());
            let requests = vec![test_request("c1", "unknown")];
            assert!(needs_serial_routing(&requests, &resolver));
        }

        #[tokio::test]
        async fn execute_batch_with_sequential_tool_runs_serially() {
            use std::sync::atomic::{AtomicUsize, Ordering};

            struct CountingTool(Arc<AtomicUsize>);
            #[async_trait]
            impl Tool for CountingTool {
                fn name(&self) -> &str {
                    "counter"
                }

                fn description(&self) -> &str {
                    "counter"
                }

                fn parameters(&self) -> serde_json::Value {
                    serde_json::json!({})
                }

                fn execution_mode(
                    &self,
                ) -> synthia_tool::traits::ExecutionMode {
                    synthia_tool::traits::ExecutionMode::Sequential
                }

                async fn call(&self, _input: ToolInput) -> ToolOutput {
                    self.0.fetch_add(1, Ordering::SeqCst);
                    ToolOutput::text(format!(
                        "{}",
                        self.0.load(Ordering::SeqCst)
                    ))
                }
            }

            let counter = Arc::new(AtomicUsize::new(0));
            let tool = Arc::new(CountingTool(counter.clone()));
            let mut tools = HashMap::new();
            tools.insert(
                "counter".to_string(),
                Arc::new(ToolAdapter::new(tool)) as Arc<dyn ExecutableTool>,
            );
            let orchestrator = DefaultToolOrchestrator::new(
                Arc::new(HashMapResolver::new(tools)),
                Arc::new(HeadlessApprovalService),
                Arc::new(NoopSandboxManager),
                RetryPolicy::default(),
            );

            let requests = vec![
                {
                    let mut r = test_request("c1", "counter");
                    r.permission = synthia_permission::Permission::AutoApprove;
                    r
                },
                {
                    let mut r = test_request("c2", "counter");
                    r.permission = synthia_permission::Permission::AutoApprove;
                    r
                },
                {
                    let mut r = test_request("c3", "counter");
                    r.permission = synthia_permission::Permission::AutoApprove;
                    r
                },
            ];
            let results = orchestrator
                .execute_batch(
                    requests,
                    test_context(),
                    tokio_util::sync::CancellationToken::new(),
                )
                .await
                .expect("batch should succeed");
            assert_eq!(results.len(), 3);
            assert_eq!(counter.load(Ordering::SeqCst), 3);
        }
    }

    mod adapter_tests {
        use async_trait::async_trait;
        use synthia_tool::{Tool, ToolInput, ToolOutput};

        use super::*;
        use crate::adapter::ToolAdapter;

        struct MockTool {
            name: &'static str,
            concurrency_safe: bool,
            return_error: bool,
        }

        #[async_trait]
        impl Tool for MockTool {
            fn name(&self) -> &str {
                self.name
            }

            fn description(&self) -> &str {
                "mock tool for adapter tests"
            }

            fn parameters(&self) -> serde_json::Value {
                serde_json::json!({})
            }

            fn is_concurrency_safe(&self) -> bool {
                self.concurrency_safe
            }

            async fn call(&self, input: ToolInput) -> ToolOutput {
                if self.return_error {
                    ToolOutput::error(format!("mock error for {}", input.name))
                } else {
                    ToolOutput::text(format!("ok: {}", input.input))
                }
            }
        }

        fn adapter_request(name: &str) -> ToolCallRequest {
            ToolCallRequest {
                call_id: "c1".to_string(),
                tool_name: name.to_string(),
                arguments: serde_json::json!({"key": "value"}),
                permission: synthia_permission::Permission::RequireConfirm,
                tool_id: None,
            }
        }

        fn adapter_context() -> ExecutionContext {
            ExecutionContext {
                session_id: "s1".to_string(),
                workspace_root: std::path::PathBuf::from("/tmp"),
                caller_agent: "agent-1".to_string(),
            }
        }

        #[tokio::test]
        async fn adapter_returns_permanent_error_on_name_mismatch() {
            let adapter = ToolAdapter::new(Arc::new(MockTool {
                name: "expected",
                concurrency_safe: false,
                return_error: false,
            }));
            let result = adapter
                .execute(
                    &adapter_request("wrong"),
                    &adapter_context(),
                    &synthia_sandbox::SandboxAttempt::None,
                    tokio_util::sync::CancellationToken::new(),
                )
                .await;
            assert!(
                matches!(result, Err(ToolExecutionError::Permanent(ref m)) if m.contains("mismatch"))
            );
        }

        #[tokio::test]
        async fn adapter_calls_tool_and_serializes_output() {
            let adapter = ToolAdapter::new(Arc::new(MockTool {
                name: "expected",
                concurrency_safe: true,
                return_error: false,
            }));
            let result = adapter
                .execute(
                    &adapter_request("expected"),
                    &adapter_context(),
                    &synthia_sandbox::SandboxAttempt::None,
                    tokio_util::sync::CancellationToken::new(),
                )
                .await
                .expect("execute succeeds");
            assert_eq!(result.call_id, "c1");
            assert_eq!(result.tool_name, "expected");
            assert!(!result.is_error);
            assert!(result.outcome.is_array());
        }

        #[tokio::test]
        async fn adapter_propagates_concurrency_safe_flag() {
            let adapter = ToolAdapter::new(Arc::new(MockTool {
                name: "safe",
                concurrency_safe: true,
                return_error: false,
            }));
            assert!(adapter.is_concurrency_safe());
        }

        #[tokio::test]
        async fn adapter_maps_error_flag_from_output() {
            let adapter = ToolAdapter::new(Arc::new(MockTool {
                name: "expected",
                concurrency_safe: false,
                return_error: true,
            }));
            let result = adapter
                .execute(
                    &adapter_request("expected"),
                    &adapter_context(),
                    &synthia_sandbox::SandboxAttempt::None,
                    tokio_util::sync::CancellationToken::new(),
                )
                .await
                .expect("execute succeeds");
            assert!(result.is_error);
        }

        struct SchemaMockTool {
            name: &'static str,
            parameters: serde_json::Value,
            call_invoked: Arc<AtomicBool>,
        }

        impl SchemaMockTool {
            fn new(
                name: &'static str,
                parameters: serde_json::Value,
            ) -> (Arc<Self>, ToolAdapter) {
                let tool = Arc::new(Self {
                    name,
                    parameters,
                    call_invoked: Arc::new(AtomicBool::new(false)),
                });
                let adapter = ToolAdapter::new(tool.clone());
                (tool, adapter)
            }

            fn was_called(&self) -> bool {
                self.call_invoked.load(Ordering::SeqCst)
            }
        }

        #[async_trait]
        impl Tool for SchemaMockTool {
            fn name(&self) -> &str {
                self.name
            }

            fn description(&self) -> &str {
                "schema mock tool for validation tests"
            }

            fn parameters(&self) -> serde_json::Value {
                self.parameters.clone()
            }

            async fn call(&self, input: ToolInput) -> ToolOutput {
                self.call_invoked.store(true, Ordering::SeqCst);
                ToolOutput::text(format!("ok: {}", input.input))
            }
        }

        fn validation_request(
            name: &str,
            arguments: serde_json::Value,
        ) -> ToolCallRequest {
            ToolCallRequest {
                call_id: "c1".to_string(),
                tool_name: name.to_string(),
                arguments,
                permission: synthia_permission::Permission::RequireConfirm,
                tool_id: None,
            }
        }

        fn validation_context() -> ExecutionContext {
            ExecutionContext {
                session_id: "s1".to_string(),
                workspace_root: std::path::PathBuf::from("/tmp"),
                caller_agent: "agent-1".to_string(),
            }
        }

        fn schema_tool() -> (Arc<SchemaMockTool>, ToolAdapter) {
            SchemaMockTool::new(
                "schema-tool",
                serde_json::json!({
                    "type": "object",
                    "properties": { "file_path": {"type": "string"}, "offset": {"type": "integer"}, "limit": {"type": "integer"} },
                    "required": ["file_path"]
                }),
            )
        }

        #[tokio::test]
        async fn valid_input_passes_validation() {
            let (tool, adapter) = schema_tool();
            let result = adapter
                .execute(
                    &validation_request(
                        "schema-tool",
                        serde_json::json!({"file_path": "/tmp/x"}),
                    ),
                    &validation_context(),
                    &synthia_sandbox::SandboxAttempt::None,
                    tokio_util::sync::CancellationToken::new(),
                )
                .await
                .expect("execute succeeds");
            assert!(!result.is_error);
            assert!(tool.was_called());
        }

        #[tokio::test]
        async fn invalid_type_rejected() {
            let (tool, adapter) = schema_tool();
            let result = adapter
                .execute(
                    &validation_request(
                        "schema-tool",
                        serde_json::json!({"file_path": 123}),
                    ),
                    &validation_context(),
                    &synthia_sandbox::SandboxAttempt::None,
                    tokio_util::sync::CancellationToken::new(),
                )
                .await;
            assert!(
                matches!(result, Err(ToolExecutionError::Permanent(ref m)) if m.contains("file_path"))
            );
            assert!(!tool.was_called());
        }

        #[tokio::test]
        async fn missing_required_field_rejected() {
            let (tool, adapter) = schema_tool();
            let result = adapter
                .execute(
                    &validation_request("schema-tool", serde_json::json!({})),
                    &validation_context(),
                    &synthia_sandbox::SandboxAttempt::None,
                    tokio_util::sync::CancellationToken::new(),
                )
                .await;
            assert!(
                matches!(result, Err(ToolExecutionError::Permanent(ref m)) if m.contains("file_path"))
            );
            assert!(!tool.was_called());
        }

        #[tokio::test]
        async fn error_message_visible_to_caller() {
            let (_tool, adapter) = schema_tool();
            let result = adapter
                .execute(
                    &validation_request(
                        "schema-tool",
                        serde_json::json!({"file_path": 123}),
                    ),
                    &validation_context(),
                    &synthia_sandbox::SandboxAttempt::None,
                    tokio_util::sync::CancellationToken::new(),
                )
                .await;
            let msg = match result {
                Err(ToolExecutionError::Permanent(m)) => m,
                other => panic!("expected Permanent error, got {other:?}"),
            };
            assert!(msg.contains("file_path"), "msg = {msg}");
            assert!(msg.contains("string"), "msg = {msg}");
        }

        #[tokio::test]
        async fn unknown_fields_ignored() {
            let (tool, adapter) = schema_tool();
            let result = adapter.execute(&validation_request("schema-tool", serde_json::json!({"file_path": "/tmp/x", "unknown_field": "ignored"})), &validation_context(), &synthia_sandbox::SandboxAttempt::None, tokio_util::sync::CancellationToken::new()).await.expect("execute succeeds");
            assert!(!result.is_error);
            assert!(tool.was_called());
        }

        #[tokio::test]
        async fn non_object_arguments_rejected() {
            let (tool, adapter) = schema_tool();
            let result = adapter
                .execute(
                    &validation_request(
                        "schema-tool",
                        serde_json::json!("a string"),
                    ),
                    &validation_context(),
                    &synthia_sandbox::SandboxAttempt::None,
                    tokio_util::sync::CancellationToken::new(),
                )
                .await;
            assert!(
                matches!(result, Err(ToolExecutionError::Permanent(ref m)) if m.contains("JSON object"))
            );
            assert!(!tool.was_called());
        }
    }

    mod file_queue_integration_tests {
        use std::{
            path::PathBuf,
            sync::atomic::{AtomicUsize, Ordering},
            time::Duration,
        };

        use async_trait::async_trait;
        use synthia_tool::{Tool, ToolInput, ToolOutput};
        use synthia_tool_exec_base::FileMutationQueue;
        use tokio_util::sync::CancellationToken;

        use super::*;
        use crate::adapter::ToolAdapter;

        struct SlowFileTool {
            name: &'static str,
            current: Arc<AtomicUsize>,
            max_concurrent: Arc<AtomicUsize>,
        }

        #[async_trait]
        impl Tool for SlowFileTool {
            fn name(&self) -> &str {
                self.name
            }

            fn description(&self) -> &str {
                "slow file tool for queue tests"
            }

            fn parameters(&self) -> serde_json::Value {
                serde_json::json!({ "type": "object", "required": ["path"], "properties": { "path": {"type": "string"} } })
            }

            fn is_concurrency_safe(&self) -> bool {
                true
            }

            async fn call(&self, input: ToolInput) -> ToolOutput {
                let cur = self.current.fetch_add(1, Ordering::SeqCst) + 1;
                self.max_concurrent.fetch_max(cur, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(50)).await;
                self.current.fetch_sub(1, Ordering::SeqCst);
                ToolOutput::text(format!(
                    "wrote {}",
                    input
                        .input
                        .get("path")
                        .and_then(|v| v.as_str())
                        .unwrap_or("?")
                ))
            }
        }

        fn queue_request(name: &str, path: &str) -> ToolCallRequest {
            ToolCallRequest {
                call_id: name.to_string(),
                tool_name: "write".to_string(),
                arguments: serde_json::json!({"path": path, "content": "x"}),
                permission: synthia_permission::Permission::AutoApprove,
                tool_id: None,
            }
        }

        fn queue_context(root: PathBuf) -> ExecutionContext {
            ExecutionContext {
                session_id: "s1".to_string(),
                workspace_root: root,
                caller_agent: "agent-1".to_string(),
            }
        }

        #[tokio::test]
        async fn adapter_serializes_same_filepath() {
            let dir = tempfile::tempdir().unwrap();
            let file = dir.path().join("same.txt");
            std::fs::write(&file, "").unwrap();
            let current = Arc::new(AtomicUsize::new(0));
            let max_concurrent = Arc::new(AtomicUsize::new(0));
            let tool = Arc::new(SlowFileTool {
                name: "write",
                current: current.clone(),
                max_concurrent: max_concurrent.clone(),
            });
            let queue = FileMutationQueue::new();
            let adapter = ToolAdapter::with_file_queue(tool, queue);
            let adapter_clone = adapter.clone();
            let req1 = queue_request("c1", file.to_str().unwrap());
            let req2 = queue_request("c2", file.to_str().unwrap());
            let ctx = queue_context(dir.path().to_path_buf());
            let h1 = tokio::spawn({
                let ctx = ctx.clone();
                async move {
                    adapter
                        .execute(
                            &req1,
                            &ctx,
                            &SandboxAttempt::None,
                            CancellationToken::new(),
                        )
                        .await
                }
            });
            let h2 = tokio::spawn({
                let ctx = ctx.clone();
                async move {
                    adapter_clone
                        .execute(
                            &req2,
                            &ctx,
                            &SandboxAttempt::None,
                            CancellationToken::new(),
                        )
                        .await
                }
            });
            let (r1, r2) = tokio::join!(h1, h2);
            assert!(r1.unwrap().is_ok());
            assert!(r2.unwrap().is_ok());
            assert_eq!(
                max_concurrent.load(Ordering::SeqCst),
                1,
                "concurrent writes to the same file should be serialized"
            );
        }

        #[tokio::test]
        async fn adapter_parallel_different_filepaths() {
            let dir = tempfile::tempdir().unwrap();
            let file_a = dir.path().join("a.txt");
            let file_b = dir.path().join("b.txt");
            std::fs::write(&file_a, "").unwrap();
            std::fs::write(&file_b, "").unwrap();
            let current = Arc::new(AtomicUsize::new(0));
            let max_concurrent = Arc::new(AtomicUsize::new(0));
            let tool = Arc::new(SlowFileTool {
                name: "write",
                current: current.clone(),
                max_concurrent: max_concurrent.clone(),
            });
            let queue = FileMutationQueue::new();
            let adapter = ToolAdapter::with_file_queue(tool, queue);
            let adapter_clone = adapter.clone();
            let req1 = queue_request("c1", file_a.to_str().unwrap());
            let req2 = queue_request("c2", file_b.to_str().unwrap());
            let ctx = queue_context(dir.path().to_path_buf());
            let h1 = tokio::spawn({
                let ctx = ctx.clone();
                async move {
                    adapter
                        .execute(
                            &req1,
                            &ctx,
                            &SandboxAttempt::None,
                            CancellationToken::new(),
                        )
                        .await
                }
            });
            let h2 = tokio::spawn({
                let ctx = ctx.clone();
                async move {
                    adapter_clone
                        .execute(
                            &req2,
                            &ctx,
                            &SandboxAttempt::None,
                            CancellationToken::new(),
                        )
                        .await
                }
            });
            let (r1, r2) = tokio::join!(h1, h2);
            assert!(r1.unwrap().is_ok());
            assert!(r2.unwrap().is_ok());
            assert_eq!(
                max_concurrent.load(Ordering::SeqCst),
                2,
                "writes to different files should run in parallel"
            );
        }

        #[tokio::test]
        async fn adapter_without_queue_runs_parallel() {
            let dir = tempfile::tempdir().unwrap();
            let file = dir.path().join("noq.txt");
            std::fs::write(&file, "").unwrap();
            let current = Arc::new(AtomicUsize::new(0));
            let max_concurrent = Arc::new(AtomicUsize::new(0));
            let tool = Arc::new(SlowFileTool {
                name: "write",
                current: current.clone(),
                max_concurrent: max_concurrent.clone(),
            });
            let adapter = ToolAdapter::new(tool);
            let adapter_clone = adapter.clone();
            let req1 = queue_request("c1", file.to_str().unwrap());
            let req2 = queue_request("c2", file.to_str().unwrap());
            let ctx = queue_context(dir.path().to_path_buf());
            let h1 = tokio::spawn({
                let ctx = ctx.clone();
                async move {
                    adapter
                        .execute(
                            &req1,
                            &ctx,
                            &SandboxAttempt::None,
                            CancellationToken::new(),
                        )
                        .await
                }
            });
            let h2 = tokio::spawn({
                let ctx = ctx.clone();
                async move {
                    adapter_clone
                        .execute(
                            &req2,
                            &ctx,
                            &SandboxAttempt::None,
                            CancellationToken::new(),
                        )
                        .await
                }
            });
            let (r1, r2) = tokio::join!(h1, h2);
            assert!(r1.unwrap().is_ok());
            assert!(r2.unwrap().is_ok());
            assert_eq!(
                max_concurrent.load(Ordering::SeqCst),
                2,
                "without file queue, concurrent writes should NOT be serialized"
            );
        }
    }

    mod tool_id_tests {
        use synthia_tool_materialization::ToolId;

        use super::*;

        #[test]
        fn tool_call_request_construct_with_tool_id() {
            let id = ToolId::new();
            let req = ToolCallRequest {
                call_id: "c1".to_string(),
                tool_name: "echo".to_string(),
                arguments: serde_json::json!({}),
                permission: synthia_permission::Permission::AutoApprove,
                tool_id: Some(id),
            };
            assert_eq!(req.tool_id, Some(id));
        }

        #[test]
        fn tool_call_request_construct_without_tool_id() {
            let req = ToolCallRequest {
                call_id: "c1".to_string(),
                tool_name: "echo".to_string(),
                arguments: serde_json::json!({}),
                permission: synthia_permission::Permission::AutoApprove,
                tool_id: None,
            };
            assert!(req.tool_id.is_none());
        }

        #[test]
        fn tool_call_result_construct_with_tool_id() {
            let id = ToolId::new();
            let res = ToolCallResult {
                call_id: "c1".to_string(),
                tool_name: "echo".to_string(),
                outcome: serde_json::json!("ok"),
                is_error: false,
                tool_id: Some(id),
            };
            assert_eq!(res.tool_id, Some(id));
        }

        #[test]
        fn tool_call_request_serde_roundtrip_with_tool_id() {
            let id = ToolId::new();
            let req = ToolCallRequest {
                call_id: "c1".to_string(),
                tool_name: "echo".to_string(),
                arguments: serde_json::json!({"key": "value"}),
                permission: synthia_permission::Permission::AutoApprove,
                tool_id: Some(id),
            };
            let json = serde_json::to_string(&req).unwrap();
            let parsed: ToolCallRequest = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed.call_id, "c1");
            assert_eq!(parsed.tool_id, Some(id));
        }

        #[test]
        fn tool_call_request_serde_roundtrip_without_tool_id() {
            let req = ToolCallRequest {
                call_id: "c1".to_string(),
                tool_name: "echo".to_string(),
                arguments: serde_json::json!({}),
                permission: synthia_permission::Permission::AutoApprove,
                tool_id: None,
            };
            let json = serde_json::to_string(&req).unwrap();
            assert!(!json.contains("tool_id"));
            let parsed: ToolCallRequest = serde_json::from_str(&json).unwrap();
            assert!(parsed.tool_id.is_none());
        }

        #[test]
        fn tool_call_result_serde_roundtrip_with_tool_id() {
            let id = ToolId::new();
            let res = ToolCallResult {
                call_id: "c1".to_string(),
                tool_name: "echo".to_string(),
                outcome: serde_json::json!("ok"),
                is_error: false,
                tool_id: Some(id),
            };
            let json = serde_json::to_string(&res).unwrap();
            let parsed: ToolCallResult = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed.tool_id, Some(id));
        }

        #[test]
        fn tool_call_result_serde_roundtrip_without_tool_id() {
            let res = ToolCallResult {
                call_id: "c1".to_string(),
                tool_name: "echo".to_string(),
                outcome: serde_json::json!("ok"),
                is_error: false,
                tool_id: None,
            };
            let json = serde_json::to_string(&res).unwrap();
            assert!(!json.contains("tool_id"));
            let parsed: ToolCallResult = serde_json::from_str(&json).unwrap();
            assert!(parsed.tool_id.is_none());
        }
    }
}

#[cfg(test)]
mod tests;
