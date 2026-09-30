//! Dispatch concern (per-tool truncation, streaming-as-
//! primary-execution-path semantics, and the
//! provenance-bearing snapshot projection).
//!
//! Six tests exercise:
//! - per-call truncation bound applied to a too-large output
//!   (`dispatch_applies_tool_truncate`)
//! - streaming dispatch collects the final `Result` while
//!   dropping progress items
//! - a stream that yields no `Result` is a contract violation
//!   (two shapes: empty stream, progress-only stream)
//! - `snapshot_with_provenance` carries the
//!   `ToolProvenance::Dynamic` flag and filters + sorts
//!
//! Local fixtures (`LargeOutputTool`, `StreamingTool`,
//! `EmptyStreamTool`, `ProgressOnlyTool`) move with the
//! section; they are referenced only by these tests.
//!
//! `use super::*;` brings in the parent block's `Arc`,
//! `PathBuf`, `ToolRegistry`, `ToolEntry`, `ToolOutput`,
//! `Context`, `ToolProvenance`, `synthia_provider::ToolUse`,
//! `collect_results`, the `Tool` trait, and `Tool7` (the
//! crate-public `Tool` alias the parent `use` brings into
//! scope). `async_trait` and `futures::stream` are repeated
//! because the sub-module derives `Tool`/`Tool7` impls and
//! builds stream bodies locally.

use async_trait::async_trait;
use futures::{StreamExt, stream};

use super::*;

#[derive(Debug)]
struct LargeOutputTool;

#[async_trait]
impl Tool7 for LargeOutputTool {
    fn name(&self) -> &str {
        "large_output"
    }

    fn description(&self) -> &str {
        "Returns output larger than the configured bound"
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({"type": "object", "properties": {}})
    }

    async fn call(
        &self,
        _input: serde_json::Value,
        _context: &Context,
    ) -> ToolOutput {
        ToolOutput::text("line\n".repeat(100))
    }
}

#[tokio::test]
async fn dispatch_applies_tool_truncate() {
    let registry = ToolRegistry::new();
    assert!(registry.register_entry(ToolEntry::new(Arc::new(LargeOutputTool))));
    let mut context =
        Context::new("truncate-session".to_string(), std::env::temp_dir());
    context.output_bound.per_call_max_bytes = 80;
    context.output_bound.per_call_max_lines = 10;
    context.output_bound.managed_dir = tempfile::tempdir().unwrap().keep();
    let outputs = collect_results(
        registry.run_stream(
            vec![synthia_provider::ToolUse {
                id: "call-1".to_string(),
                name: "large_output".to_string(),
                input: serde_json::json!({}),
            }],
            context,
        ),
        1,
    )
    .await;
    assert!(outputs[0].1.truncated_by.is_some());
    assert!(outputs[0].1.metadata.contains_key("managed_path"));
}

#[derive(Debug)]
struct StreamingTool {
    progress_count: usize,
}

#[async_trait]
impl Tool for StreamingTool {
    fn name(&self) -> &str {
        "streaming"
    }

    fn description(&self) -> &str {
        "Yields N progress items then a final Result"
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({"type": "object", "properties": {}})
    }

    async fn call(
        &self,
        _input: serde_json::Value,
        _context: &Context,
    ) -> ToolOutput {
        panic!("streaming tool should override stream, not call")
    }

    fn stream<'a>(
        &'a self,
        _input: serde_json::Value,
        _context: &'a Context,
    ) -> std::pin::Pin<
        Box<
            dyn futures::Stream<Item = crate::traits::StreamOutput> + Send + 'a,
        >,
    > {
        let n = self.progress_count;
        Box::pin(
            stream::iter((0..n).map(|i| {
                crate::traits::StreamOutput::Progress(ToolOutput::text(
                    format!("step {i}"),
                ))
            }))
            .chain(stream::once(async {
                crate::traits::StreamOutput::Result(ToolOutput::text("done"))
            })),
        )
    }
}

#[tokio::test]
async fn dispatch_consumes_stream_collects_final_result() {
    let registry = ToolRegistry::new();
    registry.register_entry(ToolEntry::new(Arc::new(StreamingTool {
        progress_count: 3,
    })));

    let results = collect_results(
        registry.run_stream(
            vec![synthia_provider::ToolUse {
                id: "call-1".to_string(),
                name: "streaming".to_string(),
                input: serde_json::json!({}),
            }],
            Context::new("s1".to_string(), PathBuf::from("/tmp")),
        ),
        1,
    )
    .await;

    assert_eq!(results.len(), 1);
    // Progress items dropped, only the final Result surfaces.
    let text = results[0].1.content[0].text().unwrap();
    assert_eq!(text, "done");
    assert!(results[0].1.is_error.is_none());
}

