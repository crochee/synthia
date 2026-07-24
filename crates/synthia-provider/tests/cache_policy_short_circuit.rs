//! Integration tests for [`CachePolicyApplier`] short-circuit behavior.
//!
//! Verifies that `Arc::ptr_eq` reference equality is used to skip
//! redundant cache policy re-application when the same `tools` /
//! `messages` `Arc` references are passed across calls — aligned with
//! opencode's `applyCachePolicy` reference equality semantics.
//!
//! # How the short-circuit works
//!
//! On the first call, [`CachePolicyApplier::apply`] performs full
//! evaluation via [`apply_cache_policy`], which uses
//! [`Arc::make_mut`] to mutate the inner `Vec` in place (when the
//! `Arc` has refcount 1, `make_mut` does NOT clone, so the pointer is
//! preserved). The applier then stores `Arc::clone` references to the
//! (now-marked) `tools` / `messages`.
//!
//! On the second call with the SAME request (same `Arc` pointers),
//! `Arc::ptr_eq` matches and the short-circuit fires: no
//! `apply_cache_policy` call, no `make_mut`, no allocation. The marks
//! from the first call are still present on the shared `Arc` data.

use std::sync::Arc;

use synthia_provider::{
    CachePolicy,
    CachePolicyApplier,
    types::{
        CompletionRequest,
        Content,
        ContentPart,
        Message,
        ToolChoice,
        ToolDefinition,
    },
};

/// Build a `CompletionRequest` with 2 tools and 3 messages
/// (user / assistant / user). Each `Arc<Vec<T>>` has refcount 1
/// because it is freshly `Arc::new`-ed.
fn make_request() -> CompletionRequest {
    CompletionRequest {
        model: "claude-3".to_string(),
        messages: Arc::new(vec![
            Message::user("hello"),
            Message::assistant("hi there"),
            Message::user("bye"),
        ]),
        tools: Arc::new(vec![
            ToolDefinition::new(
                "tool_a",
                "A",
                serde_json::json!({"type": "object"}),
            ),
            ToolDefinition::new(
                "tool_b",
                "B",
                serde_json::json!({"type": "object"}),
            ),
        ]),
        tool_choice: ToolChoice::Auto,
        temperature: None,
        max_tokens: None,
        stop_sequences: vec![],
        extra_body: None,
        cache_policy: Some(CachePolicy::default()),
    }
}

#[test]
fn first_call_performs_full_evaluation_and_returns_false() {
    let mut applier = CachePolicyApplier::new();
    let mut req = make_request();
    let policy = CachePolicy::default();

    let short_circuited = applier.apply(&mut req, &policy);

    assert!(
        !short_circuited,
        "first call must not short-circuit (no previous references stored)"
    );
    // Marks must be applied: last tool + last user message.
    assert!(
        req.tools.last().unwrap().cache_control.is_some(),
        "last tool must carry a cache_control mark after full evaluation"
    );
}

#[test]
fn second_call_with_same_arcs_short_circuits_and_returns_true() {
    let mut applier = CachePolicyApplier::new();
    let mut req = make_request();
    let policy = CachePolicy::default();

    // First call: full evaluation. make_mut sees refcount 1, mutates
    // in place, preserving the Arc pointer.
    applier.apply(&mut req, &policy);

    // Second call: same request, same Arc pointers → ptr_eq matches.
    let short_circuited = applier.apply(&mut req, &policy);

    assert!(
        short_circuited,
        "second call with identical Arc references must short-circuit"
    );
}

#[test]
fn short_circuit_preserves_marks_unchanged() {
    let mut applier = CachePolicyApplier::new();
    let mut req = make_request();
    let policy = CachePolicy::default();

    // First call applies the marks.
    applier.apply(&mut req, &policy);
    let tool_mark_after_first = req.tools.last().unwrap().cache_control.clone();

    // Second call short-circuits — the marks must be untouched.
    let short_circuited = applier.apply(&mut req, &policy);
    assert!(short_circuited);

    assert_eq!(
        req.tools.last().unwrap().cache_control,
        tool_mark_after_first,
        "marks must be unchanged after a short-circuit (no re-application)"
    );
}

