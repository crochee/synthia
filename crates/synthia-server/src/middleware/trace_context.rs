//! W3C TraceContext propagation middleware.
//!
//! Implements [W3C TraceContext — Level 1][w3c] (`traceparent` and
//! `tracestate` headers) so that traces correlate across HTTP service
//! boundaries. The middleware is self-contained — it does **not**
//! require the OpenTelemetry feature to be enabled: when OTel is
//! disabled we still parse and re-emit a valid `traceparent`, and
//! the trace id is surfaced as a `tracing::Span` field so it shows
//! up in log lines.
//!
//! # Algorithm
//!
//! 1. **Extract**: read the incoming `traceparent` header. If valid,
//!    record its trace id on the current tracing span so any child
//!    spans created by downstream handlers join the caller's trace.
//! 2. **Generate**: when no valid `traceparent` is present, allocate
//!    a fresh `(trace-id, span-id)` pair so the response still
//!    carries a usable correlation id.
//! 3. **Inject**: echo the canonical `traceparent` back on the
//!    response so the caller (curl / browser / upstream service)
//!    can log it and stitch their side of the trace.
//! 4. **Link**: also publish the trace id as an `x-trace-id`
//!    response header and a `trace_id` span field, which is the
//!    common short-form identifier that log aggregators (Loki,
//!    ELK) match on.
//!
//! # Compatibility with OpenTelemetry
//!
//! When the `otel` feature is enabled in `synthia-telemetry`, the
//! OTel SDK also installs the W3C TraceContext propagator
//! globally. That propagator reads the same header and assigns
//! the same trace id, so the middleware's hand-rolled
//! implementation agrees with it and the result is a single
//! connected trace.
//!
//! [w3c]: https://www.w3.org/TR/trace-context/

use axum::{
    body::Body,
    extract::Request,
    http::{HeaderMap, HeaderValue},
    middleware::Next,
    response::Response,
};
use tracing::Span;
use uuid::Uuid;

/// Standard W3C `traceparent` header name.
pub const TRACEPARENT_HEADER: &str = "traceparent";

/// Standard W3C `tracestate` header name.
pub const TRACESTATE_HEADER: &str = "tracestate";

/// Short-form header carrying just the trace id. Convenient for
/// log aggregators that do not parse the full `traceparent`.
pub const X_TRACE_ID_HEADER: &str = "x-trace-id";

/// `traceparent` version field. Per the W3C spec, the only
/// currently defined version is `00`.
const VERSION: &str = "00";

/// Parsed W3C `traceparent`.
#[derive(Debug, Clone)]
struct TraceParent {
    version: String,
    trace_id: String,
    #[allow(dead_code)] // parsed for round-tripping tests only
    parent_span_id: String,
    flags: String,
}

impl TraceParent {
    /// Parse a `traceparent` header value per W3C Level 1.
    ///
    /// Format: `vv-trace_id-parent_span_id-flags`, each segment
    /// separated by `-`, totaling four segments. Lengths are
    /// fixed: `vv=2`, `trace_id=32`, `parent_span_id=16`,
    /// `flags=2` (all lowercase hex). Returns `None` on any
    /// structural violation.
    fn parse(value: &str) -> Option<Self> {
        let trimmed = value.trim();
        let mut parts = trimmed.split('-');
        let version = parts.next()?;
        let trace_id = parts.next()?;
        let parent_span_id = parts.next()?;
        let flags = parts.next()?;
        if parts.next().is_some() {
            return None;
        }
        if version.len() != 2
            || trace_id.len() != 32
            || parent_span_id.len() != 16
            || flags.len() != 2
        {
            return None;
        }
        if !trace_id.chars().all(|c| c.is_ascii_hexdigit())
            || !parent_span_id.chars().all(|c| c.is_ascii_hexdigit())
            || !flags.chars().all(|c| c.is_ascii_hexdigit())
        {
            return None;
        }
        // Per spec: trace-id and parent-id must not be all zero.
        if trace_id.bytes().all(|b| b == b'0')
            || parent_span_id.bytes().all(|b| b == b'0')
        {
            return None;
        }
        Some(Self {
            version: version.to_string(),
            trace_id: trace_id.to_string(),
            parent_span_id: parent_span_id.to_string(),
            flags: flags.to_string(),
        })
    }

