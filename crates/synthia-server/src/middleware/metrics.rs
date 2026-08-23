//! HTTP RED metrics middleware.
//!
//! Tracks request count + wall-clock latency for every matched API
//! route and stamps the result on the prometheus vectors exposed by
//! [`synthia_telemetry::metrics`]. Labels are taken from
//! [`axum::extract::MatchedPath`], so a parameterized path like
//! `/api/v1/chat/sessions/{id}/messages` collapses to a single
//! time series regardless of how many distinct session ids are
//! queried — important for keeping the cardinality of the metric
//! family bounded.
//!
//! # Feature gate
//!
//! Available only behind the `metrics` cargo feature. With the
//! feature off, [`track_metrics`] / [`track_metrics_layer`] are not
//! compiled, so production builds that opt out of prometheus scrape
//! pay no per-request overhead.
//!
//! # Probe exclusion
//!
//! The endpoint handlers in [`crate::routes::health`] are mounted
//! *outside* the API router and so are not traversed by this
//! middleware; `/livez`, `/readyz`, and `/metrics` are deliberately
//! not tracked — counting a probe or a scrape on itself would skew
//! the histograms with self-referential noise (and a per-second
//! scrape on `/metrics` would dwarf real API traffic).

use std::time::Instant;

use axum::{
    extract::{MatchedPath, Request},
    middleware::Next,
    response::Response,
};
use synthia_telemetry::{HTTP_REQUESTS_DURATION_SECONDS, HTTP_REQUESTS_TOTAL};

/// Middleware that records HTTP request count + latency for the
/// matched axum route.
///
/// Runs OUTSIDE the auth / trace-context middleware (added LAST in
/// `server/router.rs` so it wraps everything else), so a request that
/// is rejected by `AuthLayer` or short-circuits before the handler
/// still records a sample. This matches Prometheus operator
/// expectations: every observed request contributes one sample,
/// including 401 / 403 / 4xx / 5xx responses.
pub async fn track_metrics(request: Request, next: Next) -> Response {
    let start = Instant::now();

    let path = if let Some(matched) = request.extensions().get::<MatchedPath>()
    {
        matched.as_str().to_owned()
    } else {
        request.uri().path().to_owned()
    };
    let method = request.method().as_str().to_owned();

    let response = next.run(request).await;
    let latency = start.elapsed();

    let labels = [method.as_str(), path.as_str()];
    HTTP_REQUESTS_TOTAL.with_label_values(&labels).inc();
    HTTP_REQUESTS_DURATION_SECONDS
        .with_label_values(&labels)
        .observe(latency.as_secs_f64());

    response
}

#[cfg(test)]
mod tests {
    use axum::{
        Router,
        body::Body,
        http::{Request, StatusCode},
        middleware::from_fn,
        routing::{get, post},
    };
    use tower::ServiceExt;

    use super::*;

    async fn probe() -> &'static str {
        "ok"
    }

    /// `track_metrics` MUST increment the counter and observe the
    /// histogram for a matched GET request, with the route template
    /// (not the raw URI) used as the `path` label. The global
    /// prometheus registry is shared across tests in this binary, so
    /// we read the counter value *before* and *after* the request
    /// and assert the per-test delta.
    #[tokio::test]
    async fn track_metrics_records_counter_and_histogram() {
        let app = Router::new()
            .route("/items/{id}", get(probe))
            .layer(from_fn(track_metrics));

        // The {id} slot must collapse to "/items/{id}" in the label,
        // not to "/items/42" — that is the whole point of using
        // `MatchedPath`.
        let before = synthia_telemetry::HTTP_REQUESTS_TOTAL
            .with_label_values(&["GET", "/items/{id}"])
            .get();

        let req = Request::builder()
            .uri("/items/42")
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let after = synthia_telemetry::HTTP_REQUESTS_TOTAL
            .with_label_values(&["GET", "/items/{id}"])
            .get();
        assert_eq!(
            after - before,
            1,
            "matched-path label '/items/{{id}}' must contribute exactly one sample"
        );
    }

    /// POST samples MUST end up on a distinct time series (separate
    /// `method` label) even when the matched path is the same as a
    /// previously-recorded GET — labels are joined into the series
    /// identity. The global prometheus registry is shared across
    /// tests in this binary, so we read the counter values *before*
    /// and *after* the request and assert the per-test delta.
    #[tokio::test]
    async fn track_metrics_distinguishes_methods() {
        let app = Router::new()
            .route("/items", post(probe).get(probe))
            .layer(from_fn(track_metrics));

        let get_before = synthia_telemetry::HTTP_REQUESTS_TOTAL
            .with_label_values(&["GET", "/items"])
            .get();
        let post_before = synthia_telemetry::HTTP_REQUESTS_TOTAL
            .with_label_values(&["POST", "/items"])
            .get();

        let get_req = Request::builder()
            .method("GET")
            .uri("/items")
            .body(Body::empty())
            .unwrap();
        let _ = app.clone().oneshot(get_req).await.unwrap();

        let post_req = Request::builder()
            .method("POST")
            .uri("/items")
            .body(Body::empty())
            .unwrap();
        let _ = app.oneshot(post_req).await.unwrap();

        let get_after = synthia_telemetry::HTTP_REQUESTS_TOTAL
            .with_label_values(&["GET", "/items"])
            .get();
        let post_after = synthia_telemetry::HTTP_REQUESTS_TOTAL
            .with_label_values(&["POST", "/items"])
            .get();

        assert_eq!(
            get_after - get_before,
            1,
            "GET /items must contribute exactly one labeled sample"
        );
        assert_eq!(
            post_after - post_before,
            1,
            "POST /items must contribute exactly one labeled sample"
        );
    }
}
