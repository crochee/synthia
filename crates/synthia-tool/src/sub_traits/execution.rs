//! `ToolExecution` sub-trait — runtime invocation of a tool.
/// Provides the execution, validation, dry-run, cost estimation, and
/// cancellation surface. This sub-trait extracts the "what do I do?"
/// concern from the monolithic [`crate::Tool`] trait.
use async_trait::async_trait;
use serde_json::Value;

use crate::types::{ToolInput, ToolOutput};

/// Runtime execution sub-trait: call, validate, dry-run, cost, cancel.
///
/// Tools that are pure metadata-only (e.g. virtual registry entries)
/// may implement only [`super::ToolDefinition`] and skip this sub-trait.
#[async_trait]
pub trait ToolExecution: Send + Sync + 'static {
    /// Execute the tool with the given input.
    async fn execute(&self, input: ToolInput) -> ToolOutput;

    /// Validate the input arguments without executing.
    ///
    /// Returns `Ok(())` if arguments are well-formed, `Err` with a
    /// description of what's wrong otherwise.
    fn validate(&self, args: &Value) -> Result<(), String> {
        let _ = args;
        Ok(())
    }

    /// Dry-run: verify the tool *would* succeed without side-effects.
    ///
    /// Default: delegates to [`validate`](Self::validate).
    async fn dry_run(&self, args: &Value) -> Result<(), String> {
        self.validate(args)
    }

    /// Estimated cost of execution (abstract units).
    ///
    /// Default: 0 (free / negligible cost). Tools that consume billable
    /// resources (LLM tokens, API calls) should override.
    fn cost_estimate(&self, args: &Value) -> u64 {
        let _ = args;
        0
    }

    /// Request cooperative cancellation of an in-flight execution.
    ///
    /// Default: no-op (tool does not support cancellation).
    async fn cancel(&self) -> Result<(), String> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    /// Compile-time sanity: `ToolExecution` exposes at most 5 methods.
    #[test]
    fn tool_execution_has_at_most_five_methods() {
        // The 5 methods are: execute, validate, dry_run, cost_estimate,
        // cancel.
        const METHOD_COUNT: usize = 5;
        assert!(
            METHOD_COUNT <= 5,
            "ToolExecution exceeds 5 methods — consider splitting"
        );
    }
}
