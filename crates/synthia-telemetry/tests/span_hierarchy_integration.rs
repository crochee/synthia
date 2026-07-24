//! Integration tests for OTel span hierarchy.
//!
//! These tests verify the complete span hierarchy that mirrors the ReAct loop:
//!   session -> invocation -> (guardian_check, context_assembly, llm_call, tool_execution, compaction)

use synthia_telemetry::{
    OtSpanContext,
    SpanBuilder,
    SpanKind,
    create_compaction_span,
    create_context_assembly_span,
    create_guardian_check_span,
    create_invocation_span,
    create_llm_call_span,
    create_session_span,
    create_tool_execution_span,
};

type StepCreator =
    Box<dyn FnOnce(&OtSpanContext) -> (OtSpanContext, tracing::Span)>;

/// Simulate a full ReAct loop iteration and verify span hierarchy.
#[test]
fn test_full_react_loop_span_hierarchy() {
    // 1. Session root
    let (session_ctx, _session_span) =
        create_session_span("session-integration-1");

    assert_eq!(session_ctx.parent_span_id, "");
    assert_eq!(
        session_ctx.attributes.get("session_id").unwrap(),
        "session-integration-1"
    );
    assert!(!session_ctx.trace_id.is_empty());

    // 2. Invocation (child of session)
    let (inv_ctx, _inv_span) = create_invocation_span(&session_ctx, "inv-1", 1);
    assert_eq!(inv_ctx.parent_span_id, session_ctx.span_id);
    assert_eq!(inv_ctx.trace_id, session_ctx.trace_id);
    assert_eq!(inv_ctx.attributes.get("iteration_number").unwrap(), "1");

    // 3. Guardian check (child of invocation)
    let (guard_ctx, _guard_span) = create_guardian_check_span(&inv_ctx, 1);
    assert_eq!(guard_ctx.parent_span_id, inv_ctx.span_id);
    assert_eq!(
        guard_ctx.attributes.get("session_id").unwrap(),
        "session-integration-1"
    );

    // 4. Context assembly (child of invocation)
    let (ctx_asm, _ctx_span) = create_context_assembly_span(&inv_ctx, 1, 2048);
    assert_eq!(ctx_asm.parent_span_id, inv_ctx.span_id);
    assert_eq!(ctx_asm.attributes.get("token_count").unwrap(), "2048");

    // 5. LLM call (child of invocation)
    let (llm_ctx, _llm_span) = create_llm_call_span(
        &inv_ctx,
        1,
        "gpt-4o",
        "abc123hash",
        500,
        150,
        1200,
    );
    assert_eq!(llm_ctx.parent_span_id, inv_ctx.span_id);
    assert_eq!(llm_ctx.attributes.get("model").unwrap(), "gpt-4o");
    assert_eq!(llm_ctx.attributes.get("prefix_hash").unwrap(), "abc123hash");
    assert_eq!(llm_ctx.attributes.get("tokens_in").unwrap(), "500");
    assert_eq!(llm_ctx.attributes.get("tokens_out").unwrap(), "150");
    assert_eq!(llm_ctx.attributes.get("latency_ms").unwrap(), "1200");

    // 6. Tool execution (child of invocation)
    let (tool_ctx, _tool_span) =
        create_tool_execution_span(&inv_ctx, 1, "read_file", "call-abc");
    assert_eq!(tool_ctx.parent_span_id, inv_ctx.span_id);
    assert_eq!(tool_ctx.attributes.get("tool_name").unwrap(), "read_file");
    assert_eq!(tool_ctx.attributes.get("tool_call_id").unwrap(), "call-abc");

    // 7. Compaction (child of invocation)
    let (comp_ctx, _comp_span) =
        create_compaction_span(&inv_ctx, 1, 4000, 1500);
    assert_eq!(comp_ctx.parent_span_id, inv_ctx.span_id);
    assert_eq!(comp_ctx.attributes.get("old_tokens").unwrap(), "4000");
    assert_eq!(comp_ctx.attributes.get("new_tokens").unwrap(), "1500");

    // Verify all step spans share the same trace_id
    for ctx in [&guard_ctx, &ctx_asm, &llm_ctx, &tool_ctx, &comp_ctx] {
        assert_eq!(ctx.trace_id, session_ctx.trace_id);
    }
}

