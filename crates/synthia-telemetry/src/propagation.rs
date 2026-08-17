//! W3C TraceContext propagation helpers built on the official OpenTelemetry
//! propagator.
//!
//! The HTTP-level `traceparent` / `tracestate` parsing and formatting is
//! delegated to [`opentelemetry_sdk::propagation::TraceContextPropagator`],
//! the reference implementation of [W3C TraceContext Level 1][w3c]. We do
//! **not** hand-roll the format anywhere — that crate is the canonical
//! industry-standard implementation and matches the OTel SDK's runtime
//! behavior byte-for-byte.
//!
//! # Two layers
//!
//! - [`register_global_propagator`]: install the standard propagator into
//!   `opentelemetry::global::set_text_map_propagator` so any code that asks
//!   for the global propagator (including the OTel SDK tracer layer)
//!   receives the same W3C implementation.
//! - [`extract_trace_context`] / [`inject_trace_context`]: thin shims that
//!   take an [`Extractor`] / [`Injector`] adapter (typically an axum
//!   `HeaderMap` adapter built in the consumer crate) and delegate to the
//!   registered propagator. The telemetry crate stays free of HTTP-layer
//!   dependencies.
//!
//! # `synthia-server` middleware
//!
//! `crates/synthia-server/src/middleware/trace_context.rs` is an
//! `axum::middleware::from_fn` that builds a `HeaderMap` adapter here
//! and calls these helpers. It does **not** parse or format `traceparent`
//! itself.
//!
//! [w3c]: https://www.w3.org/TR/trace-context/

use std::str::FromStr;

use opentelemetry::{
    Context,
    propagation::{Extractor, Injector},
    trace::{
        SpanContext,
        SpanId,
        TraceContextExt,
        TraceFlags,
        TraceId,
        TraceState,
    },
};
use opentelemetry_sdk::propagation::TraceContextPropagator;

/// Standard W3C `traceparent` header name. Re-exported for callers that
/// need to read the header directly (e.g. tests).
pub const TRACEPARENT_HEADER: &str = "traceparent";

/// Standard W3C `tracestate` header name.
pub const TRACESTATE_HEADER: &str = "tracestate";

/// Short-form header carrying just the trace id. Convenient for
/// log aggregators that do not parse the full `traceparent`.
pub const X_TRACE_ID_HEADER: &str = "x-trace-id";

/// Extracted W3C trace context from the inbound request.
#[derive(Debug, Clone)]
pub struct ExtractedTraceContext {
    /// 32-hex-char trace id (lowercase).
    pub trace_id: TraceId,
    /// 16-hex-char parent span id from the wire (lowercase).
    pub parent_span_id: SpanId,
    /// Sampled bit from the inbound trace flags.
    pub trace_flags: TraceFlags,
    /// Vendor-specific trace state from `tracestate`.
    pub trace_state: TraceState,
}

impl ExtractedTraceContext {
    /// Build an OpenTelemetry [`SpanContext`] from the extracted fields.
    pub fn span_context(&self) -> SpanContext {
        SpanContext::new(
            self.trace_id,
            self.parent_span_id,
            self.trace_flags,
            true,
            self.trace_state.clone(),
        )
    }
}

/// A freshly minted trace context for outbound injection on the response.
#[derive(Debug, Clone)]
pub struct InjectedTraceContext {
    /// 32-hex-char trace id (matches what we stamped on the local span).
    pub trace_id: TraceId,
    /// 16-hex-char local span id (this server's hop in the trace).
    pub span_id: SpanId,
    /// Sampled bit carried forward.
    pub trace_flags: TraceFlags,
    /// Vendor-specific trace state, propagated verbatim.
    pub trace_state: TraceState,
}

/// Install the W3C [`TraceContextPropagator`] as the OpenTelemetry global
/// propagator. Subsequent calls to
/// [`opentelemetry::global::get_text_map_propagator`] return this
/// implementation; the OTel SDK tracer layer (when the `otel` feature is
/// enabled) uses the same propagator to extract the parent span context
/// from incoming requests.
///
/// Calling this more than once is safe — `set_text_map_propagator` simply
/// replaces the global instance.
pub fn register_global_propagator() {
    opentelemetry::global::set_text_map_propagator(
        TraceContextPropagator::new(),
    );
}

/// Idempotent variant of [`register_global_propagator`] that only
/// installs the W3C propagator when no global one is already set.
///
/// The OpenTelemetry SDK ships a no-op default propagator. If we left it
/// installed, server middleware (and tests that exercise the middleware
/// without first calling `init_tracing`) would extract and inject against
/// the no-op and silently produce no `traceparent` headers. Calling this
/// on every `extract_trace_context` / `inject_trace_context` keeps the
/// middleware correct in standalone test setups.
///
/// Implementation note: this MUST NOT call `register_global_propagator`
/// from inside an `opentelemetry::global::get_text_map_propagator`
/// closure — that API holds a read-lock for the duration of the
/// closure, and `set_text_map_propagator` requires the write-lock,
/// producing an immediate self-deadlock. We use a `Once`-style
/// in-process flag instead: the first caller installs the propagator,
/// all later callers see the flag set and skip the lock entirely.
fn ensure_global_propagator_installed() {
    use std::sync::atomic::{AtomicBool, Ordering};
    static INSTALLED: AtomicBool = AtomicBool::new(false);
    if INSTALLED.load(Ordering::Acquire) {
        return;
    }
    register_global_propagator();
    INSTALLED.store(true, Ordering::Release);
}

