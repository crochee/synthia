use super::*;
use crate::context_trace::compute_prefix_hash;

#[test]
fn test_span_context_creation() {
    let ctx = SpanContext::new("test-session");
    assert_eq!(ctx.session_id(), "test-session");
}

#[test]
fn test_session_start_creates_span() {
    let mut ctx = SpanContext::new("sess-1");
    let guard = ctx.session_start();
    // guard is an EnteredSpan, dropping it exits the span
    drop(guard);
}

#[test]
fn test_invocation_start() {
    let mut ctx = SpanContext::new("sess-1");
    let _session_guard = ctx.session_start();
    let invocation_guard = ctx.invocation_start(1);
    // guard is an EnteredSpan
    drop(invocation_guard);
}

#[test]
fn test_step_llm_call() {
    let ctx = SpanContext::new("sess-1");
    let span = ctx.step_llm_call(1, "gpt-4");
    // Verify span can be entered and dropped without panic
    let _enter = span.enter();
}

#[test]
fn test_step_llm_call_with_attrs() {
    let ctx = SpanContext::new("sess-1");
    let span =
        ctx.step_llm_call_with_attrs(1, "gpt-4", "hash123", 100, 50, 200);
    let _enter = span.enter();
}

#[test]
fn test_step_tool_execution() {
    let ctx = SpanContext::new("sess-1");
    let span = ctx.step_tool_execution(1, "bash", "call-abc");
    let _enter = span.enter();
}

#[test]
fn test_step_context_assembly() {
    let ctx = SpanContext::new("sess-1");
    let span = ctx.step_context_assembly(1, 1024);
    let _enter = span.enter();
}

#[test]
fn test_step_guardian_check() {
    let ctx = SpanContext::new("sess-1");
    let span = ctx.step_guardian_check(1);
    let _enter = span.enter();
}

#[test]
fn test_step_compaction() {
    let ctx = SpanContext::new("sess-1");
    let span = ctx.step_compaction(1, 2000, 800);
    let _enter = span.enter();
}

#[test]
fn test_step_kind_display() {
    assert_eq!(StepKind::LlmCall.to_string(), "llm_call");
    assert_eq!(StepKind::ToolExecution.to_string(), "tool_execution");
    assert_eq!(StepKind::ContextAssembly.to_string(), "context_assembly");
    assert_eq!(StepKind::GuardianCheck.to_string(), "guardian_check");
    assert_eq!(StepKind::Compaction.to_string(), "compaction");
}

#[test]
fn test_span_hierarchy_session_then_invocation() {
    let mut ctx = SpanContext::new("sess-hierarchy");
    let _session_guard = ctx.session_start();

    // While session is entered, create invocation span
    let inv1 = ctx.invocation_start(1);
    // inv1 is an EnteredSpan
    drop(inv1);

    // Create another invocation
    let inv2 = ctx.invocation_start(2);
    drop(inv2);
}

#[test]
fn test_compute_prefix_hash_deterministic() {
    let msgs = vec!["hello".to_string(), "world".to_string()];
    let h1 = compute_prefix_hash(&msgs);
    let h2 = compute_prefix_hash(&msgs);
    assert_eq!(h1, h2);
    assert_eq!(h1.len(), 64); // SHA256 hex length
}

#[test]
fn test_compute_prefix_hash_different_inputs() {
    let h1 = compute_prefix_hash(&["hello".to_string()]);
    let h2 = compute_prefix_hash(&["world".to_string()]);
    assert_ne!(h1, h2);
}

#[test]
fn test_span_context_clone() {
    let ctx = SpanContext::new("sess-clone");
    let cloned = ctx.clone();
    assert_eq!(ctx.session_id(), cloned.session_id());
}

#[test]
fn test_step_generic_with_attrs() {
    let ctx = SpanContext::new("sess-1");
    let mut attrs = SpanAttributes::new();
    attrs.insert("custom_key".to_string(), serde_json::json!("custom_value"));
    let span = ctx.step("custom_step", 1, attrs);
    let _enter = span.enter();
}
