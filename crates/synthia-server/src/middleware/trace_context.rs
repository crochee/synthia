//! W3C TraceContext propagation middleware.
//!
//! Extracts the inbound `traceparent` header, stamps the trace id on the
//! local `tracing::Span` so it appears on every log line emitted inside
//! the handler, and echoes back a fresh `traceparent` (with our locally
//! minted span id) on the response. The middleware does **not** hand-roll
//! the wire format — extraction / injection is delegated to the
//! [`opentelemetry_sdk::propagation::TraceContextPropagator`] reference
//! implementation, the canonical industry-standard implementation of
//! [W3C TraceContext Level 1][w3c].
//!
//! # tracestate without traceparent
//!
//! W3C `tracestate` is a sibling header that may legitimately travel
//! without a `traceparent` (the spec is permissive — many
//! implementations attach `tracestate` to the *previous* trace context
//! when the new request omits one). When the propagator cannot anchor
//! a span (no upstream `traceparent`) we still carry the inbound
//! `tracestate` through verbatim if one is present, so vendor
//! metadata survives across the hop.
//!
//! # Why delegate
//!
//! Hand-rolling the W3C format is error-prone (forbidden all-zero ids,
//! lowercase hex, exact segment lengths, etc.) and creates drift with
//! the OTel SDK tracer layer. The format work lives in
//! [`synthia::telemetry::propagation`] — extraction, injection, the
//! standard header names, and id formatting — and this module owns only
//! what is HTTP-layer: the `HeaderMap` adapters, the axum middleware,
//! and minting this hop's trace/span id. It used to carry its own copy
//! of all of it, which is exactly the drift the shared implementation
//! exists to prevent.
//!
//! [w3c]: https://www.w3.org/TR/trace-context/

use std::str::FromStr;

use axum::{
    body::Body,
    extract::Request,
    http::{HeaderMap, HeaderName, HeaderValue},
    middleware::Next,
    response::Response,
};
use opentelemetry::{
    propagation::{Extractor, Injector},
    trace::{SpanId, TraceFlags, TraceId, TraceState},
};
#[cfg(test)]
use synthia::telemetry::TRACEPARENT_HEADER;
use synthia::telemetry::{
    InjectedTraceContext,
    TRACESTATE_HEADER,
    extract_trace_context,
    format_span_id,
    format_trace_id,
    inject_trace_context,
};
use tracing::Span;
use uuid::Uuid;

/// `axum::http::HeaderMap` adapter that implements
/// [`opentelemetry::propagation::Extractor`] for the inbound request.
///
/// The W3C format itself is parsed by
/// [`synthia::telemetry::extract_trace_context`]; this is only the
/// HTTP-layer adapter, which is why it lives here and not in the
/// telemetry crate (that crate stays free of HTTP dependencies).
struct HeaderMapExtractor<'a>(&'a HeaderMap);

impl Extractor for HeaderMapExtractor<'_> {
    fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).and_then(|v| v.to_str().ok())
    }

    fn keys(&self) -> Vec<&str> {
        self.0.keys().map(|name| name.as_str()).collect()
    }
}

/// `axum::http::HeaderMap` adapter that implements
/// [`opentelemetry::propagation::Injector`] for the outbound response.
struct HeaderMapInjector<'a>(&'a mut HeaderMap);

impl Injector for HeaderMapInjector<'_> {
    fn set(&mut self, key: &str, value: String) {
        if let (Ok(name), Ok(val)) = (
            HeaderName::from_bytes(key.as_bytes()),
            HeaderValue::from_str(&value),
        ) {
            self.0.insert(name, val);
        }
    }
}

/// Generate a 16-hex-char random span id. Uses the first 8 bytes of a
/// UUID v4 — high entropy, fast, no extra deps.
fn new_span_id() -> SpanId {
    let bytes: [u8; 8] = Uuid::new_v4().as_bytes()[..8]
        .try_into()
        .expect("uuid is 16 bytes");
    SpanId::from_bytes(bytes)
}

/// Generate a 32-hex-char random trace id from a UUID v4.
fn new_trace_id() -> TraceId {
    let bytes: [u8; 16] = *Uuid::new_v4().as_bytes();
    TraceId::from_bytes(bytes)
}

/// HTTP middleware that propagates W3C `traceparent`.
///
/// See the module-level docs for the algorithm.
pub async fn trace_context_middleware(
    request: Request<Body>,
    next: Next,
) -> Response {
    // 1. Extract — delegate to the registered W3C propagator.
    let extracted =
        extract_trace_context(&HeaderMapExtractor(request.headers()));

    // W3C `tracestate` is a sibling of `traceparent` and MAY travel
    // alone (the spec is permissive — many implementations attach
    // `tracestate` to the previous traceparent even when the new
    // request omits one). Read it directly from the headers so it
    // survives even when the OTel propagator can't anchor it to a
    // valid span context.
    let raw_tracestate = request
        .headers()
        .get(TRACESTATE_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);

    // The trace id we'll publish on the response: the upstream trace
    // id when present, or a freshly minted one. The local span id is
    // always freshly minted so downstream services see *us* as their
    // parent.
    let (trace_id, trace_flags, trace_state) = match extracted {
        Some(e) => (e.trace_id, e.trace_flags, e.trace_state),
        None => {
            // No upstream traceparent — mint one, but if the caller
            // supplied a bare `tracestate`, carry it through so
            // downstream observability tooling still sees vendor
            // metadata.
            let ts = raw_tracestate
                .as_deref()
                .and_then(|s| TraceState::from_str(s).ok())
                .unwrap_or_default();
            (new_trace_id(), TraceFlags::default(), ts)
        }
    };
    let local_span_id = new_span_id();

    // Stamp the trace id on the current span so it appears on every
    // log line emitted from inside the handler. `Span::record` is a
    // no-op when the field is absent or already set with this value.
    Span::current().record(
        "trace_id",
        tracing::field::display(format_trace_id(trace_id)),
    );
    Span::current().record(
        "span_id",
        tracing::field::display(format_span_id(local_span_id)),
    );

    // 2. Run the inner service.
    let mut response = next.run(request).await;

    // 3. Inject the response headers so callers can correlate. The
    // propagator writes `traceparent` (and `tracestate` when
    // non-empty) — the W3C headers are the single correlation
    // surface; no parallel short-form header is emitted.
    let headers = response.headers_mut();
    inject_trace_context(
        &mut HeaderMapInjector(headers),
        &InjectedTraceContext {
            trace_id,
            span_id: local_span_id,
            trace_flags,
            trace_state,
        },
    );

    response
}

