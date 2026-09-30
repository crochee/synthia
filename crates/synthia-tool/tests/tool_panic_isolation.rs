//! R5-9: integration test that pins the panic-isolation contract
//! of `ToolRegistry::run_stream` / `consume_tool_stream_into`.
//!
//! The contract is: a tool whose async body panics MUST NOT take
//! down the registry, the dispatcher, or any sibling tools
//! running in the same `run_stream` call. The panicking tool's
//! output is a synthesised `ToolOutput::error(...)` carrying the
//! panic message; the consumer sees exactly one `Result` for that
//! tool, and the dispatcher exits cleanly.
//!
//! Traitclaw ported the same contract (their `ToolRegistry::run`
//! uses `AssertUnwindSafe + catch_unwind` around each dispatch);
//! this test pins it for synthia.

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use async_trait::async_trait;
use synthia_provider::{ContentPart, TextContent, ToolUse};
use synthia_tool::{
    Context,
    StreamOutput,
    Tool,
    ToolEntry,
    ToolOutput,
    ToolRegistry,
};
use tokio_stream::StreamExt;

/// Tool that panics inside `call`. Used to assert that the
/// registry's panic-isolation contract holds end-to-end.
struct PanickyTool {
    name: &'static str,
}

#[async_trait]
impl Tool for PanickyTool {
    fn name(&self) -> &str {
        self.name
    }

    fn description(&self) -> &str {
        "panics on every call"
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({"type": "object", "properties": {}})
    }

    async fn call(
        &self,
        _input: serde_json::Value,
        _context: &Context,
    ) -> ToolOutput {
        panic!("kaboom_from_panicker");
    }
}

/// Tool that succeeds. Used to verify sibling tools in the
/// same `run_stream` are not affected by a panicking peer.
struct SiblingOkTool {
    name: &'static str,
    invoked: Arc<AtomicUsize>,
}

#[async_trait]
impl Tool for SiblingOkTool {
    fn name(&self) -> &str {
        self.name
    }

    fn description(&self) -> &str {
        "returns a fixed ok output"
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({"type": "object", "properties": {}})
    }

    async fn call(
        &self,
        _input: serde_json::Value,
        _context: &Context,
    ) -> ToolOutput {
        self.invoked.fetch_add(1, Ordering::SeqCst);
        ToolOutput {
            is_error: Some(false),
            content: vec![ContentPart::Text(TextContent {
                text: "ok".to_string(),
                cache_control: None,
            })],
            metadata: serde_json::Map::new(),
            truncated_by: None,
        }
    }
}

fn tool_use(id: &str, name: &str) -> ToolUse {
    ToolUse {
        id: id.to_string(),
        name: name.to_string(),
        input: serde_json::json!({}),
    }
}

#[tokio::test]
async fn panicking_tool_yields_error_result_and_does_not_kill_session() {
    let reg = ToolRegistry::new();
    reg.register_entry(ToolEntry::new(Arc::new(PanickyTool {
        name: "panicker",
    })));
    let tool_uses = vec![tool_use("c1", "panicker")];
    let mut stream = reg.run_stream(tool_uses, Context::default());
    let mut results: Vec<(String, ToolOutput)> = Vec::new();
    while let Some((call_id, item)) = stream.next().await {
        if let StreamOutput::Result(out) = item {
            results.push((call_id, out));
        }
    }
    // Contract part 1: exactly one `Result` for the panicking
    // tool — the dispatcher synthesises an error Result from
    // the caught panic payload.
    assert_eq!(results.len(), 1, "exactly one Result for the panicker");
    let (call_id, out) = &results[0];
    assert_eq!(call_id, "c1");
    // Contract part 2: the synthesised Result carries
    // `is_error = Some(true)` and the panic message itself. The
    // module doc has always claimed the output carries "the panic
    // message"; it did not, because the payload was passed as
    // `&payload` (`&Box<dyn Any + Send>`) instead of `&*payload`, so
    // every downcast missed and the text was always the opaque
    // fallback. Asserting the message is what keeps that fixed.
    assert_eq!(
        out.is_error,
        Some(true),
        "panic must surface as is_error = true"
    );
    let text = match &out.content[0] {
        ContentPart::Text(t) => t.text.clone(),
        _ => panic!("expected text content"),
    };
    assert!(
        text.contains("panicked during execution"),
        "error text must mention the panic, got: {text}"
    );
    assert!(
        text.contains("kaboom_from_panicker"),
        "the panic message must survive to the consumer, got: {text}"
    );
}

#[tokio::test]
async fn panic_in_one_tool_does_not_affect_sibling_tool() {
    let sibling_invoked = Arc::new(AtomicUsize::new(0));
    let reg = ToolRegistry::new();
    reg.register_entry(ToolEntry::new(Arc::new(PanickyTool {
        name: "panicker",
    })));
    reg.register_entry(ToolEntry::new(Arc::new(SiblingOkTool {
        name: "sibling",
        invoked: Arc::clone(&sibling_invoked),
    })));

    let tool_uses = vec![tool_use("c1", "panicker"), tool_use("c2", "sibling")];
    let mut stream = reg.run_stream(tool_uses, Context::default());
    let mut results: Vec<(String, ToolOutput)> = Vec::new();
    while let Some((call_id, item)) = stream.next().await {
        if let StreamOutput::Result(out) = item {
            results.push((call_id, out));
        }
    }
    assert_eq!(results.len(), 2, "exactly one Result per tool");
    assert_eq!(
        sibling_invoked.load(Ordering::SeqCst),
        1,
        "sibling must run despite panicker panic"
    );
    let sibling = results.iter().find(|(id, _)| id == "c2").unwrap();
    assert_eq!(sibling.1.is_error, Some(false));
}
