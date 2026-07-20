use std::sync::Arc;

use async_trait::async_trait;

use crate::{
    ExecutionContext,
    ToolCallRequest,
    ToolCallResult,
    ToolOrchestratorError,
    ToolOrchestratorEvent,
};

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

    /// How the orchestrator should schedule this tool relative to its
    /// peers. Defaults to [`synthia_tool::traits::ExecutionMode::Sequential`]
    /// for backward compatibility (existing `ExecutableTool` impls are
    /// assumed to mutate external state unless they opt-in to parallel).
    fn execution_mode(&self) -> synthia_tool::traits::ExecutionMode {
        synthia_tool::traits::ExecutionMode::Sequential
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
        sandbox_attempt: &synthia_sandbox::SandboxAttempt,
        cancellation_token: tokio_util::sync::CancellationToken,
    ) -> Result<ToolCallResult, crate::ToolExecutionError>;

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
        sandbox_attempt: &synthia_sandbox::SandboxAttempt,
        cancellation_token: tokio_util::sync::CancellationToken,
        on_event: Option<
            Box<dyn Fn(synthia_tool::FileChangeEvent) + Send + Sync>,
        >,
    ) -> Result<ToolCallResult, crate::ToolExecutionError> {
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

/// Orchestrates the execution of tool calls.
#[async_trait]
pub trait ToolOrchestrator: Send + Sync {
    /// Execute a single tool call and return its result.
    async fn execute(
        &self,
        request: ToolCallRequest,
        context: ExecutionContext,
        cancellation_token: tokio_util::sync::CancellationToken,
    ) -> Result<ToolCallResult, ToolOrchestratorError>;

    /// Execute multiple tool calls in parallel.
    async fn execute_batch(
        &self,
        requests: Vec<ToolCallRequest>,
        context: ExecutionContext,
        cancellation_token: tokio_util::sync::CancellationToken,
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
