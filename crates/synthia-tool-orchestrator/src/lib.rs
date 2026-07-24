use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, RwLock as StdRwLock},
    time::Duration,
};

use async_trait::async_trait;
use dashmap::DashMap;
use futures::stream::{self, StreamExt};
use serde::{Deserialize, Serialize};

mod edit_conflict;
pub use edit_conflict::{
    ConflictInfo,
    FileSnapshot,
    check_conflict,
    record_read,
};

/// Bookkeeping for a single in-flight tool call.
///
/// Stored as the value of `active_calls` so that
/// [`DefaultToolOrchestrator::fail_interrupted_tools`] can recover the
/// `tool_name` without an auxiliary map.
#[derive(Clone)]
struct ActiveCall {
    tool_name: String,
    token: CancellationToken,
}

/// Removes a call ID from the active-calls map when dropped.
struct ActiveCallGuard {
    map: Arc<DashMap<String, ActiveCall>>,
    call_id: String,
}

impl Drop for ActiveCallGuard {
    fn drop(&mut self) {
        self.map.remove(&self.call_id);
    }
}
use synthia_permission::{
    ApprovalOutcome,
    ApprovalPolicy,
    ApprovalService,
    Permission,
    PermissionRequest,
};
use synthia_sandbox::{SandboxAttempt, SandboxManager, SandboxPolicy};
use tokio::sync::{Mutex, OwnedMutexGuard};
use tokio_util::sync::CancellationToken;

/// A request to invoke a single tool call within an orchestrated execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallRequest {
    pub call_id: String,
    pub tool_name: String,
    pub arguments: serde_json::Value,
    /// Effective permission level that governs whether approval is required.
    pub permission: Permission,
}

/// The result of a completed tool call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallResult {
    pub call_id: String,
    pub tool_name: String,
    pub outcome: serde_json::Value,
    pub is_error: bool,
}

/// Runtime context attached to a tool call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionContext {
    pub session_id: String,
    pub workspace_root: std::path::PathBuf,
    pub caller_agent: String,
}

/// Lifecycle events emitted by a [`ToolOrchestrator`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ToolOrchestratorEvent {
    Started {
        call_id: String,
        tool_name: String,
    },
    Completed {
        call_id: String,
        tool_name: String,
        result: ToolCallResult,
    },
    Failed {
        call_id: String,
        tool_name: String,
        error: String,
    },
    Cancelled {
        call_id: String,
        tool_name: String,
    },
    /// A file-mutating tool emitted a progress event (e.g. a patch hunk was
    /// applied).
    FileChange {
        call_id: String,
        tool_name: String,
        event: synthia_tool::FileChangeEvent,
    },
    /// Edit conflict detected: file was modified since agent read it.
    EditConflict {
        call_id: String,
        tool_name: String,
        path: std::path::PathBuf,
        conflict: ConflictInfo,
    },
}

/// Errors that can be returned by a [`ToolOrchestrator`].
#[derive(Debug, thiserror::Error, Clone, Serialize, Deserialize)]
pub enum ToolOrchestratorError {
    #[error("tool call {call_id} error: {message}")]
    Generic { call_id: String, message: String },
    #[error("tool call {call_id} was cancelled")]
    Cancelled { call_id: String },
    #[error("tool call {call_id} was denied")]
    Denied { call_id: String },
    #[error("tool call {call_id} sandbox error: {message}")]
    Sandbox { call_id: String, message: String },
    #[error("tool call {call_id} tool not found: {tool_name}")]
    NotFound { call_id: String, tool_name: String },
    #[error("tool call {call_id} edit conflict on {path}")]
    EditConflict {
        call_id: String,
        path: std::path::PathBuf,
        original_content_hash: u64,
        current_content_hash: u64,
    },
}

impl ToolOrchestratorError {
    fn cancelled(call_id: impl Into<String>) -> Self {
        Self::Cancelled {
            call_id: call_id.into(),
        }
    }

    fn denied(call_id: impl Into<String>) -> Self {
        Self::Denied {
            call_id: call_id.into(),
        }
    }

    fn generic(call_id: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Generic {
            call_id: call_id.into(),
            message: message.into(),
        }
    }
}

/// Errors that can be returned by an [`ExecutableTool`].
#[derive(Debug, thiserror::Error, Clone)]
pub enum ToolExecutionError {
    #[error("transient error: {0}")]
    Transient(String),
    #[error("permanent error: {0}")]
    Permanent(String),
    #[error("cancelled")]
    Cancelled,
}

impl ToolExecutionError {
    /// Return `true` if the error is transient and the call should be retried.
    pub fn is_transient(&self) -> bool {
        matches!(self, Self::Transient(_))
    }
}

/// A tool that can be executed by the orchestrator.
///
/// Implementations are responsible for the actual work of a tool (file I/O,
/// shell execution, etc.). The orchestrator wraps each call with approval,
/// sandbox selection, retry, and lifecycle events.
#[async_trait]
pub trait ExecutableTool: Send + Sync {
    fn name(&self) -> &str;

    /// Whether this tool can be safely invoked concurrently with other
    /// invocations of itself.
    fn is_concurrency_safe(&self) -> bool {
        false
    }

    /// Execute the tool.
    ///
    /// `sandbox_attempt` is the selected sandbox profile for this call. Tools
    /// that spawn subprocesses should apply it via [`SandboxAttempt::wrap`];
    /// pure tools can ignore it.
    async fn execute(
        &self,
        request: &ToolCallRequest,
        context: &ExecutionContext,
        sandbox_attempt: &SandboxAttempt,
        cancellation_token: CancellationToken,
    ) -> Result<ToolCallResult, ToolExecutionError>;