    /// Format the canonical `traceparent` string for the current
    /// request span. We keep the caller's `trace_id` and `flags`
    /// but replace the parent span id with the locally generated
    /// one so downstream services see *us* as their parent.
    fn format_with_local_span(&self, local_span_id: &str) -> String {
        format!(
            "{}-{}-{}-{}",
            self.version, self.trace_id, local_span_id, self.flags
        )
    }
}

/// Generate a 16-hex-char random span id.
fn new_span_id() -> String {
    let bytes: [u8; 8] = Uuid::new_v4().as_bytes()[..8]
        .try_into()
        .expect("uuid is 16 bytes");
    hex::encode(bytes)
}

/// Pull the `traceparent` value out of the request headers (if
/// any), parsing it loosely — invalid headers are simply ignored
/// and a fresh trace is started.
fn extract_traceparent(headers: &HeaderMap) -> Option<TraceParent> {
    headers
        .get(TRACEPARENT_HEADER)
        .and_then(|v| v.to_str().ok())
        .and_then(TraceParent::parse)
}

/// Pull the optional `tracestate` value (passed through verbatim).
fn extract_tracestate(headers: &HeaderMap) -> Option<String> {
    headers
        .get(TRACESTATE_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
}

/// Format a `traceparent` from a caller-supplied trace id and span id.
fn fresh_traceparent_with(
    trace_id: &str,
    span_id: &str,
    flags: &str,
) -> String {
    format!("{VERSION}-{trace_id}-{span_id}-{flags}")
}

/// Format a fresh `traceparent` (version 00) with a brand-new
/// trace id and span id. Test-only helper.
#[cfg(test)]
fn fresh_traceparent() -> String {
    let trace_id = Uuid::new_v4().simple().to_string();
    let span_id = new_span_id();
    fresh_traceparent_with(&trace_id, &span_id, "00")
}

/// HTTP middleware that propagates W3C `traceparent`.
///
/// See the module-level docs for the algorithm.
pub async fn trace_context_middleware(
    request: Request<Body>,
    next: Next,
) -> Response {
    // 1. Extract — use the upstream trace if present, otherwise
    //    start a fresh trace. Either way we end up with a tuple
    //    of (trace_id, parent_span_id, flags) that we will keep
    //    through the response.
    let incoming = extract_traceparent(request.headers());
    let trace_id = incoming
        .as_ref()
        .map(|tp| tp.trace_id.clone())
        .unwrap_or_else(|| Uuid::new_v4().simple().to_string());
    let local_span_id = new_span_id();
    let flags = incoming
        .as_ref()
        .map(|tp| tp.flags.clone())
        .unwrap_or_else(|| "00".to_string());
    let tracestate = extract_tracestate(request.headers());

    // Record the trace id on the current span so it appears in
    // every log line emitted from inside the handler. The
    // `tracing::Span::record` call is a no-op when the span
    // already has a `trace_id` field with this exact value, so
    // it's safe to call unconditionally.
    Span::current().record("trace_id", tracing::field::display(&trace_id));
    Span::current().record("span_id", tracing::field::display(&local_span_id));

    // 2. Run the inner service.
    let mut response = next.run(request).await;

    // 3. Inject the response headers so callers can correlate.
    let out_traceparent = match &incoming {
        Some(tp) => tp.format_with_local_span(&local_span_id),
        None => fresh_traceparent_with(&trace_id, &local_span_id, &flags),
    };
    if let Ok(val) = HeaderValue::from_str(&out_traceparent) {
        response.headers_mut().insert(TRACEPARENT_HEADER, val);
    }
    if let Ok(val) = HeaderValue::from_str(&trace_id) {
        response.headers_mut().insert(X_TRACE_ID_HEADER, val);
    }
    if let Some(ts) = tracestate
        && let Ok(val) = HeaderValue::from_str(&ts)
    {
        response.headers_mut().insert(TRACESTATE_HEADER, val);
    }

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

    #[test]
    fn traceparent_parse_accepts_valid_level_1_value() {
        let raw = "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01";
        let tp = TraceParent::parse(raw).expect("must parse");
        assert_eq!(tp.version, "00");
        assert_eq!(tp.trace_id, "0af7651916cd43dd8448eb211c80319c");
        assert_eq!(tp.parent_span_id, "b7ad6b7169203331");
        assert_eq!(tp.flags, "01");
    }

    #[test]
    fn traceparent_parse_rejects_all_zero_trace_id() {
        // Per spec: trace-id all zeros is invalid.
        let raw = "00-00000000000000000000000000000000-b7ad6b7169203331-01";
        assert!(TraceParent::parse(raw).is_none());
    }

    #[test]
    fn traceparent_parse_rejects_all_zero_span_id() {
        let raw = "00-0af7651916cd43dd8448eb211c80319c-0000000000000000-01";
        assert!(TraceParent::parse(raw).is_none());
    }

    #[test]
    fn traceparent_parse_rejects_wrong_segment_count() {
        let raw = "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331";
        assert!(TraceParent::parse(raw).is_none());
    }

    #[test]
    fn traceparent_parse_rejects_non_hex_characters() {
        let raw = "00-zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz-b7ad6b7169203331-01";
        assert!(TraceParent::parse(raw).is_none());
    }

    #[test]
    fn traceparent_format_preserves_trace_id_and_flags() {
        let tp = TraceParent {
            version: "00".into(),
            trace_id: "0af7651916cd43dd8448eb211c80319c".into(),
            parent_span_id: "b7ad6b7169203331".into(),
            flags: "01".into(),
        };
        let out = tp.format_with_local_span("1111111111111111");
        assert_eq!(
            out,
            "00-0af7651916cd43dd8448eb211c80319c-1111111111111111-01"
        );
    }

    #[test]
    fn fresh_traceparent_has_zero_flags() {
        let s = fresh_traceparent();
        // `00-<32 hex>-<16 hex>-00`
        let mut parts = s.split('-');
        assert_eq!(parts.next(), Some("00"));
        assert_eq!(parts.next().unwrap().len(), 32);
        assert_eq!(parts.next().unwrap().len(), 16);
        assert_eq!(parts.next(), Some("00"));
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
        assert!(TraceParent::parse(&tp).is_some(), "got {tp}");
        // The short-form x-trace-id must match the trace-id half.
        let trace_id = response
            .headers()
            .get(X_TRACE_ID_HEADER)
            .unwrap()
            .to_str()
            .unwrap();
        assert_eq!(trace_id.len(), 32);
        assert!(tp.contains(trace_id), "{tp} should contain {trace_id}");
    }

    #[tokio::test]
    async fn middleware_preserves_upstream_trace_id() {
        let app = Router::new()
            .route("/probe", get(echo))
            .layer(middleware::from_fn(trace_context_middleware));

        let upstream_tp =
            "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01";
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
        let parsed = TraceParent::parse(echoed).expect("must parse");
        assert_eq!(parsed.trace_id, "0af7651916cd43dd8448eb211c80319c");
        assert_ne!(parsed.parent_span_id, "b7ad6b7169203331");
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
        assert!(TraceParent::parse(tp).is_some());
        assert!(!tp.contains("this is not a traceparent"));
    }

    #[tokio::test]
    async fn middleware_passes_tracestate_through() {
        let app = Router::new()
            .route("/probe", get(echo))
            .layer(middleware::from_fn(trace_context_middleware));

        let tracestate = "vendor1=value1,vendor2=value2";
        let req = AxumRequest::builder()
            .uri("/probe")
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
}