#[cfg(test)]
mod tests {
    use axum::{
        Router,
        body::Body,
        http::{Request as AxumRequest, StatusCode},
        middleware,
        routing::get,
    };
    use tower::ServiceExt;

    use super::*;

    async fn echo() -> &'static str {
        "ok"
    }

    #[tokio::test]
    async fn middleware_generates_traceparent_when_absent() {
        let app = Router::new()
            .route("/probe", get(echo))
            .layer(middleware::from_fn(trace_context_middleware));

        let req = AxumRequest::builder()
            .uri("/probe")
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let tp = response
            .headers()
            .get(TRACEPARENT_HEADER)
            .expect("traceparent must be present")
            .to_str()
            .unwrap()
            .to_string();
        // Must parse as a valid W3C traceparent via the same propagator
        // we delegate to. Re-derive an Extractor and call
        // extract_trace_context to confirm round-trip validity.
        let parsed =
            extract_trace_context(&HeaderMapExtractor(response.headers()))
                .expect("the response traceparent must parse");
        assert_eq!(format_trace_id(parsed.trace_id), parse_tp_trace_id(&tp));

        // The legacy short-form `x-trace-id` header has been retired —
        // `traceparent` is the single correlation surface. Guard that
        // it does not silently come back.
        assert!(
            response.headers().get("x-trace-id").is_none(),
            "x-trace-id must not be emitted; use traceparent instead"
        );
    }

    #[tokio::test]
    async fn middleware_preserves_upstream_trace_id() {
        let app = Router::new()
            .route("/probe", get(echo))
            .layer(middleware::from_fn(trace_context_middleware));

        let upstream_trace_id = "0af7651916cd43dd8448eb211c80319c";
        let upstream_tp = format!("00-{upstream_trace_id}-b7ad6b7169203331-01");
        let req = AxumRequest::builder()
            .uri("/probe")
            .header(TRACEPARENT_HEADER, upstream_tp)
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(req).await.unwrap();
        let echoed = response
            .headers()
            .get(TRACEPARENT_HEADER)
            .unwrap()
            .to_str()
            .unwrap();
        // The trace id must be preserved; only the parent span id changes.
        assert!(echoed.starts_with(&format!("00-{upstream_trace_id}-")));
        assert_ne!(parse_tp_span_id(echoed), "b7ad6b7169203331");
    }

    #[tokio::test]
    async fn middleware_recovers_from_invalid_traceparent() {
        let app = Router::new()
            .route("/probe", get(echo))
            .layer(middleware::from_fn(trace_context_middleware));

        let req = AxumRequest::builder()
            .uri("/probe")
            .header(TRACEPARENT_HEADER, "this is not a traceparent")
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(req).await.unwrap();
        // We must not propagate the malformed value — instead the
        // middleware should mint a fresh trace.
        let tp = response
            .headers()
            .get(TRACEPARENT_HEADER)
            .unwrap()
            .to_str()
            .unwrap();
        assert!(!tp.contains("this is not a traceparent"));
        // Must still be parseable by the same propagator.
        let parsed =
            extract_trace_context(&HeaderMapExtractor(response.headers()))
                .expect("response traceparent must parse");
        assert_ne!(
            format_trace_id(parsed.trace_id),
            "this is not a traceparent"
        );
    }

    #[tokio::test]
    async fn middleware_passes_tracestate_through() {
        let app = Router::new()
            .route("/probe", get(echo))
            .layer(middleware::from_fn(trace_context_middleware));

        // Per W3C TraceContext Level 1, `tracestate` is a sibling of
        // `traceparent` — both are required to form a valid inbound
        // trace context. The OTel SDK's reference propagator requires
        // `traceparent` to extract `tracestate`, so we send both.
        let upstream_trace_id = "0af7651916cd43dd8448eb211c80319c";
        let tracestate = "vendor1=value1,vendor2=value2";
        let upstream_tp = format!("00-{upstream_trace_id}-b7ad6b7169203331-01");
        let req = AxumRequest::builder()
            .uri("/probe")
            .header(TRACEPARENT_HEADER, upstream_tp)
            .header(TRACESTATE_HEADER, tracestate)
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(req).await.unwrap();
        let echoed = response
            .headers()
            .get(TRACESTATE_HEADER)
            .unwrap()
            .to_str()
            .unwrap();
        assert_eq!(echoed, tracestate);
    }

    /// Helper: extract the trace id from a `traceparent` string.
    /// Used by tests to compare against the propagator's parsed output.
    fn parse_tp_trace_id(tp: &str) -> String {
        tp.split('-')
            .nth(1)
            .expect("traceparent has 4 segments")
            .to_string()
    }

    fn parse_tp_span_id(tp: &str) -> String {
        tp.split('-')
            .nth(2)
            .expect("traceparent has 4 segments")
            .to_string()
    }
}