/// Test that multiple invocations create independent span trees under the same session.
#[test]
fn test_multiple_invocations_independent_span_trees() {
    let (session, _span) = create_session_span("sess-multi-iter");

    let spans_iter1: Vec<_> = build_invocation_spans(&session, "inv-1", 1);
    let spans_iter2: Vec<_> = build_invocation_spans(&session, "inv-2", 2);

    // All spans from both iterations share the same session trace_id
    for (ctx, _) in &spans_iter1 {
        assert_eq!(ctx.trace_id, session.trace_id);
    }
    for (ctx, _) in &spans_iter2 {
        assert_eq!(ctx.trace_id, session.trace_id);
    }

    // Invocation 1 spans are NOT children of Invocation 2
    let inv1_parent = spans_iter1[0].0.parent_span_id.clone();
    let inv2_parent = spans_iter2[0].0.parent_span_id.clone();
    assert_ne!(inv1_parent, spans_iter2[0].0.span_id);
    assert_ne!(inv2_parent, spans_iter1[0].0.span_id);

    // Both invocations are direct children of the session
    assert_eq!(inv1_parent, session.span_id);
    assert_eq!(inv2_parent, session.span_id);
}

/// Test that SpanBuilder correctly propagates trace_id from parent to grandchild.
#[test]
fn test_span_builder_trace_id_propagation_three_levels() {
    let (session, _) = create_session_span("sess-propagation");
    let (invocation, _) = create_invocation_span(&session, "inv-1", 1);

    // Build a grandchild span manually using SpanBuilder
    let builder = SpanBuilder::new(SpanKind::LlmCall, "llm_call")
        .with_parent(&invocation)
        .with_attribute("model", "claude-3")
        .with_attribute("session_id", "sess-propagation");

    let (llm_ctx, llm_span) = builder.build();

    assert_eq!(llm_ctx.trace_id, session.trace_id);
    assert_eq!(llm_ctx.trace_id, invocation.trace_id);
    assert_eq!(llm_ctx.parent_span_id, invocation.span_id);
    assert_eq!(llm_ctx.attributes.get("model").unwrap(), "claude-3");

    // tracing::Span should be valid
    let _ = format!("{:?}", llm_span);
}

/// Test that all span attributes are correctly recorded for LLM spans.
#[test]
fn test_llm_span_attributes_completeness() {
    let (session, _) = create_session_span("sess-llm-attrs");
    let (inv, _) = create_invocation_span(&session, "inv-1", 1);

    let (ctx, _) =
        create_llm_call_span(&inv, 5, "o1-mini", "hash-def", 1024, 256, 3500);

    // All required attributes must be present
    let required_attrs = [
        "session_id",
        "iteration_number",
        "model",
        "prefix_hash",
        "tokens_in",
        "tokens_out",
        "latency_ms",
    ];

    for attr in &required_attrs {
        assert!(
            ctx.attributes.contains_key(*attr),
            "LLM span should have attribute: {}",
            attr
        );
    }

    // Verify values
    assert_eq!(ctx.attributes.get("session_id").unwrap(), "sess-llm-attrs");
    assert_eq!(ctx.attributes.get("iteration_number").unwrap(), "5");
    assert_eq!(ctx.attributes.get("model").unwrap(), "o1-mini");
    assert_eq!(ctx.attributes.get("prefix_hash").unwrap(), "hash-def");
    assert_eq!(ctx.attributes.get("tokens_in").unwrap(), "1024");
    assert_eq!(ctx.attributes.get("tokens_out").unwrap(), "256");
    assert_eq!(ctx.attributes.get("latency_ms").unwrap(), "3500");
}

/// Test that all step spans have session_id and iteration_number attributes.
#[test]
fn test_all_step_spans_have_common_attributes() {
    let (session, _) = create_session_span("sess-common-attrs");
    let (inv, _) = create_invocation_span(&session, "inv-1", 3);

    let step_creators: Vec<(&str, StepCreator)> = vec![
        (
            "guardian_check",
            Box::new(|p| create_guardian_check_span(p, 3)),
        ),
        (
            "context_assembly",
            Box::new(|p| create_context_assembly_span(p, 3, 512)),
        ),
        (
            "llm_call",
            Box::new(|p| create_llm_call_span(p, 3, "gpt-4", "h", 10, 5, 100)),
        ),
        (
            "tool_execution",
            Box::new(|p| create_tool_execution_span(p, 3, "bash", "c-1")),
        ),
        (
            "compaction",
            Box::new(|p| create_compaction_span(p, 3, 1000, 400)),
        ),
    ];

    for (name, creator) in step_creators {
        let (ctx, _) = creator(&inv);
        assert!(
            ctx.attributes.contains_key("session_id"),
            "{} span should have session_id",
            name
        );
        assert!(
            ctx.attributes.contains_key("iteration_number"),
            "{} span should have iteration_number",
            name
        );
        assert_eq!(
            ctx.attributes.get("session_id").unwrap(),
            "sess-common-attrs",
            "{} span should have correct session_id",
            name
        );
        assert_eq!(
            ctx.attributes.get("iteration_number").unwrap(),
            "3",
            "{} span should have correct iteration_number",
            name
        );
    }
}

