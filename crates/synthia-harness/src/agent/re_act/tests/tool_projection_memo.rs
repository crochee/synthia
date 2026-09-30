//! R59 tool-projection memo: the loop hands the *same*
//! `Vec<ToolDefinition>` allocation to the provider on
//! consecutive iterations when nothing in the tool set
//! changed, and produces a fresh allocation when the
//! transcript-driven `Deferred` promotion changes the
//! projection.

use std::sync::Arc;

use serde_json::json;
use synthia_test_support::FakeTool;
use synthia_tool::{ToolExposure, ToolRegistry};
use tokio_util::sync::CancellationToken;

use super::{support::*, *};

/// Two iterations of one run ask the same projection question when
/// nothing about the tool set changed, so the loop hands the *same*
/// allocation to the provider on both requests. Asserted through the
/// provider's captured `Arc`s, which is what the wire actually carries.
#[tokio::test]
async fn repeated_iterations_share_one_tool_definition_allocation() {
    let use_echo = ToolUse {
        id: "c1".to_string(),
        name: "echo".to_string(),
        input: json!({}),
    };
    let provider = Arc::new(CapturingProvider::new(vec![
        tool_call_response(vec![use_echo]),
        empty_response(),
    ]));
    let registry = Arc::new(ToolRegistry::new());
    registry.register_entry(synthia_tool::ToolEntry::new(Arc::new(
        FakeTool::new("echo", "echoed"),
    )));
    registry.register_entry(synthia_tool::ToolEntry::dynamic(
        "read".to_string(),
        "read a file".to_string(),
        json!({"type": "object"}),
    ));

    let _events = run_and_collect(
        provider.clone(),
        Arc::clone(&registry),
        CancellationToken::new(),
        AgentInput::text("go"),
    )
    .await;

    let captured = provider.captured_tools.lock().await;
    assert_eq!(captured.len(), 2, "one request per iteration");
    assert!(
        Arc::ptr_eq(&captured[0], &captured[1]),
        "an unchanged tool set must reuse the projected allocation"
    );
    // The catalog is still the full one — the memo must not narrow it.
    assert_eq!(captured[0].len(), 2);
    drop(captured);
}

/// …and it does *not* cache when the projection genuinely changes: a
/// `Deferred` tool promoted by the transcript yields a new list (the
/// full schema instead of the placeholder), so the memo cannot serve the
/// stale one.
#[tokio::test]
async fn deferred_promotion_invalidates_the_projection_memo() {
    let call_deep = ToolUse {
        id: "c1".to_string(),
        name: "deep".to_string(),
        input: json!({"q": "hello"}),
    };
    let provider = Arc::new(CapturingProvider::new(vec![
        tool_call_response(vec![call_deep]),
        empty_response(),
    ]));
    let registry = Arc::new(ToolRegistry::new());
    registry.register_entry(
        synthia_tool::ToolEntry::dynamic(
            "deep".to_string(),
            "advertised by name until first call".to_string(),
            json!({"type": "object", "properties": {"q": {"type": "string"}}}),
        )
        .with_exposure(ToolExposure::Deferred),
    );

    let _events = run_and_collect(
        provider.clone(),
        Arc::clone(&registry),
        CancellationToken::new(),
        AgentInput::text("go"),
    )
    .await;

    let captured = provider.captured_tools.lock().await;
    assert_eq!(captured.len(), 2);
    assert!(
        !Arc::ptr_eq(&captured[0], &captured[1]),
        "promotion changes the projection, so it must not be memo-served"
    );
    assert_eq!(
        captured[0][0].input_schema,
        json!({"type": "object", "additionalProperties": true}),
        "before the call: the permissive placeholder"
    );
    assert_eq!(
        captured[1][0].input_schema,
        json!({"type": "object", "properties": {"q": {"type": "string"}}}),
        "after the call: the real schema"
    );
}