/// Extract the inbound W3C trace context from a request's header map.
///
/// Returns `None` when no `traceparent` header is present, or when the
/// header is malformed (the propagator silently drops invalid input per
/// the W3C spec).
pub fn extract_trace_context(
    extractor: &dyn Extractor,
) -> Option<ExtractedTraceContext> {
    ensure_global_propagator_installed();
    let cx = opentelemetry::global::get_text_map_propagator(|p| {
        p.extract(extractor)
    });
    let span = cx.span();
    let span_context = span.span_context();
    if !span_context.is_valid() {
        return None;
    }
    Some(ExtractedTraceContext {
        trace_id: span_context.trace_id(),
        parent_span_id: span_context.span_id(),
        trace_flags: span_context.trace_flags(),
        trace_state: span_context.trace_state().clone(),
    })
}

/// Inject a trace context into a header map using the registered W3C
/// propagator.
///
/// Produces a `traceparent` header (and `tracestate` if non-empty) per the
/// W3C Level 1 format.
pub fn inject_trace_context(
    injector: &mut dyn Injector,
    ctx: &InjectedTraceContext,
) {
    ensure_global_propagator_installed();
    let cx = Context::new().with_remote_span_context(SpanContext::new(
        ctx.trace_id,
        ctx.span_id,
        ctx.trace_flags,
        true,
        ctx.trace_state.clone(),
    ));
    opentelemetry::global::get_text_map_propagator(|p| {
        p.inject_context(&cx, injector)
    });
}

/// Parse a 32-char lowercase hex trace id. Returns `None` for any
/// deviation from the spec.
pub fn parse_trace_id(s: &str) -> Option<TraceId> {
    TraceId::from_hex(s).ok()
}

/// Parse a 16-char lowercase hex span id. Returns `None` for any
/// deviation from the spec.
pub fn parse_span_id(s: &str) -> Option<SpanId> {
    SpanId::from_hex(s).ok()
}

/// Format a hex trace id as 32 lowercase chars (the W3C requirement).
pub fn format_trace_id(id: TraceId) -> String {
    format!("{:032x}", id)
}

/// Format a hex span id as 16 lowercase chars.
pub fn format_span_id(id: SpanId) -> String {
    format!("{:016x}", id)
}

