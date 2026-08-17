use std::pin::Pin;

use async_trait::async_trait;
use futures::Stream;
use serde::{Deserialize, Serialize};

use crate::types::{Context, ToolOutput};

/// How the orchestrator should schedule a tool relative to its peers.
///
/// Default is [`ExecutionMode::Parallel`]; tools that mutate external
/// state (filesystem, processes) should override and return
/// [`ExecutionMode::Sequential`] so the orchestrator never races two
/// invocations of the same tool (or two different mutating tools in the
/// same batch).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionMode {
    /// May run concurrently with other `Parallel` tools.
    #[default]
    Parallel,
    /// Must run alone, after every preceding tool call has completed.
    Sequential,
}

/// Tool stream output — items yielded by [`Tool::stream`].
///
/// - `Progress`: intermediate progress (e.g. file change events,
///   partial results). Not the final result.
/// - `Result`: final output (already truncated). A stream yields
///   exactly one `Result`.
#[derive(Debug, Clone)]
pub enum StreamOutput {
    Progress(ToolOutput),
    Result(ToolOutput),
}

/// Core tool trait — object-safe, 7 methods.
///
/// ## Execution model
///
/// `stream` is the **primary** execution entry point. The dispatcher
/// (`ToolRegistry::run_stream`) consumes it as a `Stream<Item =
/// (call_id, StreamOutput)>`: it forwards every `Progress` item to the
/// agent loop (where they surface as
/// `AgentEvent::System(SystemEvent::ToolProgress)` events), then takes
/// the single `Result` as the tool's final output.
///
/// If a `stream` impl closes without yielding any `Result`, the
/// dispatcher synthesizes `Result(ToolOutput::error("...contract
/// violation"))` so the caller always receives exactly one terminal
/// output per tool invocation. If a `stream` impl panics, the
/// dispatcher recovers the panic payload and surfaces it as
/// `Result(ToolOutput::error("tool `<name>` panicked during
/// execution: <message>"))`.
///
/// `call` is the **simple-implementation path**. Tools whose execution
/// is "compute a single value" implement only `call`; the default
/// `stream` impl wraps `call() → truncate() → StreamOutput::Result`.
///
/// Tools that genuinely stream (long-running shell sessions, file
/// watchers, progress-emitting operations) override `stream`
/// directly — and are responsible for truncating their own `Progress`
/// and `Result` outputs.
#[async_trait]
pub trait Tool: Send + Sync {
    /// Tool name (LLM function name, unique per registry).
    fn name(&self) -> &str;

    /// Human-readable description (LLM tool_choice).
    fn description(&self) -> &str;

    /// JSON Schema for tool parameters (LLM tool_choice enumeration).
    fn parameters(&self) -> serde_json::Value;

    /// Scheduling mode: `Parallel` = concurrent-safe,
    /// `Sequential` = must run alone.
    fn mode(&self) -> ExecutionMode {
        ExecutionMode::Parallel
    }

    /// Simple execution path: receive JSON input + context, return one
    /// tool output. Default `stream` delegates here, so the vast
    /// majority of tools implement only this method.
    ///
    /// Implementations should parse `input` internally:
    /// ```ignore
    /// let args: MyArgs = match serde_json::from_value(input) {
    ///     Ok(a) => a,
    ///     Err(e) => return ToolOutput::error(format!("Invalid arguments: {e}")),
    /// };
    /// ```
    async fn call(
        &self,
        input: serde_json::Value,
        context: &Context,
    ) -> ToolOutput;

    /// Streaming execution: yield zero or more `Progress` items then
    /// exactly one `Result`. Default impl wraps `call()` →
    /// `truncate()` → `StreamOutput::Result`. Tools that need true
    /// streaming (file watchers, long-running commands with progress
    /// events) override this method.
    ///
    /// **Contract**:
    ///
    /// - May yield any number of `Progress` items (zero is allowed).
    /// - MUST eventually yield exactly one `Result`. If the stream
    ///   closes without a `Result`, the dispatcher synthesizes
    ///   `Result(ToolOutput::error("...contract violation"))` so the
    ///   caller always observes a terminal output.
    /// - The dispatcher does NOT truncate stream output. When
    ///   overriding, **call `truncate()` before yielding each item**
    ///   — both `Progress` and `Result`. The default `stream` handles
    ///   truncation only for tools that delegate to `call`.
    /// - Panics are recovered: `AssertUnwindSafe + catch_unwind`
    ///   wraps the drain loop, and the payload surfaces as
    ///   `Result(ToolOutput::error("tool `<name>` panicked during
    ///   execution: <message>"))`. Don't rely on panics for control
    ///   flow — they are diagnostic only.
    fn stream<'a>(
        &'a self,
        input: serde_json::Value,
        context: &'a Context,
    ) -> Pin<Box<dyn Stream<Item = StreamOutput> + Send + 'a>> {
        Box::pin(futures::stream::once(async move {
            let mut output = self.call(input, context).await;
            self.truncate(&mut output, context).await;
            StreamOutput::Result(output)
        }))
    }

    /// Truncate tool output per [`Context::output_bound`] config.
    ///
    /// Default: delegate to [`crate::truncate::bound_output`].
    /// Override for custom truncation strategies.
    async fn truncate(&self, output: &mut ToolOutput, context: &Context) {
        let _ = crate::truncate::bound_output(
            output,
            &context.output_bound,
            &context.session_id,
            self.name(),
        )
        .await;
    }
}