    /// Execute the tool with a callback for file-mutation progress events.
    ///
    /// The default implementation ignores the callback and delegates to
    /// [`execute`](Self::execute). Tools that produce per-hunk progress
    /// events (e.g. `apply_patch`) override this to forward events to the
    /// orchestrator's event stream.
    async fn execute_with_events(
        &self,
        request: &ToolCallRequest,
        context: &ExecutionContext,
        sandbox_attempt: &SandboxAttempt,
        cancellation_token: CancellationToken,
        on_event: Option<
            Box<dyn Fn(synthia_tool::FileChangeEvent) + Send + Sync>,
        >,
    ) -> Result<ToolCallResult, ToolExecutionError> {
        let _ = on_event;
        self.execute(request, context, sandbox_attempt, cancellation_token)
            .await
    }
}

/// Resolves a tool name to an [`ExecutableTool`] instance.
#[async_trait]
pub trait ToolResolver: Send + Sync {
    fn resolve(&self, name: &str) -> Option<Arc<dyn ExecutableTool>>;
}

/// A simple in-memory resolver backed by a `HashMap`.
#[derive(Clone, Default)]
pub struct HashMapResolver {
    tools: Arc<HashMap<String, Arc<dyn ExecutableTool>>>,
}

impl HashMapResolver {
    /// Create a new resolver from a map of tool names to tools.
    pub fn new(tools: HashMap<String, Arc<dyn ExecutableTool>>) -> Self {
        Self {
            tools: Arc::new(tools),
        }
    }

    /// Consume the resolver and return the underlying tool map.
    pub fn into_tools(self) -> HashMap<String, Arc<dyn ExecutableTool>> {
        Arc::try_unwrap(self.tools).unwrap_or_else(|arc| (*arc).clone())
    }
}

#[async_trait]
impl ToolResolver for HashMapResolver {
    fn resolve(&self, name: &str) -> Option<Arc<dyn ExecutableTool>> {
        self.tools.get(name).cloned()
    }
}

/// A resolver that supports runtime registration of tools.
///
/// Useful for dynamically discovered tools (e.g. from MCP servers) that must
/// be added to the orchestrator after construction.
#[derive(Clone, Default)]
pub struct DynamicResolver {
    tools: Arc<StdRwLock<HashMap<String, Arc<dyn ExecutableTool>>>>,
}

impl DynamicResolver {
    /// Create an empty resolver.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a resolver pre-populated with the given tools.
    pub fn with_tools(tools: HashMap<String, Arc<dyn ExecutableTool>>) -> Self {
        Self {
            tools: Arc::new(StdRwLock::new(tools)),
        }
    }

    /// Register a tool at runtime.
    pub fn register(
        &self,
        name: impl Into<String>,
        tool: Arc<dyn ExecutableTool>,
    ) {
        self.tools
            .write()
            .expect("DynamicResolver RwLock poisoned")
            .insert(name.into(), tool);
    }

    /// Remove a previously registered tool.
    pub fn unregister(&self, name: &str) -> bool {
        self.tools
            .write()
            .expect("DynamicResolver RwLock poisoned")
            .remove(name)
            .is_some()
    }

    /// Check whether a tool is currently registered.
    pub fn contains(&self, name: &str) -> bool {
        self.tools
            .read()
            .expect("DynamicResolver RwLock poisoned")
            .contains_key(name)
    }
}

#[async_trait]
impl ToolResolver for DynamicResolver {
    fn resolve(&self, name: &str) -> Option<Arc<dyn ExecutableTool>> {
        self.tools
            .read()
            .expect("DynamicResolver RwLock poisoned")
            .get(name)
            .cloned()
    }
}

/// Retry policy for individual tool calls.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetryPolicy {
    pub max_attempts: u32,
    pub base_delay_ms: u64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 1,
            base_delay_ms: 0,
        }
    }
}

/// Concurrency policy for batch execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConcurrencyPolicy {
    pub max_concurrent: usize,
}

impl Default for ConcurrencyPolicy {
    fn default() -> Self {
        Self { max_concurrent: 5 }
    }
}

/// Orchestrates the execution of tool calls.
#[async_trait]
pub trait ToolOrchestrator: Send + Sync {
    /// Execute a single tool call and return its result.
    async fn execute(
        &self,
        request: ToolCallRequest,
        context: ExecutionContext,
        cancellation_token: CancellationToken,
    ) -> Result<ToolCallResult, ToolOrchestratorError>;

    /// Execute multiple tool calls in parallel.
    async fn execute_batch(
        &self,
        requests: Vec<ToolCallRequest>,
        context: ExecutionContext,
        cancellation_token: CancellationToken,
    ) -> Result<Vec<ToolCallResult>, ToolOrchestratorError>;

    /// Subscribe to lifecycle events emitted by this orchestrator.
    fn event_stream(
        &self,
    ) -> tokio::sync::broadcast::Receiver<ToolOrchestratorEvent>;

    /// Cancel an in-flight tool call identified by `call_id`.
    async fn cancel(&self, call_id: &str) -> Result<(), ToolOrchestratorError>;

    /// Best-effort fail all currently active tool calls.
    ///
    /// Invoked when the agent loop is interrupted (cancellation or
    /// steering). For each active call the orchestrator cancels its
    /// `CancellationToken`, removes it from the active set, and emits a
    /// [`ToolOrchestratorEvent::Failed`] with a descriptive error so
    /// downstream observers (event log, telemetry) see the
    /// interruption.
    ///
    /// Returns the number of tool calls that were actually interrupted
    /// by this call. Calls that complete concurrently while this method
    /// runs will remove themselves via the `ActiveCallGuard` RAII drop
    /// and are skipped.
    ///
    /// The default implementation is a no-op that returns `0`; it
    /// exists so that test stubs and alternative implementations do not
    /// need to override this method unless they track active calls.
    fn fail_interrupted_tools(&self) -> usize {
        0
    }
}

