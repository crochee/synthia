//! [`RetrieveFullOutputTool`] — the `__get_full_output` virtual
//! tool.
//!
//! A truncating output transformer rewrites an oversized tool
//! result into an excerpt plus a marker naming a handle (for
//! example `[truncated: 1234 chars elided; full output via
//! __get_full_output("out-1")]`) and stashes the full text in a
//! [`FullOutputStore`]. This tool is the model-facing half of that
//! contract: the model calls it with the handle from the marker and
//! gets the original output back.
//!
//! traitclaw parity: `FullOutputRetriever` in
//! `traitclaw-core/src/transformers.rs` serves the same role, but
//! keyed by tool name (so a second oversized call from the same
//! tool silently replaced the first). Synthia keys by the handle
//! the store mints, so every stashed output stays addressable.
//!
//! The tool takes the store as a trait object, so the transformer
//! that stashes output, its host, and this tool all share one
//! instance:
//!
//! ```rust
//! use std::sync::Arc;
//!
//! use synthia_core::InMemoryFullOutputStore;
//! use synthia_tool::{Tool, full_output_tool};
//!
//! let store = Arc::new(InMemoryFullOutputStore::unbounded());
//! let tool = full_output_tool(store);
//! assert_eq!(tool.name(), "__get_full_output");
//! ```

use std::sync::Arc;

use async_trait::async_trait;
use schemars_derive::JsonSchema;
use serde::Deserialize;
use synthia_core::FullOutputStore;

use crate::{
    traits::{ExecutionMode, Tool},
    types::{Context, ToolOutput},
};

/// Tool name the truncation marker advertises. The steering
/// crate's `BudgetAwareTruncator` embeds the same literal (it
/// cannot depend on this crate), so the two are pinned together by
/// test.
pub const FULL_OUTPUT_TOOL_NAME: &str = "__get_full_output";

/// Model-facing description: what the tool does and where the
/// handle comes from.
const DESCRIPTION: &str = "Return the full text of a tool output that was elided \
     earlier by truncation. The truncated result carries a marker such as \
     `[truncated: 1234 chars elided; full output via __get_full_output(\"out-1\")]`; \
     call this tool with that handle to read the complete output. Handles exist \
     only for the current session — an unknown or evicted handle returns an error.";

/// Arguments of [`FULL_OUTPUT_TOOL_NAME`]. Kept private and derive
/// the schema (shell-tool pattern) so the type and the LLM-facing
/// schema cannot drift.
#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(extend("additionalProperties" = false))]
struct RetrieveArgs {
    #[schemars(
        description = "Handle from a truncation marker, e.g. \"out-1\"."
    )]
    handle: String,
}

/// Build the `__get_full_output` tool serving `store`.
///
/// The store MUST be the same instance (or shared backing) the
/// truncating transformer stashes into.
#[must_use]
pub fn full_output_tool(
    store: Arc<dyn FullOutputStore>,
) -> RetrieveFullOutputTool {
    RetrieveFullOutputTool::new(store)
}

/// Virtual tool that serves stashed full outputs back to the model.
pub struct RetrieveFullOutputTool {
    store: Arc<dyn FullOutputStore>,
}

impl RetrieveFullOutputTool {
    /// Tool reading from `store`.
    #[must_use]
    pub fn new(store: Arc<dyn FullOutputStore>) -> Self {
        Self { store }
    }
}

#[async_trait]
impl Tool for RetrieveFullOutputTool {
    fn name(&self) -> &str {
        FULL_OUTPUT_TOOL_NAME
    }

    fn description(&self) -> &str {
        DESCRIPTION
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::to_value(schemars::schema_for!(RetrieveArgs))
            .expect("RetrieveArgs schema is always serializable")
    }

    fn mode(&self) -> ExecutionMode {
        // Read-only lookup into a shared store.
        ExecutionMode::Parallel
    }

    async fn call(
        &self,
        input: serde_json::Value,
        _context: &Context,
    ) -> ToolOutput {
        let args: RetrieveArgs = match serde_json::from_value(input) {
            Ok(args) => args,
            Err(e) => {
                return ToolOutput::error(format!("Invalid arguments: {e}"));
            }
        };
        match self.store.get(&args.handle) {
            Some(text) => ToolOutput::text(text),
            None => ToolOutput::error(format!(
                "unknown output handle: {}",
                args.handle
            )),
        }
    }
}

// `stream` keeps the default single-`Result` shape: a lookup has
// no progress to report.

#[cfg(test)]
mod tests {
    use serde_json::json;
    use synthia_core::InMemoryFullOutputStore;

    use super::*;

    /// Shared store pre-loaded with one stashed output.
    fn store_with(text: &str) -> (Arc<dyn FullOutputStore>, String) {
        let store: Arc<dyn FullOutputStore> =
            Arc::new(InMemoryFullOutputStore::unbounded());
        let handle = store.put(text.to_string());
        (store, handle)
    }

    fn text_of(output: &ToolOutput) -> String {
        output
            .content
            .first()
            .and_then(|part| part.text())
            .map(str::to_string)
            .unwrap_or_default()
    }

    #[test]
    fn name_and_mode_follow_the_virtual_tool_contract() {
        let tool =
            full_output_tool(Arc::new(InMemoryFullOutputStore::unbounded()));
        assert_eq!(tool.name(), "__get_full_output");
        assert_eq!(tool.mode(), ExecutionMode::Parallel);
        assert!(tool.description().contains("elided"));
    }

    #[test]
    fn parameters_require_a_handle_and_forbid_extras() {
        let tool =
            full_output_tool(Arc::new(InMemoryFullOutputStore::unbounded()));
        let schema = tool.parameters();
        assert_eq!(schema["additionalProperties"], json!(false));
        assert_eq!(schema["properties"]["handle"]["type"], json!("string"));
        assert_eq!(schema["required"], json!(["handle"]));
    }

    #[tokio::test]
    async fn call_returns_the_text_stashed_under_the_handle() {
        let (store, handle) = store_with("the complete tool output");
        let tool = RetrieveFullOutputTool::new(store);
        let out = tool
            .call(json!({"handle": handle}), &Context::default())
            .await;
        assert!(out.is_text());
        assert_eq!(text_of(&out), "the complete tool output");
    }

    #[tokio::test]
    async fn unknown_handle_is_an_error_result() {
        let (store, _) = store_with("stashed");
        let tool = RetrieveFullOutputTool::new(store);
        let out = tool
            .call(json!({"handle": "out-99"}), &Context::default())
            .await;
        assert_eq!(out.is_error, Some(true));
        assert_eq!(text_of(&out), "unknown output handle: out-99");
    }

    #[tokio::test]
    async fn missing_handle_argument_is_an_invalid_arguments_error() {
        let (store, _) = store_with("stashed");
        let tool = RetrieveFullOutputTool::new(store);
        let out = tool.call(json!({}), &Context::default()).await;
        assert_eq!(out.is_error, Some(true));
        assert!(text_of(&out).starts_with("Invalid arguments:"));
    }
}