#[derive(Debug)]
struct EmptyStreamTool;

#[async_trait]
impl Tool for EmptyStreamTool {
    fn name(&self) -> &str {
        "empty_stream"
    }

    fn description(&self) -> &str {
        "Stream that never yields a Result"
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({"type": "object", "properties": {}})
    }

    async fn call(
        &self,
        _input: serde_json::Value,
        _context: &Context,
    ) -> ToolOutput {
        ToolOutput::text("unused")
    }

    fn stream<'a>(
        &'a self,
        _input: serde_json::Value,
        _context: &'a Context,
    ) -> std::pin::Pin<
        Box<
            dyn futures::Stream<Item = crate::traits::StreamOutput> + Send + 'a,
        >,
    > {
        Box::pin(stream::empty::<crate::traits::StreamOutput>())
    }
}

#[tokio::test]
async fn dispatch_returns_error_when_stream_yields_no_result() {
    let registry = ToolRegistry::new();
    registry.register_entry(ToolEntry::new(Arc::new(EmptyStreamTool)));

    let results = collect_results(
        registry.run_stream(
            vec![synthia_provider::ToolUse {
                id: "call-1".to_string(),
                name: "empty_stream".to_string(),
                input: serde_json::json!({}),
            }],
            Context::new("s1".to_string(), PathBuf::from("/tmp")),
        ),
        1,
    )
    .await;

    assert_eq!(results.len(), 1);
    assert!(results[0].1.is_error.unwrap_or(false));
    let text = results[0].1.content[0].text().unwrap();
    assert!(text.contains("contract violation"), "got: {text}");
}

#[derive(Debug)]
struct ProgressOnlyTool;

#[async_trait]
impl Tool for ProgressOnlyTool {
    fn name(&self) -> &str {
        "progress_only"
    }

    fn description(&self) -> &str {
        "Stream that yields only Progress items, no Result"
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({"type": "object", "properties": {}})
    }

    async fn call(
        &self,
        _input: serde_json::Value,
        _context: &Context,
    ) -> ToolOutput {
        ToolOutput::text("unused")
    }

    fn stream<'a>(
        &'a self,
        _input: serde_json::Value,
        _context: &'a Context,
    ) -> std::pin::Pin<
        Box<
            dyn futures::Stream<Item = crate::traits::StreamOutput> + Send + 'a,
        >,
    > {
        Box::pin(stream::iter([
            crate::traits::StreamOutput::Progress(ToolOutput::text("a")),
            crate::traits::StreamOutput::Progress(ToolOutput::text("b")),
        ]))
    }
}

#[tokio::test]
async fn dispatch_treats_no_result_as_contract_violation() {
    let registry = ToolRegistry::new();
    registry.register_entry(ToolEntry::new(Arc::new(ProgressOnlyTool)));

    let results = collect_results(
        registry.run_stream(
            vec![synthia_provider::ToolUse {
                id: "call-1".to_string(),
                name: "progress_only".to_string(),
                input: serde_json::json!({}),
            }],
            Context::new("s1".to_string(), PathBuf::from("/tmp")),
        ),
        1,
    )
    .await;

    assert_eq!(results.len(), 1);
    assert!(results[0].1.is_error.unwrap_or(false));
}

/// `snapshot_with_provenance` returns one record per visible tool
/// with `provenance: ToolProvenance::Dynamic` (since `register_entry`
/// always wraps as Dynamic).
#[test]
fn snapshot_with_provenance_returns_records_with_provenance() {
    let registry = ToolRegistry::new();
    registry.register_entry(ToolEntry::new(Arc::new(NamedTool("alpha"))));
    let snap = registry.snapshot_with_provenance();
    assert_eq!(snap.len(), 1);
    assert_eq!(snap[0].metadata.name, "alpha");
    assert_eq!(snap[0].provenance, ToolProvenance::Dynamic);
}

/// `snapshot_with_provenance` filters hidden entries (same as
/// `snapshot`) and returns records sorted by name.
#[test]
fn snapshot_with_provenance_skips_hidden_and_sorts() {
    let registry = ToolRegistry::new();
    registry.register_entry(ToolEntry::new(Arc::new(NamedTool("zoo"))));
    registry.register_entry(
        ToolEntry::new(Arc::new(NamedTool("apple"))).with_is_hidden(true),
    );
    registry.register_entry(ToolEntry::new(Arc::new(NamedTool("mango"))));
    let snap = registry.snapshot_with_provenance();
    let names: Vec<&str> =
        snap.iter().map(|r| r.metadata.name.as_str()).collect();
    assert_eq!(names, vec!["mango", "zoo"]);
}
