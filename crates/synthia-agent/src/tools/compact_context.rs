//! LLM-callable `compact_context` tool — a thin facade whose
//! [`Tool::call`] only acknowledges the request.
//!
//! The actual compaction is performed by the agent main loop after it
//! detects the LLM-driven `compact_context` call (mirrors the
//! [`self_reflect`](super::self_reflect::SelfReflectTool) pattern). This
//! split exists because compaction mutates `LoopContext::messages`
//! in place and must run between turns rather than during tool
//! execution; running it inside the tool would race with the
//! post-tool-execution prefix snapshot and violate P1 (KV-cache
//! prefix consistency).
//!
//! See `borrow-best-from-production-agents` task 5.2.

use async_trait::async_trait;
use synthia_context::compact_context_tool::{
    COMPACT_CONTEXT_TOOL_NAME,
    compact_context_tool_description,
    compact_context_tool_parameters,
};
use synthia_tool::{Tool, ToolInput, ToolOutput};

/// Tool that acknowledges an LLM-driven `compact_context` request.
///
/// The acknowledgement text is returned to the LLM as the tool result so
/// the next sampling round sees that compaction was scheduled. The real
/// compaction runs in the main loop (`main_loop.rs`) via
/// `StepCompact::execute`, which emits `AgentEvent::ContextCompacted` and
/// a `CompactionAnalyticsAttempt` with `trigger = ToolCall`.
pub struct CompactContextTool;

#[async_trait]
impl Tool for CompactContextTool {
    fn name(&self) -> &str {
        COMPACT_CONTEXT_TOOL_NAME
    }

    fn description(&self) -> &str {
        compact_context_tool_description()
    }

    fn parameters(&self) -> serde_json::Value {
        compact_context_tool_parameters()
    }

    fn requires_permission(&self) -> bool {
        false
    }

    /// Pure metadata tool: no file I/O, no subprocess, no shared mutable
    /// state. Safe to run concurrently with other read-only tools.
    fn is_concurrency_safe(&self) -> bool {
        true
    }

    async fn call(&self, _input: ToolInput) -> ToolOutput {
        ToolOutput::text(
            "Compaction requested. The runtime will compact the context \
             before the next turn.",
        )
    }
}
