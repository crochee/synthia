//! HTTP request tracing middleware.
//!
//! Creates the `http_request` span for every inbound request and
//! stamps two response headers:
//!
//! - `x-request-time-ms` — wall-clock milliseconds spent inside
//!   the inner service. E2E tests use this header to assert
//!   server-side latency targets without including the test
//!   framework's HTTP setup overhead.
//!
//! Correlation is driven entirely by the [W3C TraceContext][w3c]
//! standard (`traceparent` / `tracestate`) handled by
//! [`crate::middleware::trace_context`]. This module does NOT
//! mint a separate `X-Request-ID` — the trace id and span id
//! exposed by `trace_context` are sufficient to stitch logs,
//! traces, and metrics together. The previous
//! `X-Request-ID` header was redundant with `x-trace-id` and
//! created two parallel correlation schemes; it has been removed
//! in favour of the W3C standard.
//!
//! [w3c]: https://www.w3.org/TR/trace-context/

use std::{
    task::{Context, Poll},
    time::Instant,
};

use axum::{
    http::{HeaderValue, Request},
    response::Response,
};
use tower::{Layer, Service};
use tracing::Instrument;

const REQUEST_TIME_HEADER: &str = "x-request-time-ms";

/// HTTP request tracing middleware.
///
/// See module docs for the high-level design.
#[derive(Clone)]
pub struct RequestTracing<S> {
    inner: S,
}

impl<S> RequestTracing<S> {
    pub fn new(inner: S) -> Self {
        Self { inner }
    }
}

impl<S, B> Service<Request<B>> for RequestTracing<S>
where
    S: Service<Request<B>, Response = Response> + Clone + Send + 'static,
    S::Future: Send + 'static,
    B: Send + 'static,
{
    type Error = S::Error;
    type Future = std::pin::Pin<
        Box<
            dyn std::future::Future<
                    Output = Result<Self::Response, Self::Error>,
                > + Send,
        >,
    >;
    type Response = S::Response;

    fn poll_ready(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<B>) -> Self::Future {
        // Create the request span. `trace_id` and `span_id` are
        // declared as `Empty` placeholders here and populated by
        // [`crate::middleware::trace_context::trace_context_middleware`],
        // which runs as an outer middleware (closer to the wire)
        // and calls `Span::current().record(...)` before any
        // handler executes. This ordering guarantees the values
        // are present in the very first log line emitted under
        // this span.
        let span = tracing::info_span!(
            "http_request",
            method = %req.method(),
            uri = %req.uri(),
            trace_id = tracing::field::Empty,
            span_id = tracing::field::Empty,
        );

        let clone = self.inner.clone();
        let mut inner = std::mem::replace(&mut self.inner, clone);

        let fut = async move {
            let started = Instant::now();
            let mut response = inner.call(req).await?;
            let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
            let status = response.status().as_u16();
            if let Ok(value) =
                HeaderValue::from_str(&format!("{elapsed_ms:.3}"))
            {
                response.headers_mut().insert(REQUEST_TIME_HEADER, value);
            }
            // Access log entry: emits inside the http_request span
            // so the `trace_id` / `span_id` fields populated by the
            // outer trace-context middleware show up automatically
            // on every log line. This is the canonical stitch point
            // for logs → traces → metrics.
            tracing::info!(
                status = status,
                elapsed_ms = format!("{elapsed_ms:.3}"),
                "http_response",
            );
            Ok(response)
        };

        Box::pin(fut.instrument(span))
    }
}

/// Tower Layer for RequestTracing
#[derive(Clone)]
pub struct RequestTracingLayer;

impl<S> Layer<S> for RequestTracingLayer {
    type Service = RequestTracing<S>;

    fn layer(&self, inner: S) -> Self::Service {
        RequestTracing::new(inner)
    }
}

#[cfg(test)]
mod tests {
    use axum::{
        Router,
        body::Body,
        http::{Request, StatusCode},
        routing::get,
    };
    use tower::ServiceExt;

    use super::*;

    async fn handler() -> &'static str {
        "ok"
    }

    #[tokio::test]
    async fn request_tracing_sets_timing_header() {
        let app = Router::new()
            .route("/probe", get(handler))
            .layer(RequestTracingLayer);

        let req = Request::builder()
            .uri("/probe")
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let elapsed = response
            .headers()
            .get(REQUEST_TIME_HEADER)
            .expect("x-request-time-ms");
        let ms: f64 = elapsed.to_str().unwrap().parse().unwrap();
        // The handler does nothing, but the wall clock still ticks
        // across the `Instant::now()` boundary; allow up to 50 ms.
        assert!(
            (0.0..50.0).contains(&ms),
            "elapsed should be a small positive number, got {ms}"
        );
    }

    #[tokio::test]
    async fn request_tracing_no_longer_emits_request_id_header() {
        // The legacy X-Request-ID header has been retired in favour
        // of W3C TraceContext (`traceparent` / `x-trace-id`). Make
        // sure it does not silently come back.
        let app = Router::new()
            .route("/probe", get(handler))
            .layer(RequestTracingLayer);

        let req = Request::builder()
            .uri("/probe")
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(req).await.unwrap();
        assert!(
            response.headers().get("X-Request-ID").is_none(),
            "X-Request-ID must not be emitted; use x-trace-id + traceparent instead",
        );
    }
}
