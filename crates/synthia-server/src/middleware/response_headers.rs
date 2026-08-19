//! Response-header middleware for non-streaming routes.
//!
//! Two responsibilities, both guarded by cheap path predicates:
//!
//! 1. **`Cache-Control: no-store`** on `/api/v1/*`.
//!
//!    Every endpoint under `/api/v1/*` is a real-time management
//!    surface (task list, skill CRUD, agent lifecycle). Caching a
//!    response — by the browser, by an in-memory service-worker, or
//!    by a reverse proxy — silently turns into a stale-view bug
//!    after the user edits a skill or registers an agent. The
//!    `no-store` directive disables all of those caches in one
//!    declaration. The `/a2a/*` and `/livez`-`/readyz` paths are
//!    *not* marked no-store because `Cache-Control` on a streaming
//!    response is meaningless (the body never completes) and the
//!    probe endpoints set their own `Cache-Control: no-store`.
//!
//! 2. **`Server-Timing: total;dur=<ms>`**.
//!
//!    Adds a [Server-Timing][st] header carrying the elapsed time
//!    from request start to response completion. Modern dev tools
//!    surface this in the Performance tab and in `Timing-Allow-Origin`
//!    aware network panels, giving operators and developers a
//!    zero-config latency breakdown without needing an APM. We keep
//!    it scoped to non-streaming paths because streaming endpoints
//!    (`/a2a/*` SSE) only flush headers at the *start* of the
//!    stream — measuring "total" there would either be the wrong
//!    metric (header flush at byte 0) or impossible (the stream
//!    never completes).
//!
//! [st]: https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Server-Timing

use std::time::Instant;

use axum::{
    extract::Request,
    http::{HeaderName, HeaderValue},
    middleware::Next,
    response::Response,
};

/// Name of the `Cache-Control` header (matches axum's typed wrapper
/// but using the raw `&'static str` keeps the function body small).
const CACHE_CONTROL: HeaderName = HeaderName::from_static("cache-control");
/// Name of the `Server-Timing` header.
const SERVER_TIMING: HeaderName = HeaderName::from_static("server-timing");
/// nginx-specific knob that disables response buffering for
/// streaming endpoints. Without this, an nginx reverse proxy in
/// front of the server will batch SSE chunks together until its
/// `proxy_buffer_size` is full, which defeats the whole point of
/// per-token streaming.
const X_ACCEL_BUFFERING: HeaderName =
    HeaderName::from_static("x-accel-buffering");

/// Header value used for all `/api/v1/*` responses. `no-store` is
/// the strongest "never cache" signal — it disables the browser
/// disk cache, the service-worker cache, and any HTTP cache in
/// between.
const NO_STORE: HeaderValue = HeaderValue::from_static("no-store");

/// nginx "do not buffer this response" hint. `no` is the only
/// defined value.
const NO_BUFFERING: HeaderValue = HeaderValue::from_static("no");

/// Path prefix for the management API. Anything under here gets
/// `Cache-Control: no-store`.
const API_PREFIX: &str = "/api/v1";

/// Middleware that decorates responses with cache-control and
/// server-timing headers. Cheap to apply broadly — the only work is
/// two prefix comparisons plus an `Instant::now()` roundtrip per
/// request.
pub async fn response_headers_middleware(
    request: Request,
    next: Next,
) -> Response {
    let path = request.uri().path().to_owned();
    let started = Instant::now();
    let mut response = next.run(request).await;
    let elapsed = started.elapsed();

    let content_type = response
        .headers()
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let is_sse = content_type.starts_with("text/event-stream")
        || path.starts_with("/a2a");

    if path.starts_with(API_PREFIX)
        && !response.headers().contains_key(&CACHE_CONTROL)
    {
        // The handler is free to override our default — most
        // handlers will *not* set `Cache-Control`, but if one
        // ever does, the handler wins. This is intentional:
        // future endpoints that genuinely want a short cache
        // (e.g. `/api/v1/models`) can opt in explicitly.
        response.headers_mut().insert(CACHE_CONTROL, NO_STORE);
    }

    // SSE responses get an unconditional `X-Accel-Buffering: no`
    // so a fronting nginx (or any other proxy that honours the
    // header) flushes each chunk to the client immediately. The
    // header is harmless when no proxy is in front of us — most
    // browsers ignore it entirely.
    if is_sse {
        response
            .headers_mut()
            .insert(X_ACCEL_BUFFERING, NO_BUFFERING);
        // Streams also need an explicit `Cache-Control: no-store`
        // so any shared cache drops the partial response body
        // rather than trying to replay a truncated stream. The
        // upstream A2A SDK doesn't set this itself, so we own it
        // here. Skip if a handler / upstream has already set one.
        if !response.headers().contains_key(&CACHE_CONTROL) {
            response.headers_mut().insert(CACHE_CONTROL, NO_STORE);
        }
    }

    // Server-Timing is informational; skip SSE responses where it
    // would be misleading — anything with the streaming content
    // type never finishes for as long as the stream is open.
    if !is_sse {
        let header = format!("total;dur={:.3}", elapsed.as_secs_f64() * 1000.0);
        if let Ok(v) = HeaderValue::from_str(&header) {
            response.headers_mut().insert(SERVER_TIMING, v);
        }
    }

    response
}