/// Test that SpanKind::Session is only used for root spans.
#[test]
fn test_session_span_kind_is_root_only() {
    let (session, _) = create_session_span("sess-root-only");
    // Session span has no parent
    assert!(session.parent_span_id.is_empty());
    // Session span has a unique trace_id
    assert!(!session.trace_id.is_empty());
}

/// Test that the span hierarchy correctly supports the full session lifecycle:
/// session -> inv1 -> [steps] -> inv2 -> [steps] -> session end
#[test]
fn test_complete_session_lifecycle() {
    let (session_ctx, _session_span) = create_session_span("sess-lifecycle");

    // Iteration 1
    let (inv1, _) = create_invocation_span(&session_ctx, "inv-1", 1);
    let (llm1, _) = create_llm_call_span(&inv1, 1, "gpt-4", "h1", 200, 80, 500);
    let (tool1, _) = create_tool_execution_span(&inv1, 1, "search", "t-1");

    // Iteration 2
    let (inv2, _) = create_invocation_span(&session_ctx, "inv-2", 2);
    let (llm2, _) = create_llm_call_span(&inv2, 2, "gpt-4", "h2", 300, 60, 700);
    let (guard2, _) = create_guardian_check_span(&inv2, 2);

    // Iteration 3 (with compaction)
    let (inv3, _) = create_invocation_span(&session_ctx, "inv-3", 3);
    let (llm3, _) =
        create_llm_call_span(&inv3, 3, "gpt-4", "h3", 400, 100, 800);
    let (comp3, _) = create_compaction_span(&inv3, 3, 5000, 2000);

    // Verify hierarchy depth
    assert_eq!(llm1.parent_span_id, inv1.span_id);
    assert_eq!(inv1.parent_span_id, session_ctx.span_id);
    assert_eq!(llm2.parent_span_id, inv2.span_id);
    assert_eq!(inv2.parent_span_id, session_ctx.span_id);
    assert_eq!(comp3.parent_span_id, inv3.span_id);
    assert_eq!(inv3.parent_span_id, session_ctx.span_id);

    // All spans share the session trace_id
    for ctx in [
        &llm1, &tool1, &llm2, &guard2, &llm3, &comp3, &inv1, &inv2, &inv3,
    ] {
        assert_eq!(ctx.trace_id, session_ctx.trace_id);
    }

    // Verify span ends properly
    let mut session_ctx_end = session_ctx.clone();
    session_ctx_end.end();
    assert!(session_ctx_end.end_time.is_some());
}

/// Helper function that builds all step spans for an invocation (simulating ReAct loop).
fn build_invocation_spans(
    session: &OtSpanContext,
    inv_id: &str,
    iteration: usize,
) -> Vec<(OtSpanContext, tracing::Span)> {
    let (inv, inv_span) = create_invocation_span(session, inv_id, iteration);

    // Create a mutable copy of invocation context for step parent
    let inv_parent = inv.clone();

    let mut spans = vec![(inv, inv_span)];

    // Guardian check
    let (g, _) = create_guardian_check_span(&inv_parent, iteration);
    spans.push((g, tracing::Span::none()));

    // Context assembly
    let (c, _) = create_context_assembly_span(&inv_parent, iteration, 1024);
    spans.push((c, tracing::Span::none()));

    // LLM call
    let (l, _) = create_llm_call_span(
        &inv_parent,
        iteration,
        "gpt-4o",
        "hash",
        256,
        128,
        800,
    );
    spans.push((l, tracing::Span::none()));

    // Tool execution
    let (t, _) =
        create_tool_execution_span(&inv_parent, iteration, "bash", "call-1");
    spans.push((t, tracing::Span::none()));

    spans
}
