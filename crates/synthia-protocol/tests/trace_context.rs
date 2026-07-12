use synthia_protocol::W3cTraceContext;

#[test]
fn invalid_context_is_marker() {
    let ctx = W3cTraceContext::invalid();
    assert!(ctx.is_invalid());
}

#[test]
fn real_context_is_not_invalid() {
    let ctx = W3cTraceContext {
        traceparent: "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01"
            .to_string(),
        tracestate: None,
    };
    assert!(!ctx.is_invalid());
}

#[cfg(feature = "otel")]
#[test]
fn from_current_otel_requires_active_span() {
    let ctx = W3cTraceContext::from_current_otel();
    assert!(ctx.is_none(), "expected None when no OTel span is active");
}

#[cfg(feature = "otel")]
#[test]
fn from_span_context_preserves_tracestate() {
    use opentelemetry::trace::{
        SpanContext,
        SpanId,
        TraceFlags,
        TraceId,
        TraceState,
    };

    let ts = TraceState::from_key_value([("vendor", "value")])
        .expect("valid tracestate");
    let trace_id = TraceId::from_hex("0af7651916cd43dd8448eb211c80319c")
        .expect("valid trace id");
    let span_id = SpanId::from_hex("b7ad6b7169203331").expect("valid span id");
    let sc = SpanContext::new(trace_id, span_id, TraceFlags::SAMPLED, true, ts);

    let ctx =
        W3cTraceContext::from_span_context(&sc).expect("valid span context");
    assert_eq!(
        ctx.tracestate.as_deref(),
        Some("vendor=value"),
        "vendor tracestate must survive the conversion (P8)"
    );
}

#[cfg(feature = "otel")]
#[test]
fn from_span_context_drops_empty_tracestate() {
    use opentelemetry::trace::{
        SpanContext,
        SpanId,
        TraceFlags,
        TraceId,
        TraceState,
    };

    let trace_id = TraceId::from_hex("0af7651916cd43dd8448eb211c80319c")
        .expect("valid trace id");
    let span_id = SpanId::from_hex("b7ad6b7169203331").expect("valid span id");
    let sc = SpanContext::new(
        trace_id,
        span_id,
        TraceFlags::SAMPLED,
        true,
        TraceState::default(),
    );

    let ctx =
        W3cTraceContext::from_span_context(&sc).expect("valid span context");
    assert_eq!(ctx.tracestate, None);
}

#[cfg(feature = "otel")]
#[test]
fn from_span_context_rejects_invalid() {
    use opentelemetry::trace::SpanContext;
    let sc = SpanContext::empty_context();
    assert!(W3cTraceContext::from_span_context(&sc).is_none());
}