#[test]
fn marks_applied_to_last_tool_and_last_user_message() {
    let mut applier = CachePolicyApplier::new();
    let mut req = make_request();
    let policy = CachePolicy::default();

    applier.apply(&mut req, &policy);

    // Last tool (tool_b) is marked; first tool (tool_a) is not.
    assert!(
        req.tools[0].cache_control.is_none(),
        "non-last tools must not be marked"
    );
    assert!(
        req.tools[1].cache_control.is_some(),
        "last tool must be marked"
    );

    // Last user message ("bye", messages[2]) is marked; the first
    // user message ("hello", messages[0]) is not.
    match &req.messages[0].content {
        Content::Single(ContentPart::Text(tc)) => {
            assert!(
                tc.cache_control.is_none(),
                "non-last user message must not be marked"
            )
        }
        _ => panic!("expected Single(Text) for messages[0]"),
    }
    match &req.messages[2].content {
        Content::Single(ContentPart::Text(tc)) => {
            assert!(
                tc.cache_control.is_some(),
                "last user message must be marked"
            )
        }
        _ => panic!("expected Single(Text) for messages[2]"),
    }
}

#[test]
fn new_tools_arc_breaks_short_circuit() {
    let mut applier = CachePolicyApplier::new();
    let mut req = make_request();
    let policy = CachePolicy::default();

    applier.apply(&mut req, &policy);

    // Replace tools with a new Arc (different allocation).
    req.tools = Arc::new(vec![ToolDefinition::new(
        "tool_c",
        "C",
        serde_json::json!({"type": "object"}),
    )]);

    let short_circuited = applier.apply(&mut req, &policy);

    assert!(
        !short_circuited,
        "different tools Arc pointer must not short-circuit"
    );
    // The new tool must be marked (full evaluation ran).
    assert!(
        req.tools.last().unwrap().cache_control.is_some(),
        "new tool must be marked after full evaluation"
    );
}

#[test]
fn new_messages_arc_breaks_short_circuit() {
    let mut applier = CachePolicyApplier::new();
    let mut req = make_request();
    let policy = CachePolicy::default();

    applier.apply(&mut req, &policy);

    // Replace messages with a new Arc (different allocation).
    req.messages = Arc::new(vec![Message::user("fresh message")]);

    let short_circuited = applier.apply(&mut req, &policy);

    assert!(
        !short_circuited,
        "different messages Arc pointer must not short-circuit"
    );
    // The new last user message must be marked.
    match &req.messages[0].content {
        Content::Single(ContentPart::Text(tc)) => assert!(
            tc.cache_control.is_some(),
            "new last user message must be marked after full evaluation"
        ),
        _ => panic!("expected Single(Text)"),
    }
}

#[test]
fn applier_resets_after_partial_change() {
    let mut applier = CachePolicyApplier::new();
    let mut req = make_request();
    let policy = CachePolicy::default();

    // First call: full evaluation.
    applier.apply(&mut req, &policy);

    // Change only tools (messages unchanged). ptr_eq for tools fails
    // → no short-circuit → full evaluation → applier stores new refs.
    req.tools = Arc::new(vec![ToolDefinition::new(
        "tool_d",
        "D",
        serde_json::json!({}),
    )]);
    let short_circuited = applier.apply(&mut req, &policy);
    assert!(!short_circuited);

    // Now call again with the current (unchanged) request → short-circuit.
    let short_circuited = applier.apply(&mut req, &policy);
    assert!(
        short_circuited,
        "after full evaluation stores new refs, the next identical call must short-circuit"
    );
}

#[test]
fn default_applier_behaves_like_new() {
    let mut a = CachePolicyApplier::default();
    let mut b = CachePolicyApplier::new();
    let mut req = make_request();
    let policy = CachePolicy::default();

    let ra = a.apply(&mut req, &policy);
    let rb = b.apply(&mut req, &policy);

    assert_eq!(ra, rb, "Default and new() must behave identically");
    assert!(!ra, "first call must not short-circuit");
}

#[test]
fn repeated_short_circuits_stay_true() {
    let mut applier = CachePolicyApplier::new();
    let mut req = make_request();
    let policy = CachePolicy::default();

    applier.apply(&mut req, &policy); // full eval

    // 2nd, 3rd, 4th calls with same Arcs → all short-circuit.
    for i in 2..=4 {
        let short_circuited = applier.apply(&mut req, &policy);
        assert!(
            short_circuited,
            "call #{i} with identical Arcs must short-circuit"
        );
    }
}