/// Default implementation of [`ToolOrchestrator`].
#[derive(Clone)]
pub struct DefaultToolOrchestrator {
    tool_resolver: Arc<dyn ToolResolver>,
    approval_service: Arc<dyn ApprovalService>,
    sandbox_manager: Arc<dyn SandboxManager>,
    retry_policy: RetryPolicy,
    concurrency_policy: ConcurrencyPolicy,
    /// Session-level sandbox policy applied to every tool call unless a
    /// per-request override is added in the future.
    sandbox_policy: SandboxPolicy,
    event_sender: tokio::sync::broadcast::Sender<ToolOrchestratorEvent>,
    /// Active call IDs to the [`ActiveCall`] (tool_name + cancellation
    /// token) that controls them.
    active_calls: Arc<DashMap<String, ActiveCall>>,
    /// Per-tool serialization locks for tools that are not concurrency-safe.
    per_tool_locks: Arc<DashMap<String, Arc<Mutex<()>>>>,
    /// Snapshot store for edit conflict detection.
    snapshot_store: Arc<tokio::sync::RwLock<HashMap<PathBuf, FileSnapshot>>>,
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
        }
    }

    fn emit(&self, event: ToolOrchestratorEvent) {
        let _ = self.event_sender.send(event);
    }

    async fn acquire_tool_lock(
        &self,
        tool_name: &str,
    ) -> Option<OwnedMutexGuard<()>> {
        let lock = self
            .per_tool_locks
            .entry(tool_name.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone();
        Some(lock.lock_owned().await)
    }

    async fn check_edit_conflict(
        &self,
        path: &PathBuf,
    ) -> Option<ConflictInfo> {
        check_conflict(path, &self.snapshot_store).await
    }

    async fn record_file_read(&self, path: &PathBuf, content: &[u8]) {
        record_read(path, content, &self.snapshot_store).await;
    }
}

#[async_trait]
impl ToolOrchestrator for DefaultToolOrchestrator {
    async fn execute(
        &self,
        request: ToolCallRequest,
        context: ExecutionContext,
        cancellation_token: CancellationToken,
    ) -> Result<ToolCallResult, ToolOrchestratorError> {
        if cancellation_token.is_cancelled() {
            self.emit(ToolOrchestratorEvent::Cancelled {
                call_id: request.call_id.clone(),
                tool_name: request.tool_name.clone(),
            });
            return Err(ToolOrchestratorError::cancelled(&request.call_id));
        }

        let call_id = request.call_id.clone();
        let tool_name = request.tool_name.clone();

        // Use a child token so that `cancel(call_id)` can cancel just this
        // call while still honouring the caller-wide cancellation token.
        let call_token = cancellation_token.child_token();
        self.active_calls.insert(
            call_id.clone(),
            ActiveCall {
                tool_name: tool_name.clone(),
                token: call_token.clone(),
            },
        );
        let _active_guard = ActiveCallGuard {
            map: self.active_calls.clone(),
            call_id: call_id.clone(),
        };

        let tool = match self.tool_resolver.resolve(&tool_name) {
            Some(tool) => tool,
            None => {
                let error = format!("tool '{}' not found", tool_name);
                self.emit(ToolOrchestratorEvent::Failed {
                    call_id: call_id.clone(),
                    tool_name: tool_name.clone(),
                    error: error.clone(),
                });
                return Err(ToolOrchestratorError::NotFound {
                    call_id: call_id.clone(),
                    tool_name: tool_name.clone(),
                });
            }
        };

        self.emit(ToolOrchestratorEvent::Started {
            call_id: call_id.clone(),
            tool_name: tool_name.clone(),
        });

        // Approval check driven by the effective permission level.
        let approval_outcome = match &request.permission {
            Permission::AutoApprove => ApprovalOutcome::Approve,
            Permission::RequireConfirm | Permission::RequireExplicit => {
                let permission_request =
                    synthia_permission::PermissionRequest::new(
                        tool_name.clone(),
                        request.arguments.clone(),
                        true,
                    );
                let permission_future =
                    self.approval_service.ask(permission_request);
                match permission_future
                    .await_with_cancellation(&call_token)
                    .await
                {
                    Ok(outcome) => outcome.outcome.into(),
                    Err(_) => ApprovalOutcome::Deny,
                }
            }
            Permission::Block | Permission::Deny { .. } => {
                ApprovalOutcome::Deny
            }
        };

        if approval_outcome == ApprovalOutcome::Deny {
            self.emit(ToolOrchestratorEvent::Failed {
                call_id: call_id.clone(),
                tool_name: tool_name.clone(),
                error: "denied".to_string(),
            });
            return Err(ToolOrchestratorError::denied(&call_id));
        }

        // Sandbox selection. Platform availability is checked here; the caller
        // applies the resulting attempt inside the tool if it spawns commands.
        let mut sandbox_attempt = self
            .sandbox_manager
            .select(
                self.sandbox_policy.clone(),
                &tool_name,
                std::env::consts::OS,
            )
            .await
            .map_err(|e| ToolOrchestratorError::Sandbox {
                call_id: call_id.clone(),
                message: e.to_string(),
            })?;

        if matches!(sandbox_attempt, SandboxAttempt::Unavailable) {
            match self.sandbox_policy.on_unavailable() {
                synthia_sandbox::OnUnavailable::Deny => {
                    self.emit(ToolOrchestratorEvent::Failed {
                        call_id: call_id.clone(),
                        tool_name: tool_name.clone(),
                        error: "sandbox unavailable".to_string(),
                    });
                    return Err(ToolOrchestratorError::Sandbox {
                        call_id: call_id.clone(),
                        message: "sandbox unavailable".to_string(),
                    });
                }
                synthia_sandbox::OnUnavailable::Prompt => {
                    // Prompt policy explicitly allows falling back to
                    // unsandboxed execution. Emit a warning event so the
                    // decision is observable (P6/P9) and continue with no
                    // sandboxing.
                    self.emit(ToolOrchestratorEvent::Failed {
                        call_id: call_id.clone(),
                        tool_name: tool_name.clone(),
                        error: "sandbox unavailable; continuing unsandboxed per policy".to_string(),
                    });
                    sandbox_attempt = SandboxAttempt::None;
                }
            }
        }

        // Serialize non-concurrency-safe tools.
        let _tool_lock = if tool.is_concurrency_safe() {
            None
        } else {
            self.acquire_tool_lock(&tool_name).await
        };

        let event_sender = self.event_sender.clone();
        let event_call_id = call_id.clone();
        let event_tool_name = tool_name.clone();
        let on_event_arc: Option<
            Arc<dyn Fn(synthia_tool::FileChangeEvent) + Send + Sync>,
        > = Some(Arc::new(move |event| {
            let _ = event_sender.send(ToolOrchestratorEvent::FileChange {
                call_id: event_call_id.clone(),
                tool_name: event_tool_name.clone(),
                event,
            });
        }));

        let mut attempt = 0u32;
        loop {
            if call_token.is_cancelled() {
                self.emit(ToolOrchestratorEvent::Cancelled {
                    call_id: call_id.clone(),
                    tool_name: tool_name.clone(),
                });
                return Err(ToolOrchestratorError::cancelled(&call_id));
            }

            if is_write_tool(&tool_name)
                && let Some(path) = extract_file_path(
                    &tool_name,
                    &request.arguments,
                    &context.workspace_root,
                )
                && let Some(conflict) = self.check_edit_conflict(&path).await
            {
                let original_hash = conflict.agent_snapshot.content_hash;
                let current_hash = conflict.current_hash;
                self.emit(ToolOrchestratorEvent::EditConflict {
                    call_id: call_id.clone(),
                    tool_name: tool_name.clone(),
                    path: path.clone(),
                    conflict,
                });
                return Err(ToolOrchestratorError::EditConflict {
                    call_id: call_id.clone(),
                    path,
                    original_content_hash: original_hash,
                    current_content_hash: current_hash,
                });
            }

            let on_event: Option<
                Box<dyn Fn(synthia_tool::FileChangeEvent) + Send + Sync>,
            > = on_event_arc.as_ref().map(|arc| {
                let arc = arc.clone();
                Box::new(move |event| arc(event))
                    as Box<dyn Fn(_) + Send + Sync>
            });

            attempt += 1;
            match tool
                .execute_with_events(
                    &request,
                    &context,
                    &sandbox_attempt,
                    call_token.clone(),
                    on_event,
                )
                .await
            {
                Ok(result) => {
                    if is_read_tool(&tool_name)
                        && let Some(path) = extract_file_path(
                            &tool_name,
                            &request.arguments,
                            &context.workspace_root,
                        )
                        && let Some(text) =
                            result.outcome.get("text").and_then(|v| v.as_str())
                    {
                        self.record_file_read(&path, text.as_bytes()).await;
                    }
                    self.emit(ToolOrchestratorEvent::Completed {
                        call_id: call_id.clone(),
                        tool_name: tool_name.clone(),
                        result: result.clone(),
                    });
                    return Ok(result);
                }
                Err(ToolExecutionError::Cancelled) => {
                    self.emit(ToolOrchestratorEvent::Cancelled {
                        call_id: call_id.clone(),
                        tool_name: tool_name.clone(),
                    });
                    return Err(ToolOrchestratorError::cancelled(&call_id));
                }
                Err(err) => {
                    let is_transient = err.is_transient();
                    let error_message = err.to_string();
                    if attempt >= self.retry_policy.max_attempts
                        || !is_transient
                    {
                        self.emit(ToolOrchestratorEvent::Failed {
                            call_id: call_id.clone(),
                            tool_name: tool_name.clone(),
                            error: error_message.clone(),
                        });
                        return Err(ToolOrchestratorError::generic(
                            &call_id,
                            error_message,
                        ));
                    }

                    let delay = Duration::from_millis(
                        self.retry_policy.base_delay_ms
                            * 2_u64.pow(attempt.saturating_sub(1)),
                    );
                    tokio::select! {
                        _ = tokio::time::sleep(delay) => {}
                        _ = call_token.cancelled() => {
                            self.emit(ToolOrchestratorEvent::Cancelled {
                                call_id: call_id.clone(),
                                tool_name: tool_name.clone(),
                            });
                            return Err(ToolOrchestratorError::cancelled(&call_id));
                        }
                    }
                }
            }
        }
    }

