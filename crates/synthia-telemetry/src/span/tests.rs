//! Unit tests for the `span` module family.
//!
//! The original 22 tests lived at the bottom of
//! `span.rs`; they're hoisted into this sibling file
//! so the production code (`kind` / `context` /
//! `builder` / `creators`) doesn't carry the test
//! body weight.
//!
//! Coverage map:
//!
//! - `SpanKind::name` (1): `test_span_kind_names`.
//! - `SpanContext` lifecycle (5): `test_span_context_root`,
//!   `_child`, `_end`, `_with_attribute`, `_set_attribute`.
//! - `SpanBuilder` (3): `_with_parent`, `_root`, `_multiple_attributes`,
//!   `_build_span_method`.
//! - Create-* functions (6): `test_create_session_span`,
//!   `_invocation_span`, `_step_span`,
//!   `_tool_execution_span`, `_context_assembly_span`,
//!   `_guardian_check_span`, `_compaction_span`,
//!   `_llm_call_span_all_attributes`,
//!   `_all_step_kinds`.
//! - Hierarchy invariants (2): `test_span_hierarchy_session_to_llm_call`,
//!   `test_multiple_invocations_same_session`.
//! - Duration (1): `test_span_context_duration`.

use super::{
    builder::SpanBuilder,
    context::SpanContext,
    creators::{
        create_compaction_span,
        create_context_assembly_span,
        create_guardian_check_span,
        create_invocation_span,
        create_llm_call_span,
        create_session_span,
        create_step_span,
        create_tool_execution_span,
    },
    kind::SpanKind,
};

#[test]
fn test_span_kind_names() {
    assert_eq!(SpanKind::Session.name(), "session");
    assert_eq!(SpanKind::Invocation.name(), "invocation");
    assert_eq!(SpanKind::LlmCall.name(), "llm_call");
    assert_eq!(SpanKind::ToolExecution.name(), "tool_execution");
    assert_eq!(SpanKind::ContextAssembly.name(), "context_assembly");
    assert_eq!(SpanKind::GuardianCheck.name(), "guardian_check");
    assert_eq!(SpanKind::Compaction.name(), "compaction");
}

#[test]
fn test_span_context_root() {
    let ctx = SpanContext::root("trace-123");
    assert_eq!(ctx.trace_id, "trace-123");
    assert!(!ctx.span_id.is_empty());
    assert!(ctx.parent_span_id.is_empty());
    assert!(ctx.end_time.is_none());
}

#[test]
fn test_span_context_child() {
    let parent = SpanContext::root("trace-123");
    let child = parent.child();

    assert_eq!(child.trace_id, parent.trace_id);
    assert_eq!(child.parent_span_id, parent.span_id);
    assert_ne!(child.span_id, parent.span_id);
}

#[test]
fn test_span_context_end() {
    let mut ctx = SpanContext::root("trace-123");
    assert!(ctx.end_time.is_none());
    ctx.end();
    assert!(ctx.end_time.is_some());
}

#[test]
fn test_span_context_with_attribute() {
    let ctx = SpanContext::root("trace-123")
        .with_attribute("key1", "value1")
        .with_attribute("key2", "value2");

    assert_eq!(ctx.attributes.get("key1").unwrap(), "value1");
    assert_eq!(ctx.attributes.get("key2").unwrap(), "value2");
}

#[test]
fn test_span_context_set_attribute() {
    let mut ctx = SpanContext::root("trace-123");
    ctx.set_attribute("key", "value");
    assert_eq!(ctx.attributes.get("key").unwrap(), "value");
}

#[test]
fn test_span_builder_with_parent() {
    let parent = SpanContext::root("trace-abc");
    let builder = SpanBuilder::new(SpanKind::Invocation, "invocation")
        .with_parent(&parent)
        .with_attribute("iteration", "1");

    let (ctx, span) = builder.build();

    assert_eq!(ctx.parent_span_id, parent.span_id);
    assert_eq!(ctx.trace_id, parent.trace_id);
    assert_eq!(ctx.attributes.get("iteration").unwrap(), "1");
    // Verify span is valid
    let _ = format!("{:?}", span);
}

