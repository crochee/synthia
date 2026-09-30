//! Shared test fixtures and the `collect_results` stream helper.
//!
//! Three tool impls (`TestEntryTool`, `ShadowTool`,
//! `NamedTool`) are referenced across every section of the
//! parent `mod tests` block — registration, exposure,
//! session-scope, snapshot, descriptor cache, mutations. They
//! live here so the shared implementations have one canonical
//! home and the parent block can stop redefining them inline.
//!
//! Items are `pub` (gated by `#[cfg(test)]` on the parent
//! module) so the parent `tests/mod.rs` can re-export them
//! and sibling test sub-modules can pick them up via
//! `use super::*;`.

use async_trait::async_trait;

use super::{Context, Tool, ToolOutput};

/// Drain a `run_stream` stream and collect exactly one `Result` per
/// expected call. Drops `Progress` items. Used by tests that don't care
/// about progress visibility — they just want the final outputs.
pub async fn collect_results(
    stream: impl futures::Stream<Item = (String, crate::traits::StreamOutput)>
    + Unpin,
    expected: usize,
) -> Vec<(String, crate::types::ToolOutput)> {
    use futures::StreamExt;
    let mut stream = std::pin::pin!(stream);
    let mut out = Vec::new();
    while let Some((call_id, item)) = stream.next().await {
        if let crate::traits::StreamOutput::Result(output) = item {
            out.push((call_id, output));
            if out.len() == expected {
                break;
            }
        }
    }
    out
}

#[derive(Debug)]
pub struct TestEntryTool;

#[async_trait]
impl Tool for TestEntryTool {
    fn name(&self) -> &str {
        "test"
    }

    fn description(&self) -> &str {
        "A test tool"
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {}
        })
    }

    async fn call(
        &self,
        _input: serde_json::Value,
        _context: &Context,
    ) -> ToolOutput {
        ToolOutput::text("test output")
    }
}

/// `register_entry` returns `false` when a Core
/// tool already occupies the name. This is the
/// immutability contract for builtin tools —
/// user code cannot shadow them. The
/// provider-side `register_entry_inner` is
/// private, but we exercise the public path
/// via `register_entry` (which delegates).
#[derive(Debug)]
pub struct ShadowTool;

#[async_trait]
impl Tool for ShadowTool {
    fn name(&self) -> &str {
        "core_name"
    }

    fn description(&self) -> &str {
        "tries to shadow a Core tool"
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({"type": "object"})
    }

    async fn call(&self, _: serde_json::Value, _: &Context) -> ToolOutput {
        ToolOutput::text("shadow")
    }
}

/// A tool whose name comes from a `&'static str`
/// constructor argument — used by tests that need
/// arbitrary, alphabetically-ordered tool names to
/// prove deterministic sort order in `snapshot()`
/// and the descriptor / snapshot caches.
#[derive(Debug)]
pub struct NamedTool(pub &'static str);

#[async_trait]
impl Tool for NamedTool {
    fn name(&self) -> &str {
        self.0
    }

    fn description(&self) -> &str {
        "named"
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({"type": "object"})
    }

    async fn call(&self, _: serde_json::Value, _: &Context) -> ToolOutput {
        ToolOutput::text("named")
    }
}