    async fn execute_batch(
        &self,
        requests: Vec<ToolCallRequest>,
        context: ExecutionContext,
        cancellation_token: CancellationToken,
    ) -> Result<Vec<ToolCallResult>, ToolOrchestratorError> {
        let futures = requests.into_iter().map(|request| {
            let ctx = context.clone();
            let cancel = cancellation_token.clone();
            async move { self.execute(request, ctx, cancel).await }
        });

        let results: Vec<Result<ToolCallResult, ToolOrchestratorError>> =
            stream::iter(futures)
                .buffer_unordered(self.concurrency_policy.max_concurrent)
                .collect()
                .await;

        results.into_iter().collect()
    }

    fn event_stream(
        &self,
    ) -> tokio::sync::broadcast::Receiver<ToolOrchestratorEvent> {
        self.event_sender.subscribe()
    }

    async fn cancel(&self, call_id: &str) -> Result<(), ToolOrchestratorError> {
        let tool_name = self
            .active_calls
            .get(call_id)
            .map(|e| e.value().tool_name.clone())
            .unwrap_or_default();
        if let Some(entry) = self.active_calls.get(call_id) {
            entry.value().token.cancel();
        }
        self.emit(ToolOrchestratorEvent::Cancelled {
            call_id: call_id.to_string(),
            tool_name,
        });
        Ok(())
    }

