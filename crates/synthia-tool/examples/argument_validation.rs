//! # Tool-argument validation, opt-in
//!
//! Run it:
//!
//! ```text
//! cargo run --example argument_validation -p synthia-tool
//! ```
//!
//! The same `ToolRegistry`, the same `Tool` impl — the only thing
//! that changes is `with_argument_validation(true)`. Two
//! dispatches:
//!
//! - **Validation off** (the default): the registry passes the
//!   call's input to the tool body unchanged. The tool's own
//!   `unwrap_or("(missing `text`)")` produces a plain-text
//!   fallback. Behaviour unchanged from before R74.
//! - **Validation on**: the registry validates the call's
//!   arguments against the tool's JSON Schema *before* dispatch.
//!   A mismatch (missing required field, wrong-typed field)
//!   synthesises an `is_error` `Result` whose body lists the
//!   dotted-path violations, so the caller can self-correct
//!   without round-tripping through the tool body.
//!
//! The example ends by printing `ARGUMENT-VALIDATION: OK` after
//! asserting — from the tool's own call counter — that:
//!
//! - the **off** run executed the tool twice (both calls
//!   reached the body, even the malformed one);
//! - the **on** run executed the tool once (the malformed call
//!   was caught at the registry, the tool body ran for the
//!   well-formed call only).

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use async_trait::async_trait;
use synthia_provider::ToolUse;
use synthia_tool::{Context, Tool, ToolEntry, ToolOutput, ToolRegistry};
use tokio_stream::StreamExt;

/// A test tool that requires a `text: string` field. Without it,
/// the call should be rejected at the registry when validation
/// is on.
#[derive(Debug)]
struct EchoTool {
    calls: Arc<AtomicUsize>,
}

#[async_trait]
impl Tool for EchoTool {
    fn name(&self) -> &str {
        "echo"
    }

    fn description(&self) -> &str {
        "Echo the `text` argument back."
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["text"],
            "properties": {
                "text": {"type": "string"},
            },
        })
    }

    async fn call(
        &self,
        input: serde_json::Value,
        _context: &Context,
    ) -> ToolOutput {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let text = input
            .get("text")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("(missing `text`)");
        ToolOutput::text(format!("echo: {text}"))
    }
}

async fn collect_outs(
    registry: &ToolRegistry,
    calls: Vec<ToolUse>,
) -> Vec<ToolOutput> {
    let ctx = Context::new("s1".to_string(), std::path::PathBuf::from("/tmp"));
    let mut out = Vec::new();
    let mut stream = registry.run_stream(calls, ctx);
    while let Some((_id, item)) = stream.next().await {
        if let synthia_tool::traits::StreamOutput::Result(o) = item {
            out.push(o);
        }
    }
    out
}

#[tokio::main]
async fn main() {
    println!("=== synthia: tool-argument validation, opt-in ===\n");

    let calls = Arc::new(AtomicUsize::new(0));

    // ---- validation OFF (default) -----------------------------------
    println!("[1/2] validation OFF — both calls reach the tool body");
    let off = ToolRegistry::new();
    off.register_entry(ToolEntry::new(Arc::new(EchoTool {
        calls: Arc::clone(&calls),
    })));
    assert!(!off.argument_validation_enabled());

    let outs_off = collect_outs(
        &off,
        vec![
            ToolUse {
                id: "c1".to_string(),
                name: "echo".to_string(),
                input: serde_json::json!({"text": "ok"}),
            },
            // Missing required `text`: the registry does not check,
            // so the tool body runs and `unwrap_or` returns the
            // fallback.
            ToolUse {
                id: "c2".to_string(),
                name: "echo".to_string(),
                input: serde_json::json!({}),
            },
        ],
    )
    .await;
    let off_tool_calls = calls.load(Ordering::SeqCst);
    println!("      tool bodies run  : {off_tool_calls} (expected 2)");
    for (i, o) in outs_off.iter().enumerate() {
        let text: String = o
            .content
            .iter()
            .filter_map(|p| match p {
                synthia_provider::ContentPart::Text(t) => Some(t.text.clone()),
                _ => None,
            })
            .collect();
        let is_err = o.is_error.unwrap_or(false);
        println!("      out[{i}] is_error={is_err}: {text}");
    }

    // ---- validation ON ---------------------------------------------
    println!("\n[2/2] validation ON — the malformed call is rejected early");
    calls.store(0, Ordering::SeqCst);
    let on = ToolRegistry::new().with_argument_validation(true);
    on.register_entry(ToolEntry::new(Arc::new(EchoTool {
        calls: Arc::clone(&calls),
    })));
    assert!(on.argument_validation_enabled());

    let outs_on = collect_outs(
        &on,
        vec![
            ToolUse {
                id: "c1".to_string(),
                name: "echo".to_string(),
                input: serde_json::json!({"text": "ok"}),
            },
            // Missing required `text`: the registry synthesises an
            // `is_error: true` Result with the dotted-path
            // violation, and the tool body does NOT run.
            ToolUse {
                id: "c2".to_string(),
                name: "echo".to_string(),
                input: serde_json::json!({}),
            },
        ],
    )
    .await;
    let on_tool_calls = calls.load(Ordering::SeqCst);
    println!("      tool bodies run  : {on_tool_calls} (expected 1)");

    // The well-formed call still succeeded.
    let ok_idx_text: String = outs_on[0]
        .content
        .iter()
        .filter_map(|p| match p {
            synthia_provider::ContentPart::Text(t) => Some(t.text.clone()),
            _ => None,
        })
        .collect();
    println!("      out[0] is_error  : {:?}", outs_on[0].is_error);
    println!("      out[0] text      : {ok_idx_text}");

    // The malformed call is rejected with an is_error body listing
    // the violation.
    assert_eq!(outs_on[1].is_error, Some(true));
    let err_text: String = outs_on[1]
        .content
        .iter()
        .filter_map(|p| match p {
            synthia_provider::ContentPart::Text(t) => Some(t.text.clone()),
            _ => None,
        })
        .collect();
    println!("      out[1] is_error  : {:?}", outs_on[1].is_error);
    println!(
        "      out[1] text head : {}",
        err_text.lines().next().unwrap_or("")
    );

    // ---- the proof ---------------------------------------------------
    assert_eq!(
        off_tool_calls, 2,
        "validation OFF: both calls (even the malformed one) must \
         reach the tool body — that is the unchanged-by-R74 contract"
    );
    assert_eq!(
        on_tool_calls, 1,
        "validation ON: only the well-formed call must reach the \
         tool body; the malformed one is rejected at the registry"
    );
    assert!(
        ok_idx_text.contains("ok"),
        "the well-formed call must still succeed: {ok_idx_text}"
    );
    assert!(
        err_text.contains("invalid arguments for tool `echo`")
            && err_text.contains("text:")
            && err_text.contains("required"),
        "the malformed-call error body must name the missing field: \
         {err_text}"
    );

    println!("\nARGUMENT-VALIDATION: OK");
}
