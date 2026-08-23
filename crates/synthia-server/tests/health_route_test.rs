//! Probe endpoint integration tests.
//!
//! Asserts the public surface of the probe endpoints:
//!
//! - `GET /livez` → 200 with `status: "ok"` (liveness).
//! - `GET /readyz` → 200 with `status: "ok"` once the router
//!   bootstrap completed (readiness).
//!
//! These tests run via `tower::ServiceExt::oneshot` against the
//! router created by `create_router` so they cover middleware
//! ordering (auth + trace + cors) as well as the route handler
//! itself.

use std::sync::Arc;

use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use synthia_server::{create_router, state::AppState};
use tower::ServiceExt;

async fn make_app() -> axum::Router {
    let temp = tempfile::TempDir::new().unwrap();
    let session_manager = synthia_session::manager::SessionRegistry::new(
        temp.path().to_path_buf(),
    );
    let state =
        AppState::for_test(session_manager, temp.path().to_path_buf()).await;
    create_router(Arc::new(state)).await
}

#[tokio::test]
async fn test_livez_is_public_and_returns_ok() {
    // Liveness: if the process can serve HTTP, the probe is OK.
    // Must be reachable without an Authorization header, exactly
    // like /readyz, so orchestrator probes never depend on auth.
    let app = make_app().await;

    let req = Request::builder()
        .uri("/livez")
        .method("GET")
        // Intentionally NO Authorization header.
        .body(axum::body::Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "/livez must always answer 200 while the process serves HTTP"
    );

    let body_bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(body["status"], "ok", "got: {body}");
}

#[tokio::test]
async fn test_readyz_is_public_and_ready_after_router_bootstrap() {
    // Readiness: `create_router` finishes bootstrap before
    // returning, so a router-built app must report ready on
    // the first probe.
    let app = make_app().await;

    let req = Request::builder()
        .uri("/readyz")
        .method("GET")
        // Intentionally NO Authorization header.
        .body(axum::body::Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "/readyz must be 200 once the router bootstrap completed"
    );

    let body_bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(body["status"], "ok", "got: {body}");
}