    fn fail_interrupted_tools(&self) -> usize {
        let mut count = 0usize;
        // Snapshot (call_id, tool_name, token) first so we do not hold
        // the DashMap iterator across mutation. DashMap iterators
        // borrow the underlying shard shards and cannot outlive
        // removals.
        let entries: Vec<(String, String, CancellationToken)> = self
            .active_calls
            .iter()
            .map(|entry| {
                (
                    entry.key().clone(),
                    entry.value().tool_name.clone(),
                    entry.value().token.clone(),
                )
            })
            .collect();

        for (call_id, tool_name, token) in entries {
            // Cancel first to signal the tool. Cancelling a token that
            // has already been cancelled (or whose call has completed)
            // is a no-op.
            token.cancel();
            // Remove from active_calls. If `ActiveCallGuard::drop`
            // already removed the entry (concurrent completion), skip
            // the emit so we do not double-report.
            if self.active_calls.remove(&call_id).is_some() {
                count += 1;
                self.emit(ToolOrchestratorEvent::Failed {
                    call_id: call_id.clone(),
                    tool_name,
                    error: "Tool execution interrupted".to_string(),
                });
            }
        }
        count
    }
}

fn extract_file_path(
    tool_name: &str,
    arguments: &serde_json::Value,
    workspace_root: &std::path::Path,
) -> Option<std::path::PathBuf> {
    match tool_name {
        "write" | "multi_edit" => {}
        _ => return None,
    }
    let path_str = arguments.get("path").and_then(|v| v.as_str())?;
    let p = std::path::PathBuf::from(path_str);
    Some(if p.is_absolute() {
        p
    } else {
        workspace_root.join(p)
    })
}

fn is_read_tool(tool_name: &str) -> bool {
    tool_name == "read"
}

fn is_write_tool(tool_name: &str) -> bool {
    matches!(tool_name, "write" | "multi_edit")
}

/// Adapter that wraps a [`synthia_tool::Tool`] so it can be used as an
/// [`ExecutableTool`] inside the orchestrator.
pub mod adapter {
    use std::sync::Arc;

    use async_trait::async_trait;
    use synthia_tool_exec_base::FileMutationQueue;
    use tokio_util::sync::CancellationToken;

    use crate::{
        ExecutableTool,
        ExecutionContext,
        ToolCallRequest,
        ToolCallResult,
        ToolExecutionError,
        extract_file_path,
    };

    /// Wraps a `synthia_tool::Tool` to expose the orchestrator's
    /// `ExecutableTool` interface.
    ///
    /// When constructed with [`ToolAdapter::with_file_queue`], file-mutating
    /// tools (`write`, `multi_edit`) acquire a per-filepath mutex before
    /// the underlying tool is invoked, serializing concurrent writes to
    /// the same realpath while allowing parallel writes to different files.
    #[derive(Clone)]
    pub struct ToolAdapter {
        tool: Arc<dyn synthia_tool::Tool>,
        file_queue: Option<FileMutationQueue>,
    }

    impl ToolAdapter {
        pub fn new(tool: Arc<dyn synthia_tool::Tool>) -> Self {
            Self {
                tool,
                file_queue: None,
            }
        }

        pub fn with_file_queue(
            tool: Arc<dyn synthia_tool::Tool>,
            file_queue: FileMutationQueue,
        ) -> Self {
            Self {
                tool,
                file_queue: Some(file_queue),
            }
        }
    }

    #[async_trait]
    impl ExecutableTool for ToolAdapter {
        fn name(&self) -> &str {
            self.tool.name()
        }

        fn is_concurrency_safe(&self) -> bool {
            self.tool.is_concurrency_safe()
        }

        async fn execute(
            &self,
            request: &ToolCallRequest,
            context: &ExecutionContext,
            sandbox_attempt: &synthia_sandbox::SandboxAttempt,
            cancellation_token: CancellationToken,
        ) -> Result<ToolCallResult, ToolExecutionError> {
            if request.tool_name != self.tool.name() {
                return Err(ToolExecutionError::Permanent(format!(
                    "tool name mismatch: expected '{}', got '{}'",
                    self.tool.name(),
                    request.tool_name
                )));
            }

            validate_tool_input(&request.arguments, &self.tool.parameters())?;

            // Acquire per-filepath lock for file-mutating tools so that
            // concurrent writes to the same realpath are serialized.
            let _file_guard = if let Some(queue) = &self.file_queue
                && let Some(path) = extract_file_path(
                    &request.tool_name,
                    &request.arguments,
                    &context.workspace_root,
                ) {
                Some(queue.acquire(path).await)
            } else {
                None
            };

            let tool_context = synthia_tool::types::ToolExecutionContext {
                session_id: context.session_id.clone(),
                workspace_root: context.workspace_root.clone(),
                caller_agent: context.caller_agent.clone(),
                dispatch_mode: synthia_tool::types::DispatchMode::Fork,
                messages: Vec::new(),
            };

            let input = synthia_tool::ToolInput {
                name: request.tool_name.clone(),
                input: request.arguments.clone(),
                context: tool_context,
            };

            // Route through `call_with_sandbox` so tools that spawn
            // subprocesses (bash) apply the orchestrator-selected
            // `SandboxAttempt`. Pure tools (file I/O, search) keep the
            // default impl which ignores `sandbox_attempt` and delegates
            // to `Tool::call`. Previously this called `self.tool.call`
            // directly, discarding the sandbox — that was the U1
            // single-point-of-failure (bash ran with parent-process
            // privileges regardless of the policy verdict).
            let output = self
                .tool
                .call_with_sandbox(input, sandbox_attempt, &cancellation_token)
                .await;
            let outcome =
                serde_json::to_value(&output.content).map_err(|e| {
                    ToolExecutionError::Permanent(format!(
                        "failed to serialize tool output: {}",
                        e
                    ))
                })?;

            Ok(ToolCallResult {
                call_id: request.call_id.clone(),
                tool_name: request.tool_name.clone(),
                outcome,
                is_error: output.is_error.unwrap_or(false),
            })
        }