#[test]
fn test_span_builder_root() {
    let builder = SpanBuilder::new(SpanKind::Session, "session")
        .with_trace_id("trace-root")
        .with_attribute("session_id", "sess-1");

    let (ctx, span) = builder.build();

    assert!(ctx.parent_span_id.is_empty());
    assert_eq!(ctx.trace_id, "trace-root");
    assert_eq!(ctx.attributes.get("session_id").unwrap(), "sess-1");
    let _ = format!("{:?}", span);
}

#[test]
fn test_create_session_span() {
    let (ctx, span) = create_session_span("test-session");

    assert_eq!(ctx.attributes.get("session_id").unwrap(), "test-session");
    assert_eq!(ctx.parent_span_id, "");
    assert!(!ctx.trace_id.is_empty());
    let _ = format!("{:?}", span);
}

#[test]
fn test_create_invocation_span() {
    let (session_ctx, _) = create_session_span("sess-inv");
    let (inv_ctx, inv_span) = create_invocation_span(&session_ctx, "inv-1", 1);

    assert_eq!(inv_ctx.parent_span_id, session_ctx.span_id);
    assert_eq!(inv_ctx.trace_id, session_ctx.trace_id);
    assert_eq!(inv_ctx.attributes.get("invocation_id").unwrap(), "inv-1");
    assert_eq!(inv_ctx.attributes.get("iteration_number").unwrap(), "1");
    let _ = format!("{:?}", inv_span);
}

#[test]
fn test_create_step_span() {
    let (session_ctx, _) = create_session_span("sess-step");
    let (inv_ctx, _) = create_invocation_span(&session_ctx, "inv-1", 1);
    let (step_ctx, step_span) =
        create_step_span(&inv_ctx, SpanKind::LlmCall, "llm_call", 1);

    assert_eq!(step_ctx.parent_span_id, inv_ctx.span_id);
    assert_eq!(step_ctx.trace_id, inv_ctx.trace_id);
    assert_eq!(step_ctx.attributes.get("iteration_number").unwrap(), "1");
    let _ = format!("{:?}", step_span);
}

#[test]
fn test_create_llm_call_span_all_attributes() {
    let (session_ctx, _) = create_session_span("sess-llm");
    let (inv_ctx, _) = create_invocation_span(&session_ctx, "inv-1", 1);
    let (llm_ctx, llm_span) =
        create_llm_call_span(&inv_ctx, 1, "gpt-4", "hash123", 100, 50, 200);

    assert_eq!(llm_ctx.attributes.get("model").unwrap(), "gpt-4");
    assert_eq!(llm_ctx.attributes.get("prefix_hash").unwrap(), "hash123");
    assert_eq!(llm_ctx.attributes.get("tokens_in").unwrap(), "100");
    assert_eq!(llm_ctx.attributes.get("tokens_out").unwrap(), "50");
    assert_eq!(llm_ctx.attributes.get("latency_ms").unwrap(), "200");
    assert_eq!(llm_ctx.attributes.get("session_id").unwrap(), "sess-llm");
    let _ = format!("{:?}", llm_span);
}

#[test]
fn test_span_hierarchy_session_to_llm_call() {
    // Build a complete hierarchy: session -> invocation -> llm_call
    let (session, _) = create_session_span("sess-hierarchy");
    let (invocation, _) = create_invocation_span(&session, "inv-1", 1);
    let (llm_call, _) =
        create_llm_call_span(&invocation, 1, "gpt-4", "hash", 100, 50, 200);

    // Verify chain: llm_call.parent == invocation.span_id, invocation.parent == session.span_id
    assert_eq!(llm_call.parent_span_id, invocation.span_id);
    assert_eq!(invocation.parent_span_id, session.span_id);

    // All share the same trace_id
    assert_eq!(session.trace_id, invocation.trace_id);
    assert_eq!(invocation.trace_id, llm_call.trace_id);
}

#[test]
fn test_span_builder_multiple_attributes() {
    let parent = SpanContext::root("trace-multi")
        .with_attribute("session_id", "sess-multi");

    let builder = SpanBuilder::new(SpanKind::ToolExecution, "tool_execution")
        .with_parent(&parent)
        .with_attribute("tool_name", "bash")
        .with_attribute("tool_call_id", "call-123")
        .with_attribute("iteration_number", "2");

    let (ctx, _) = builder.build();

    assert_eq!(ctx.attributes.get("tool_name").unwrap(), "bash");
    assert_eq!(ctx.attributes.get("tool_call_id").unwrap(), "call-123");
    assert_eq!(ctx.attributes.get("iteration_number").unwrap(), "2");
}

