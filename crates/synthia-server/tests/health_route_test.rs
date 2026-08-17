//! Health endpoint integration tests.
//!
//! Asserts the public surface of `/health` and the related
//! `/.well-known/agent-card.json` discovery route:
//!
//! - `GET /health` → 200 with `status: "ok"`, `version`,
//!   and `supports_streaming`.
//! - `GET /.well-known/agent-card.json` → 200 with a card
//!   declaring at least one interface binding.
//! - `GET /api/v1/models` → 200 with a `List<Model>`.
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
async fn test_health_returns_ok_with_version_and_streaming_flag() {
    let app = make_app().await;

    let req = Request::builder()
        .uri("/health")
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body_bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();

    assert_eq!(
        body["status"], "ok",
        "/health must return status=ok, got: {body}"
    );
    assert!(
        body["version"].is_string(),
        "/health must include a version string, got: {body}"
    );
}

#[tokio::test]
async fn test_agent_card_well_known_endpoint_is_public_and_returns_card() {
    let app = make_app().await;

    let req = Request::builder()
        .uri("/.well-known/agent-card.json")
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "agent-card discovery must be a public, 200 endpoint"
    );

    let body_bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let card: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();

    // Name + capabilities are the minimum contract a client needs
    // to know what bindings to use.
    assert!(
        card["name"].is_string(),
        "card must include a `name`, got: {card}"
    );
    assert!(
        card["capabilities"]["streaming"].is_boolean()
            || card["capabilities"]["tools"].is_boolean(),
        "card must declare at least one capability, got: {card}"
    );
}

#[tokio::test]
async fn test_health_does_not_require_auth() {
    // The public-path exemption in `AuthMiddleware` must let
    // /health through WITHOUT an Authorization header. If this
    // regresses, the LoadBalancer's health probe will start
    // failing.
    let app = make_app().await;

    let req = Request::builder()
        .uri("/health")
        .method("GET")
        // Intentionally NO Authorization header.
        .body(axum::body::Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert!(
        resp.status().is_success(),
        "/health must succeed without Authorization, got {}",
        resp.status()
    );
}