        async fn execute_with_events(
            &self,
            request: &ToolCallRequest,
            context: &ExecutionContext,
            sandbox_attempt: &synthia_sandbox::SandboxAttempt,
            cancellation_token: CancellationToken,
            on_event: Option<
                Box<dyn Fn(synthia_tool::FileChangeEvent) + Send + Sync>,
            >,
        ) -> Result<ToolCallResult, ToolExecutionError> {
            if request.tool_name != self.tool.name() {
                return Err(ToolExecutionError::Permanent(format!(
                    "tool name mismatch: expected '{}', got '{}'",
                    self.tool.name(),
                    request.tool_name
                )));
            }

            validate_tool_input(&request.arguments, &self.tool.parameters())?;

            let _file_guard = if let Some(queue) = &self.file_queue
                && let Some(path) = extract_file_path(
                    &request.tool_name,
                    &request.arguments,
                    &context.workspace_root,
                ) {
                Some(queue.acquire(path).await)
            } else {
                None
            };

            let tool_context = synthia_tool::types::ToolExecutionContext {
                session_id: context.session_id.clone(),
                workspace_root: context.workspace_root.clone(),
                caller_agent: context.caller_agent.clone(),
                dispatch_mode: synthia_tool::types::DispatchMode::Fork,
                messages: Vec::new(),
            };

            let input = synthia_tool::ToolInput {
                name: request.tool_name.clone(),
                input: request.arguments.clone(),
                context: tool_context,
            };

            let output = if let Some(on_event) = on_event {
                let callback = std::sync::Arc::new(on_event);
                self.tool
                    .call_with_progress(input, callback, &cancellation_token)
                    .await
            } else {
                self.tool
                    .call_with_sandbox(
                        input,
                        sandbox_attempt,
                        &cancellation_token,
                    )
                    .await
            };

            let outcome =
                serde_json::to_value(&output.content).map_err(|e| {
                    ToolExecutionError::Permanent(format!(
                        "failed to serialize tool output: {}",
                        e
                    ))
                })?;

            Ok(ToolCallResult {
                call_id: request.call_id.clone(),
                tool_name: request.tool_name.clone(),
                outcome,
                is_error: output.is_error.unwrap_or(false),
            })
        }
    }

    /// Validate `arguments` against the tool's `parameters()` JSON schema.
    ///
    /// This is a sanity check, not a full JSON Schema validator: it only
    /// enforces that `arguments` is a JSON object, that every entry in
    /// `required` is present, and that present fields match the declared
    /// `type`. Unknown fields are ignored (serde default). Constraints like
    /// `minimum`, `maximum`, `pattern`, or `minLength` are intentionally
    /// not checked.
    fn validate_tool_input(
        arguments: &serde_json::Value,
        parameters: &serde_json::Value,
    ) -> Result<(), ToolExecutionError> {
        let args_obj = arguments.as_object().ok_or_else(|| {
            ToolExecutionError::Permanent(
                "Invalid input: expected JSON object".to_string(),
            )
        })?;

        if let Some(required) =
            parameters.get("required").and_then(|v| v.as_array())
        {
            for field in required {
                if let Some(field_name) = field.as_str()
                    && !args_obj.contains_key(field_name)
                {
                    return Err(ToolExecutionError::Permanent(format!(
                        "Invalid input: missing required field `{field_name}`"
                    )));
                }
            }
        }

        if let Some(properties) =
            parameters.get("properties").and_then(|v| v.as_object())
        {
            for (field, schema) in properties {
                let Some(value) = args_obj.get(field) else {
                    continue;
                };
                let Some(expected_type) =
                    schema.get("type").and_then(|v| v.as_str())
                else {
                    continue;
                };
                if !type_matches(expected_type, value) {
                    return Err(ToolExecutionError::Permanent(format!(
                        "Invalid input: field `{field}` expected type {expected_type}, got {}",
                        json_type_name(value)
                    )));
                }
            }
        }

        Ok(())
    }

    /// Return `true` if `value` matches the JSON Schema `type` keyword.
    ///
    /// Per the design spec, `integer` and `number` are both satisfied by
    /// any JSON number — a float like `1.5` is accepted for an `integer`
    /// field. This keeps the check a lenient sanity check rather than a
    /// strict schema validator.
    fn type_matches(expected: &str, value: &serde_json::Value) -> bool {
        match expected {
            "string" => value.is_string(),
            "integer" | "number" => value.is_number(),
            "boolean" => value.is_boolean(),
            "array" => value.is_array(),
            "object" => value.is_object(),
            _ => true,
        }
    }

    /// Human-readable name for a JSON value's type, for error messages.
    fn json_type_name(value: &serde_json::Value) -> &'static str {
        match value {
            serde_json::Value::Null => "null",
            serde_json::Value::Bool(_) => "boolean",
            serde_json::Value::Number(_) => "number",
            serde_json::Value::String(_) => "string",
            serde_json::Value::Array(_) => "array",
            serde_json::Value::Object(_) => "object",
        }
    }
}

/// Build a [`HashMapResolver`] containing the default tools provided by
/// `synthia-tool`, wrapped via [`adapter::ToolAdapter`].
pub fn default_tool_resolver() -> HashMapResolver {
    use std::sync::Arc;

    use synthia_tool::builtin::{
        ApplyPatchTool,
        GlobTool,
        GrepTool,
        MultiEditTool,
        ReadTool,
        WebFetchTool,
        WriteTool,
    };

    let mut tools: HashMap<String, Arc<dyn ExecutableTool>> = HashMap::new();
    tools.insert(
        "read".to_string(),
        Arc::new(adapter::ToolAdapter::new(Arc::new(ReadTool::new()))),
    );
    tools.insert(
        "write".to_string(),
        Arc::new(adapter::ToolAdapter::new(Arc::new(WriteTool))),
    );
    tools.insert(
        "glob".to_string(),
        Arc::new(adapter::ToolAdapter::new(Arc::new(GlobTool))),
    );
    tools.insert(
        "grep".to_string(),
        Arc::new(adapter::ToolAdapter::new(Arc::new(GrepTool))),
    );
    tools.insert(
        "multi_edit".to_string(),
        Arc::new(adapter::ToolAdapter::new(Arc::new(MultiEditTool))),
    );
    tools.insert(
        "apply_patch".to_string(),
        Arc::new(adapter::ToolAdapter::new(Arc::new(
            ApplyPatchTool::default(),
        ))),
    );
    tools.insert(
        "web_fetch".to_string(),
        Arc::new(adapter::ToolAdapter::new(Arc::new(WebFetchTool::new()))),
    );
    HashMapResolver::new(tools)
}

