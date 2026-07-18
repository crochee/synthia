use std::sync::Arc;

use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

use crate::FileChangeEvent;
pub use crate::types::{ToolInput, ToolOutput};

/// Callback used by tools to emit [`FileChangeEvent`] progress events.
pub type FileChangeCallback = Arc<dyn Fn(FileChangeEvent) + Send + Sync>;

/// How the orchestrator should schedule a tool relative to its peers.
///
/// Default is [`ExecutionMode::Parallel`]; tools that mutate external
/// state (filesystem, processes) should override and return
/// [`ExecutionMode::Sequential`] so the orchestrator never races two
/// invocations of the same tool (or two different mutating tools in
/// the same batch).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExecutionMode {
    /// May run concurrently with other `Parallel` tools.
    #[default]
    Parallel,
    /// Must run alone, after every preceding tool call has completed.
    Sequential,
}

#[cfg_attr(
    feature = "unified-registry",
    deprecated(
        since = "0.1.0",
        note = "use the unified Tool trait in synthia_core::tool (feature `unified-registry`)"
    )
)]
#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters(&self) -> serde_json::Value;
    fn requires_permission(&self) -> bool {
        false
    }
    fn is_hidden(&self) -> bool {
        false
    }
    /// Whether this tool can be safely invoked concurrently with other
    /// invocations of itself. Pure / read-only tools should return `true`;
    /// mutating tools (file writes, shell exec, edit) should return `false`.
    ///
    /// Default is `false` for backward compatibility — old `impl Tool`
    /// blocks automatically default to serial execution.
    fn is_concurrency_safe(&self) -> bool {
        false
    }

    /// Whether the LLM is allowed to invoke this tool directly.
    ///
    /// Defaults to `true`. Tools that should only fire as side-effects
    /// of other tool calls (e.g. auto-loaders, hidden helpers) can
    /// override to return `false` to be removed from the LLM's tool
    /// choice enumeration.
    ///
    /// Note: `is_user_invocable` is independent of `is_hidden`. A tool
    /// can be `is_user_invocable() == true` AND `is_hidden() == true`
    /// — that means the LLM sees it (so it can call it) but it does
    /// not appear in user-facing `/help` listings.
    fn is_user_invocable(&self) -> bool {
        true
    }

    /// Whether the orchestrator may run this tool in parallel with
    /// others. Defaults to [`ExecutionMode::Parallel`].
    fn execution_mode(&self) -> ExecutionMode {
        ExecutionMode::Parallel
    }

    /// Default adapter from a raw JSON value (the LLM tool result)
    /// into a structured [`ToolOutput`]. Tools that need richer
    /// packaging (e.g. truncation metadata, counts) override this.
    fn output(&self, raw: serde_json::Value) -> ToolOutput {
        ToolOutput::from_raw(raw)
    }

    async fn call(&self, input: ToolInput) -> ToolOutput;

    /// Execute the tool with a sandbox attempt selected by the orchestrator.
    ///
    /// Tools that spawn subprocesses (e.g. bash) should apply the sandbox via
    /// [`synthia_sandbox::SandboxAttempt::wrap`] before spawning. Pure tools
    /// (file I/O, search) do not spawn subprocesses and rely on the default
    /// implementation, which ignores `sandbox_attempt` and delegates to
    /// [`call`](Self::call).
    ///
    /// `sandbox_attempt` is `&SandboxAttempt` (not owned) because the
    /// orchestrator may reuse the same selection across retries; tools must
    /// not assume they can consume it.
    ///
    /// `token` is a cancellation token that tools should check periodically
    /// to allow cooperative cancellation of long-running operations.
    async fn call_with_sandbox(
        &self,
        input: ToolInput,
        sandbox_attempt: &synthia_sandbox::SandboxAttempt,
        token: &CancellationToken,
    ) -> ToolOutput {
        let _ = sandbox_attempt;
        let _ = token;
        self.call(input).await
    }

    /// Execute the tool with progress events for file mutations.
    ///
    /// The default implementation delegates to [`call`](Self::call) and
    /// discards the event callback. File-mutating tools that want to expose
    /// per-hunk progress (e.g. `apply_patch`) override this method.
    async fn call_with_progress(
        &self,
        input: ToolInput,
        _on_event: FileChangeCallback,
        _token: &CancellationToken,
    ) -> ToolOutput {
        self.call(input).await
    }
}