#[test]
fn test_create_all_step_kinds() {
    let (session, _) = create_session_span("sess-all");
    let (inv, _) = create_invocation_span(&session, "inv-1", 1);

    // Test all step kinds can be created
    let (ctx_llm, _) = create_step_span(&inv, SpanKind::LlmCall, "llm_call", 1);
    let (ctx_tool, _) =
        create_step_span(&inv, SpanKind::ToolExecution, "tool_execution", 1);
    let (ctx_ctx, _) = create_step_span(
        &inv,
        SpanKind::ContextAssembly,
        "context_assembly",
        1,
    );
    let (ctx_guard, _) =
        create_step_span(&inv, SpanKind::GuardianCheck, "guardian_check", 1);
    let (ctx_comp, _) =
        create_step_span(&inv, SpanKind::Compaction, "compaction", 1);

    for ctx in [&ctx_llm, &ctx_tool, &ctx_ctx, &ctx_guard, &ctx_comp] {
        assert_eq!(ctx.parent_span_id, inv.span_id);
        assert_eq!(ctx.trace_id, inv.trace_id);
    }
}

#[test]
fn test_create_tool_execution_span() {
    let (session, _) = create_session_span("sess-tool");
    let (inv, _) = create_invocation_span(&session, "inv-1", 1);
    let (tool_ctx, _) =
        create_tool_execution_span(&inv, 1, "read_file", "call-abc");

    assert_eq!(tool_ctx.attributes.get("tool_name").unwrap(), "read_file");
    assert_eq!(tool_ctx.attributes.get("tool_call_id").unwrap(), "call-abc");
}

#[test]
fn test_create_context_assembly_span() {
    let (session, _) = create_session_span("sess-ctx");
    let (inv, _) = create_invocation_span(&session, "inv-1", 1);
    let (ctx_assembly, _) = create_context_assembly_span(&inv, 1, 1024);

    assert_eq!(ctx_assembly.attributes.get("token_count").unwrap(), "1024");
}

#[test]
fn test_create_guardian_check_span() {
    let (session, _) = create_session_span("sess-guard");
    let (inv, _) = create_invocation_span(&session, "inv-1", 1);
    let (guard_ctx, _) = create_guardian_check_span(&inv, 1);

    assert_eq!(guard_ctx.parent_span_id, inv.span_id);
}

#[test]
fn test_create_compaction_span() {
    let (session, _) = create_session_span("sess-compact");
    let (inv, _) = create_invocation_span(&session, "inv-1", 1);
    let (comp_ctx, _) = create_compaction_span(&inv, 1, 2000, 800);

    assert_eq!(comp_ctx.attributes.get("old_tokens").unwrap(), "2000");
    assert_eq!(comp_ctx.attributes.get("new_tokens").unwrap(), "800");
}

#[test]
fn test_span_context_duration() {
    let mut ctx = SpanContext::root("trace-dur");
    std::thread::sleep(std::time::Duration::from_millis(10));
    ctx.end();
    // Duration should be measurable after end
    assert!(ctx.duration().is_some());
    assert!(ctx.duration().unwrap().as_millis() >= 10);
}

#[test]
fn test_multiple_invocations_same_session() {
    let (session, _) = create_session_span("sess-multi-inv");

    let (inv1, _) = create_invocation_span(&session, "inv-1", 1);
    let (inv2, _) = create_invocation_span(&session, "inv-2", 2);

    // Both invocations are children of the same session
    assert_eq!(inv1.parent_span_id, session.span_id);
    assert_eq!(inv2.parent_span_id, session.span_id);

    // But have different span IDs
    assert_ne!(inv1.span_id, inv2.span_id);

    // All share the same trace
    assert_eq!(inv1.trace_id, inv2.trace_id);
    assert_eq!(inv1.trace_id, session.trace_id);
}

#[test]
fn test_span_builder_build_span_method() {
    let parent = SpanContext::root("trace-build");
    let builder = SpanBuilder::new(SpanKind::LlmCall, "llm_call")
        .with_parent(&parent)
        .with_attribute("model", "gpt-4");

    let span = builder.build_span();
    let _ = format!("{:?}", span);
}