#[cfg(test)]
mod tests {
    use futures::StreamExt;

    use super::*;
    use crate::types::ToolOutput;

    // -- StreamOutput enum ----------------------------------------

    /// `StreamOutput::Progress(...)`
    /// MUST wrap a `ToolOutput`
    /// verbatim.
    #[test]
    fn stream_output_progress_wraps_tool_output() {
        let out = ToolOutput::text("intermediate");
        let so = StreamOutput::Progress(out.clone());
        match so {
            StreamOutput::Progress(t) => assert_eq!(t, out),
            StreamOutput::Result(_) => panic!("expected Progress"),
        }
    }

    /// `StreamOutput::Result(...)`
    /// MUST wrap a `ToolOutput`
    /// verbatim.
    #[test]
    fn stream_output_result_wraps_tool_output() {
        let out = ToolOutput::text("final");
        let so = StreamOutput::Result(out.clone());
        match so {
            StreamOutput::Result(t) => assert_eq!(t, out),
            StreamOutput::Progress(_) => panic!("expected Result"),
        }
    }

    /// `StreamOutput` MUST be
    /// `Debug + Clone` (the
    /// dispatcher needs Clone
    /// for error fallback).
    #[test]
    fn stream_output_supports_debug_and_clone() {
        let out = ToolOutput::text("x");
        let progress = StreamOutput::Progress(out.clone());
        let _copy = progress.clone();
        let _ = format!("{:?}", progress);
        let result = StreamOutput::Result(out);
        let _copy = result.clone();
        let _ = format!("{:?}", result);
    }

    // -- Default Tool::stream() contract ---------------------------

    /// The DEFAULT `Tool::stream()`
    /// impl MUST yield exactly one
    /// `Result` (never any `Progress`).
    /// This pins the contract that
    /// tools implementing only
    /// `call()` don't accidentally
    /// emit progress.
    #[tokio::test]
    async fn default_stream_yields_exactly_one_result() {
        struct StubTool;
        #[async_trait::async_trait]
        impl Tool for StubTool {
            fn name(&self) -> &str {
                "stub"
            }

            fn description(&self) -> &str {
                "stub"
            }

            fn parameters(&self) -> serde_json::Value {
                serde_json::json!({})
            }

            async fn call(
                &self,
                _input: serde_json::Value,
                _context: &Context,
            ) -> ToolOutput {
                ToolOutput::text("hi")
            }
        }

        let tool = StubTool;
        let ctx = Context::default();
        let mut stream = tool.stream(serde_json::json!({}), &ctx);
        let mut progress_count = 0;
        let mut result_count = 0;
        while let Some(item) = stream.next().await {
            match item {
                StreamOutput::Progress(_) => progress_count += 1,
                StreamOutput::Result(_) => result_count += 1,
            }
        }
        assert_eq!(progress_count, 0, "default stream MUST NOT yield Progress");
        assert_eq!(
            result_count, 1,
            "default stream MUST yield exactly 1 Result"
        );
    }

    /// The DEFAULT `Tool::stream()`
    /// impl MUST forward the
    /// underlying `call()` output
    /// (text content verbatim).
    #[tokio::test]
    async fn default_stream_forwards_call_output() {
        struct StubTool;
        #[async_trait::async_trait]
        impl Tool for StubTool {
            fn name(&self) -> &str {
                "stub"
            }

            fn description(&self) -> &str {
                "stub"
            }

            fn parameters(&self) -> serde_json::Value {
                serde_json::json!({})
            }

            async fn call(
                &self,
                _input: serde_json::Value,
                _context: &Context,
            ) -> ToolOutput {
                ToolOutput::text("payload-from-call")
            }
        }

        let tool = StubTool;
        let ctx = Context::default();
        let mut stream = tool.stream(serde_json::json!({}), &ctx);
        let mut found = String::new();
        while let Some(item) = stream.next().await {
            if let StreamOutput::Result(out) = item {
                for p in out.content {
                    if let synthia_provider::types::ContentPart::Text(t) = p {
                        found = t.text;
                    }
                }
            }
        }
        assert_eq!(found, "payload-from-call");
    }

    /// `ExecutionMode::Sequential`
    /// MUST be different from
    /// `Parallel` (no accidental
    /// aliasing — they trigger
    /// different scheduler paths).
    #[test]
    fn execution_mode_variants_distinct() {
        assert_ne!(ExecutionMode::Parallel, ExecutionMode::Sequential);
    }

    /// `ExecutionMode` MUST
    /// serialize as snake_case
    /// (`"parallel"`, `"sequential"`).
    #[test]
    fn execution_mode_serializes_as_snake_case() {
        assert_eq!(
            serde_json::to_string(&ExecutionMode::Parallel).unwrap(),
            "\"parallel\""
        );
        assert_eq!(
            serde_json::to_string(&ExecutionMode::Sequential).unwrap(),
            "\"sequential\""
        );
    }
}