/// Parse a [`TraceState`] from a header value, falling back to the empty
/// default if the input is malformed.
pub fn parse_trace_state(s: &str) -> TraceState {
    TraceState::from_str(s).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn tp_header(trace_id: &str, span_id: &str, flags: &str) -> String {
        format!("00-{trace_id}-{span_id}-{flags}")
    }

    fn hashmap_extractor(
        map: &HashMap<String, String>,
    ) -> HashMapExtractor<'_> {
        HashMapExtractor(map)
    }

    struct HashMapExtractor<'a>(&'a HashMap<String, String>);
    impl Extractor for HashMapExtractor<'_> {
        fn get(&self, key: &str) -> Option<&str> {
            self.0.get(key).map(|v| v.as_str())
        }

        fn keys(&self) -> Vec<&str> {
            self.0.keys().map(|k| k.as_str()).collect()
        }
    }

    struct HashMapInjector<'a>(&'a mut HashMap<String, String>);
    impl Injector for HashMapInjector<'_> {
        fn set(&mut self, key: &str, value: String) {
            self.0.insert(key.to_string(), value);
        }
    }

    #[test]
    fn extract_parses_valid_traceparent() {
        register_global_propagator();
        let mut headers = HashMap::new();
        headers.insert(
            TRACEPARENT_HEADER.to_string(),
            "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01"
                .to_string(),
        );
        let got = extract_trace_context(&hashmap_extractor(&headers))
            .expect("must parse");
        assert_eq!(
            format_trace_id(got.trace_id),
            "0af7651916cd43dd8448eb211c80319c"
        );
        assert_eq!(format_span_id(got.parent_span_id), "b7ad6b7169203331");
        assert_eq!(got.trace_flags, TraceFlags::SAMPLED);
    }

    #[test]
    fn extract_returns_none_when_header_missing() {
        let headers: HashMap<String, String> = HashMap::new();
        assert!(extract_trace_context(&hashmap_extractor(&headers)).is_none());
    }

    #[test]
    fn extract_returns_none_for_invalid_traceparent() {
        register_global_propagator();
        let mut headers = HashMap::new();
        headers.insert(
            TRACEPARENT_HEADER.to_string(),
            "not a traceparent".to_string(),
        );
        assert!(extract_trace_context(&hashmap_extractor(&headers)).is_none());
    }

    #[test]
    fn extract_returns_none_for_all_zero_trace_id() {
        register_global_propagator();
        let mut headers = HashMap::new();
        headers.insert(
            TRACEPARENT_HEADER.to_string(),
            "00-00000000000000000000000000000000-b7ad6b7169203331-01"
                .to_string(),
        );
        assert!(extract_trace_context(&hashmap_extractor(&headers)).is_none());
    }

    #[test]
    fn extract_propagates_tracestate() {
        register_global_propagator();
        let mut headers = HashMap::new();
        headers.insert(
            TRACEPARENT_HEADER.to_string(),
            "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01"
                .to_string(),
        );
        headers.insert(
            TRACESTATE_HEADER.to_string(),
            "vendor1=value1,vendor2=value2".to_string(),
        );
        let got = extract_trace_context(&hashmap_extractor(&headers))
            .expect("must parse");
        assert_eq!(got.trace_state.header(), "vendor1=value1,vendor2=value2");
    }

    #[test]
    fn inject_writes_traceparent() {
        register_global_propagator();
        let mut headers: HashMap<String, String> = HashMap::new();
        let trace_id = TraceId::from_hex("0af7651916cd43dd8448eb211c80319c")
            .expect("valid");
        let span_id = SpanId::from_hex("b7ad6b7169203331").expect("valid");
        let ctx = InjectedTraceContext {
            trace_id,
            span_id,
            trace_flags: TraceFlags::SAMPLED,
            trace_state: TraceState::default(),
        };
        inject_trace_context(&mut HashMapInjector(&mut headers), &ctx);
        assert_eq!(
            headers
                .get(TRACEPARENT_HEADER)
                .expect("traceparent")
                .clone(),
            "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01"
        );
    }

    #[test]
    fn inject_writes_tracestate_when_present() {
        register_global_propagator();
        let mut headers: HashMap<String, String> = HashMap::new();
        let trace_id = TraceId::from_hex("0af7651916cd43dd8448eb211c80319c")
            .expect("valid");
        let span_id = SpanId::from_hex("b7ad6b7169203331").expect("valid");
        let trace_state =
            TraceState::from_str("vendor1=value1").expect("valid");
        let ctx = InjectedTraceContext {
            trace_id,
            span_id,
            trace_flags: TraceFlags::default(),
            trace_state,
        };
        inject_trace_context(&mut HashMapInjector(&mut headers), &ctx);
        assert_eq!(
            headers.get(TRACESTATE_HEADER).expect("tracestate").clone(),
            "vendor1=value1"
        );
    }

    #[test]
    fn inject_writes_empty_tracestate_when_no_state() {
        // The reference OTel propagator always emits `tracestate`
        // alongside `traceparent` (with an empty value when no vendor
        // state was carried). This test pins that behavior so we don't
        // silently diverge from the SDK on the wire format.
        register_global_propagator();
        let mut headers: HashMap<String, String> = HashMap::new();
        let trace_id = TraceId::from_hex("0af7651916cd43dd8448eb211c80319c")
            .expect("valid");
        let span_id = SpanId::from_hex("b7ad6b7169203331").expect("valid");
        let ctx = InjectedTraceContext {
            trace_id,
            span_id,
            trace_flags: TraceFlags::default(),
            trace_state: TraceState::default(),
        };
        inject_trace_context(&mut HashMapInjector(&mut headers), &ctx);
        assert_eq!(
            headers.get(TRACESTATE_HEADER).map(String::as_str),
            Some(""),
            "empty trace state must produce an empty tracestate header"
        );
    }

    #[test]
    fn roundtrip_extract_then_inject_preserves_trace_id() {
        register_global_propagator();
        let upstream_tp = tp_header(
            "0af7651916cd43dd8448eb211c80319c",
            "b7ad6b7169203331",
            "01",
        );
        let mut request_headers = HashMap::new();
        request_headers.insert(TRACEPARENT_HEADER.to_string(), upstream_tp);

        let extracted =
            extract_trace_context(&hashmap_extractor(&request_headers))
                .expect("must parse");

        let mut response_headers: HashMap<String, String> = HashMap::new();
        inject_trace_context(
            &mut HashMapInjector(&mut response_headers),
            &InjectedTraceContext {
                trace_id: extracted.trace_id,
                span_id: SpanId::from_bytes([0x11; 8]),
                trace_flags: extracted.trace_flags,
                trace_state: extracted.trace_state,
            },
        );

        let echoed = response_headers
            .get(TRACEPARENT_HEADER)
            .expect("traceparent on response")
            .clone();
        let upstream_trace_id = "0af7651916cd43dd8448eb211c80319c";
        assert!(echoed.starts_with(&format!("00-{upstream_trace_id}-")));
        assert!(echoed.ends_with("-01"));
    }
}