/// Build a [`HashMapResolver`] with the default tools, wiring a shared
/// [`FileMutationQueue`] into file-mutating tools (`write`, `multi_edit`)
/// so that concurrent writes to the same realpath are serialized.
///
/// All clones of the returned queue share the same underlying map, so
/// callers can clone the queue before calling this function and use the
/// other clone for inspection or to pass to additional tools.
pub fn default_tool_resolver_with_file_queue(
    file_queue: synthia_tool_exec_base::FileMutationQueue,
) -> HashMapResolver {
    use std::sync::Arc;

    use synthia_tool::builtin::{
        ApplyPatchTool,
        GlobTool,
        GrepTool,
        MultiEditTool,
        ReadTool,
        WebFetchTool,
        WriteTool,
    };

    let mut tools: HashMap<String, Arc<dyn ExecutableTool>> = HashMap::new();
    tools.insert(
        "read".to_string(),
        Arc::new(adapter::ToolAdapter::new(Arc::new(ReadTool::new()))),
    );
    tools.insert(
        "write".to_string(),
        Arc::new(adapter::ToolAdapter::with_file_queue(
            Arc::new(WriteTool),
            file_queue.clone(),
        )),
    );
    tools.insert(
        "glob".to_string(),
        Arc::new(adapter::ToolAdapter::new(Arc::new(GlobTool))),
    );
    tools.insert(
        "grep".to_string(),
        Arc::new(adapter::ToolAdapter::new(Arc::new(GrepTool))),
    );
    tools.insert(
        "multi_edit".to_string(),
        Arc::new(adapter::ToolAdapter::with_file_queue(
            Arc::new(MultiEditTool),
            file_queue.clone(),
        )),
    );
    tools.insert(
        "apply_patch".to_string(),
        Arc::new(adapter::ToolAdapter::new(Arc::new(
            ApplyPatchTool::default(),
        ))),
    );
    tools.insert(
        "web_fetch".to_string(),
        Arc::new(adapter::ToolAdapter::new(Arc::new(WebFetchTool::new()))),
    );
    HashMapResolver::new(tools)
}

#[cfg(test)]
impl DefaultToolOrchestrator {
    fn has_active_call(&self, call_id: &str) -> bool {
        self.active_calls.contains_key(call_id)
    }