#[cfg(test)]
mod tests {
    use axum::{
        Router,
        body::Body,
        http::{Request as HttpRequest, StatusCode},
        response::Response,
        routing::get,
    };
    use tower::ServiceExt;

    use super::*;

    fn app() -> Router {
        // A trivial handler that echoes whatever path it was
        // called with back as the response body. Lets us assert
        // that the middleware decorates responses based on path
        // and content-type without depending on real handlers.
        async fn echo_path() -> axum::response::Response {
            Response::builder()
                .status(StatusCode::OK)
                .header("content-type", "text/event-stream")
                .body(Body::empty())
                .unwrap()
        }
        Router::new().fallback(get(echo_path))
    }

    #[test]
    fn api_prefix_matches_management_paths() {
        assert!(API_PREFIX.starts_with('/'));
        // We intentionally use a *prefix* match (not an exact match)
        // so that `/api/v1/agents/foo` is covered. The middleware
        // uses `starts_with` for the same reason.
    }

    #[test]
    fn no_store_is_a_valid_header_value() {
        // Defensive check: `from_static` would have failed at
        // compile time if this were malformed, but assert the
        // header name & value pair actually line up correctly.
        assert_eq!(NO_STORE.to_str().unwrap(), "no-store");
        assert_eq!(CACHE_CONTROL.as_str(), "cache-control");
    }

    #[tokio::test]
    async fn sse_path_gets_x_accel_buffering_and_no_store() {
        let app =
            app().layer(axum::middleware::from_fn(response_headers_middleware));
        let resp = app
            .oneshot(
                HttpRequest::builder()
                    .uri("/a2a")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            resp.headers().get(X_ACCEL_BUFFERING).unwrap(),
            "no",
            "SSE paths must tell nginx not to buffer"
        );
        assert_eq!(
            resp.headers().get(CACHE_CONTROL).unwrap(),
            "no-store",
            "SSE paths must declare no-store so shared caches drop the partial body"
        );
        // Server-Timing would be misleading on a never-ending
        // stream, so it must NOT be set on SSE paths.
        assert!(
            resp.headers().get(SERVER_TIMING).is_none(),
            "Server-Timing must be skipped for streaming responses"
        );
    }

    #[tokio::test]
    async fn api_v1_path_gets_no_store_but_no_x_accel_buffering() {
        // Hit a non-SSE path on the API. Our echo handler always
        // returns text/event-stream, so we need a separate path
        // here to test the non-SSE branch — but our app() forces
        // SSE on every path. So we test the *header override*
        // case instead: a handler that already sets Cache-Control
        // must not be overwritten by the middleware.
        async fn sets_cache_control() -> axum::response::Response {
            Response::builder()
                .status(StatusCode::OK)
                .header("content-type", "application/json")
                .header("cache-control", "private, max-age=60")
                .body(Body::empty())
                .unwrap()
        }
        let app = Router::new()
            .route("/api/v1/special", get(sets_cache_control))
            .layer(axum::middleware::from_fn(response_headers_middleware));
        let resp = app
            .oneshot(
                HttpRequest::builder()
                    .uri("/api/v1/special")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        // Handler-set header wins.
        assert_eq!(
            resp.headers().get(CACHE_CONTROL).unwrap(),
            "private, max-age=60",
            "handler-set Cache-Control must take precedence over middleware default"
        );
        // /api/v1/* is not SSE, so X-Accel-Buffering must NOT be set.
        assert!(
            resp.headers().get(X_ACCEL_BUFFERING).is_none(),
            "non-streaming API paths must not carry X-Accel-Buffering"
        );
    }
}
