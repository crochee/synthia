use axum::http::{HeaderMap, Request};
use axum::response::Response;
use opentelemetry::propagation::TextMapPropagator;
use opentelemetry_sdk::propagation::TraceContextPropagator;
use tracing_opentelemetry::OpenTelemetrySpanExt;

const TRACEPARENT_HEADER: &str = "traceparent";

struct HeaderCarrier<'a> {
    headers: &'a HeaderMap,
}

impl<'a> opentelemetry::propagation::Extractor for HeaderCarrier<'a> {
    fn get(&self, key: &str) -> Option<&str> {
        self.headers.get(key).and_then(|v| v.to_str().ok())
    }

    fn keys(&self) -> Vec<&str> {
        self.headers.keys().map(|k| k.as_str()).collect()
    }
}

pub async fn trace_context_middleware(
    request: Request<axum::body::Body>,
    next: axum::middleware::Next,
) -> Response {
    let propagator = TraceContextPropagator::new();
    let carrier = HeaderCarrier {
        headers: request.headers(),
    };

    let parent_context = propagator.extract(&carrier);

    let current_span = tracing::Span::current();
    current_span.set_parent(parent_context);

    let traceparent = request
        .headers()
        .get(TRACEPARENT_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let mut response = next.run(request).await;

    if let Some(tp) = traceparent {
        if let Ok(val) = tp.parse() {
            response.headers_mut().insert(TRACEPARENT_HEADER, val);
        }
    }

    response
}