    /// Register a synthetic active call for testing `fail_interrupted_tools`.
    ///
    /// Returns the call's `CancellationToken` so tests can assert that it
    /// was cancelled.
    fn register_test_active_call(
        &self,
        call_id: &str,
        tool_name: &str,
    ) -> CancellationToken {
        let token = CancellationToken::new();
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

    struct UnavailableSandboxManager;

    #[async_trait]
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

    struct FailingTool {
        calls: Arc<AtomicUsize>,
        fail_transient_until: usize,
    }

    #[async_trait]
    impl ExecutableTool for FailingTool {
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
            })
        }
    }

    struct SlowTool {
        delay: Duration,
    }

    #[async_trait]
    impl ExecutableTool for SlowTool {
        fn name(&self) -> &str {
            "slow"
        }

        async fn execute(
            &self,
            request: &ToolCallRequest,
            _context: &ExecutionContext,
            _sandbox_attempt: &SandboxAttempt,
            cancellation_token: CancellationToken,
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
            })
        }
    }

    struct RecordingApprovalService {
        calls: AtomicUsize,
    }

    #[async_trait]
    impl ApprovalService for RecordingApprovalService {
        async fn request_approval(
            &self,
            _tool: &str,
            _args: &serde_json::Value,
            _policy: ApprovalPolicy,
            _timeout: Duration,
            _cancel: CancellationToken,
        ) -> Result<ApprovalOutcome, ApprovalError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(ApprovalOutcome::Deny)
        }
        fn ask(
            &self,
            _request: PermissionRequest,
        ) -> synthia_permission::PermissionFuture {
            synthia_permission::PermissionFuture::immediate_denied()
        }
    }

    struct TimeoutApprovalService;

    #[async_trait]
    impl ApprovalService for TimeoutApprovalService {
        async fn request_approval(
            &self,
            _tool: &str,
            _args: &serde_json::Value,
            _policy: ApprovalPolicy,
            _timeout: Duration,
            _cancel: CancellationToken,
        ) -> Result<ApprovalOutcome, ApprovalError> {
            Err(ApprovalError::Timeout)
        }

        fn ask(
            &self,
            _request: PermissionRequest,
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
            permission: Permission::RequireConfirm,
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
                CancellationToken::new(),
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
        request.permission = Permission::AutoApprove;

        let result = orchestrator
            .execute(request, test_context(), CancellationToken::new())
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
                CancellationToken::new(),
            )
            .await;

        assert!(matches!(result, Err(ToolOrchestratorError::Denied { .. })));
    }

    #[tokio::test]
    async fn execute_respects_cancellation_token() {
        let orchestrator = test_orchestrator_with_tool(Arc::new(EchoTool));
        let cancel = CancellationToken::new();
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
                CancellationToken::new(),
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
        request.permission = Permission::AutoApprove;

        let result = orchestrator
            .execute(request, test_context(), CancellationToken::new())
            .await;

        assert!(result.is_ok());
        assert_eq!(calls.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn execute_reports_permanent_error_without_retry() {
        struct PermanentFailTool;

        #[async_trait]
        impl ExecutableTool for PermanentFailTool {
            fn name(&self) -> &str {
                "permanent"
            }

            async fn execute(
                &self,
                request: &ToolCallRequest,
                _context: &ExecutionContext,
                _sandbox_attempt: &SandboxAttempt,
                _cancellation_token: CancellationToken,
            ) -> Result<ToolCallResult, ToolExecutionError> {
                Err(ToolExecutionError::Permanent(format!(
                    "permanent failure for {}",
                    request.call_id
                )))
            }
        }

        let _calls = Arc::new(AtomicUsize::new(0));
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
        request.permission = Permission::AutoApprove;

        let result = orchestrator
            .execute(request, test_context(), CancellationToken::new())
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
        request.permission = Permission::AutoApprove;

        let result = orchestrator
            .execute(request, test_context(), CancellationToken::new())
            .await;

        assert!(matches!(result, Err(ToolOrchestratorError::Sandbox { .. })));
    }

    #[tokio::test]
    async fn execute_batch_runs_multiple_requests() {
        let orchestrator = test_orchestrator_with_tool(Arc::new(EchoTool));
        let requests =
            vec![test_request("a", "echo"), test_request("b", "echo")];
        for req in &requests {
            let mut r = req.clone();
            r.permission = Permission::AutoApprove;
        }
        let requests: Vec<_> = requests
            .into_iter()
            .map(|mut r| {
                r.permission = Permission::AutoApprove;
                r
            })
            .collect();

        let result = orchestrator
            .execute_batch(requests, test_context(), CancellationToken::new())
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
        let cancel = CancellationToken::new();
        let mut request = test_request("slow-1", "slow");
        request.permission = Permission::AutoApprove;

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

        // Expect exactly 3 Failed events with the interrupted error.
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
        // No events should have been emitted.
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn test_fail_interrupted_skips_already_completed_calls() {
        // Simulate the scenario where `ActiveCallGuard::drop` has
        // already removed the entry from `active_calls` BEFORE
        // `fail_interrupted_tools` runs. This is not a true concurrent
        // race; we synchronously remove the entry to model the
        // post-drop state that `fail_interrupted_tools` must skip.
        let orchestrator = test_orchestrator_with_tool(Arc::new(EchoTool));
        let mut rx = orchestrator.event_stream();

        let _token_a = orchestrator.register_test_active_call("call-a", "echo");
        let _token_b = orchestrator.register_test_active_call("call-b", "read");

        // Simulate `ActiveCallGuard::drop` for call-b: the tool finished
        // on its own and removed itself from the active set.
        assert!(orchestrator.active_calls.remove("call-b").is_some());

        let count = orchestrator.fail_interrupted_tools();

        // Only call-a remained; call-b was already gone.
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
        // Verify that `fail_interrupted_tools` emits one
        // `ToolOrchestratorEvent::Failed` per interrupted call, carrying
        // the original `call_id` and `tool_name` so downstream
        // consumers (session JSONL, telemetry) can correlate them.
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
                permission: Permission::RequireConfirm,
            };
            request.permission = Permission::AutoApprove;

            let result = orchestrator
                .execute(request, test_context(), CancellationToken::new())
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
                permission: Permission::RequireConfirm,
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
                    CancellationToken::new(),
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
                    CancellationToken::new(),
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
                    CancellationToken::new(),
                )
                .await
                .expect("execute succeeds");

            assert!(result.is_error);
        }

        /// Mock tool with a configurable JSON schema and invocation tracking,
        /// used to exercise `ToolAdapter` input validation.
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
                permission: Permission::RequireConfirm,
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
                    "properties": {
                        "file_path": {"type": "string"},
                        "offset": {"type": "integer"},
                        "limit": {"type": "integer"}
                    },
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
                    CancellationToken::new(),
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
                    CancellationToken::new(),
                )
                .await;

            assert!(matches!(
                result,
                Err(ToolExecutionError::Permanent(ref m))
                    if m.contains("file_path")
            ));
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
                    CancellationToken::new(),
                )
                .await;

            assert!(matches!(
                result,
                Err(ToolExecutionError::Permanent(ref m))
                    if m.contains("file_path")
            ));
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
                    CancellationToken::new(),
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
            let result = adapter
                .execute(
                    &validation_request(
                        "schema-tool",
                        serde_json::json!({
                            "file_path": "/tmp/x",
                            "unknown_field": "ignored"
                        }),
                    ),
                    &validation_context(),
                    &synthia_sandbox::SandboxAttempt::None,
                    CancellationToken::new(),
                )
                .await
                .expect("execute succeeds");

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
                    CancellationToken::new(),
                )
                .await;

            assert!(matches!(
                result,
                Err(ToolExecutionError::Permanent(ref m))
                    if m.contains("JSON object")
            ));
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

        /// A mock file-mutating tool that records concurrent executions.
        /// It sleeps briefly to widen the window for detecting overlap.
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
                serde_json::json!({
                    "type": "object",
                    "required": ["path"],
                    "properties": {
                        "path": {"type": "string"}
                    }
                })
            }

            fn is_concurrency_safe(&self) -> bool {
                // Return true so the orchestrator's per-tool lock doesn't
                // serialize calls — we want to test the FileMutationQueue.
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
                permission: Permission::AutoApprove,
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

            // Two concurrent calls to the same file — should serialize.
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

            // Should never have had more than 1 concurrent execution.
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

            // Two concurrent calls to different files — should run in parallel.
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

            // Both tools should have been running simultaneously.
            assert_eq!(
                max_concurrent.load(Ordering::SeqCst),
                2,
                "writes to different files should run in parallel"
            );
        }

        #[tokio::test]
        async fn adapter_without_queue_runs_parallel() {
            // When no FileMutationQueue is configured, the adapter should
            // NOT serialize (the tool's is_concurrency_safe=true is honored).
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
            // No file queue — plain adapter.
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

            // Without the queue, both should run concurrently.
            assert_eq!(
                max_concurrent.load(Ordering::SeqCst),
                2,
                "without file queue, concurrent writes should NOT be serialized"
            );
        }
    }
}

#[cfg(test)]
mod tests;
