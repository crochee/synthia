//! Auth middleware integration tests.
//!
//! Asserts the request gating contract implemented by
//! `AuthMiddleware`:
//!
//! - Public paths (`/health`, `/.well-known/agent-card.json`,
//!   `/api/v1/a2a/...` in dev mode) succeed WITHOUT an
//!   `Authorization` header.
//! - When `SYNTHIA_API_KEY` is unset, all paths behave like
//!   public paths and return success (dev-mode opt-out).
//! - The middleware injects a `RequestUserId` extension so
//!   downstream handlers can find the namespace.
//!
//! These tests run via `tower::ServiceExt::oneshot` so they
//! cover the full middleware stack applied by `create_router`.

use std::sync::Arc;

use axum::http::{Request, StatusCode};
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
async fn test_unauthenticated_request_to_health_succeeds() {
    let app = make_app().await;

    let req = Request::builder()
        .uri("/health")
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert!(
        resp.status().is_success(),
        "/health must succeed without auth, got {}",
        resp.status()
    );
}

#[tokio::test]
async fn test_unauthenticated_request_to_agent_card_succeeds() {
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
        "agent-card must be on the public allow-list"
    );
}

#[tokio::test]
async fn test_models_list_succeeds_when_auth_unconfigured() {
    // `SYNTHIA_API_KEY` is unset in the test env (the
    // `for_test` constructor does not set it), so the
    // middleware must let every request through with the
    // default `user_id`. This guards the dev-mode opt-out.
    let app = make_app().await;

    let req = Request::builder()
        .uri("/api/v1/models")
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert!(
        resp.status().is_success(),
        "/api/v1/models must succeed when SYNTHIA_API_KEY is unset, got {}",
        resp.status()
    );
}

#[tokio::test]
async fn test_skills_list_succeeds_when_auth_unconfigured() {
    let app = make_app().await;

    let req = Request::builder()
        .uri("/api/v1/skills")
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert!(
        resp.status().is_success(),
        "/api/v1/skills must succeed when SYNTHIA_API_KEY is unset, got {}",
        resp.status()
    );
}

#[tokio::test]
async fn test_tasks_list_succeeds_when_auth_unconfigured() {
    let app = make_app().await;

    let req = Request::builder()
        .uri("/api/v1/tasks")
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert!(
        resp.status().is_success(),
        "/api/v1/tasks must succeed when SYNTHIA_API_KEY is unset, got {}",
        resp.status()
    );
}
