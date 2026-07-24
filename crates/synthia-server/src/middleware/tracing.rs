use std::task::{Context, Poll};

use axum::{
    http::{HeaderValue, Request},
    response::Response,
};
use tower::{Layer, Service};
use uuid::Uuid;

const REQUEST_ID_HEADER: &str = "X-Request-ID";

/// Request tracing middleware that generates a unique Request ID for each request.
///
/// Adds an `X-Request-ID` header to all responses for traceability.
/// The request ID is also propagated as a tracing span attribute.
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
        // Generate a unique request ID
        let request_id = Uuid::new_v4().to_string();

        // Add to tracing span
        let span = tracing::info_span!(
            "http_request",
            method = %req.method(),
            uri = %req.uri(),
            request_id = %request_id,
        );
        span.in_scope(|| {
            tracing::info!("Request started");
        });

        // Add request ID to response headers
        let request_id_header = HeaderValue::from_str(&request_id)
            .unwrap_or_else(|_| HeaderValue::from_static("unknown"));

        let clone = self.inner.clone();
        let mut inner = std::mem::replace(&mut self.inner, clone);

        Box::pin(async move {
            let mut response = inner.call(req).await?;
            response
                .headers_mut()
                .insert(REQUEST_ID_HEADER, request_id_header);
            Ok(response)
        })
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
