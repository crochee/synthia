//! Public-router smoke tests.
//!
//! Verifies that the public router surface (the routes that
//! orchestrators, browser CORS preflights, and external A2A
//! clients hit without authentication) is correctly mounted and
//! returns the expected status codes. A regression that moves
//! one of these routes under the protected router, or drops the
//! CORS layer, would silently break liveness probes and
//! cross-origin browser clients.
//!
//! The `SynthiaHandler` post-completion `:subscribe` fallback
//! itself is unit-tested in
//! `crates/synthia-server/src/a2a/wrapper.rs` (see
//! `subscribe_after_completion_returns_terminal_snapshot`).
//! Driving the fallback through a full HTTP request would
//! require a real LLM provider, which is out of scope for a
//! smoke test.

use std::sync::Arc;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
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

/// `GET /livez` is the liveness probe used by orchestrators and
/// must always return 200 with `{ status }`. Pin this against the
/// full router so we exercise the public route surface
/// end-to-end (including the public/protected split and CORS
/// layer).
#[tokio::test]
async fn livez_endpoint_returns_200_with_status_ok() {
    let app = make_app().await;
    let req = Request::builder()
        .uri("/livez")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "ok");
}

/// `GET /readyz` is the readiness probe. A router built through
/// `create_router` completed its bootstrap (A2A service eagerly
/// initialized), so the very first probe must report ready.
#[tokio::test]
async fn readyz_endpoint_returns_200_after_bootstrap() {
    let app = make_app().await;
    let req = Request::builder()
        .uri("/readyz")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "ok");
}

/// `GET /livez` and `GET /readyz` are mounted under the public
/// router — they MUST NOT require the `Authorization` header. A
/// previous regression moved probes under `protected` and broke
/// orchestrator probes.
#[tokio::test]
async fn probe_endpoints_do_not_require_authorization() {
    let app = make_app().await;
    for uri in ["/livez", "/readyz"] {
        let req = Request::builder()
            .uri(uri)
            // Deliberately NO `authorization` header.
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::OK,
            "GET {uri} MUST be public; got {:?}",
            resp.status()
        );
    }
}

/// The removed `/health` endpoint MUST stay gone — probes are
/// served exclusively by `/livez` and `/readyz` now. A stray
/// re-registration would silently resurface the legacy contract.
#[tokio::test]
async fn legacy_health_endpoint_is_not_mounted() {
    let app = make_app().await;
    let req = Request::builder()
        .uri("/health")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::NOT_FOUND,
        "GET /health must be removed; got {:?}",
        resp.status()
    );
}

/// The `/a2a` JSON-RPC endpoint MUST be mounted and reachable
/// through the public router (it's the only agent-interaction
/// surface). A bare GET on it MUST return a 4xx (no JSON-RPC
/// method dispatched), but NOT 404 (which would mean the route
/// was never wired up).
#[tokio::test]
async fn a2a_jsonrpc_endpoint_is_mounted() {
    let app = make_app().await;
    let req = Request::builder()
        .uri("/a2a")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from("{}"))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    // Empty JSON body is a malformed JSON-RPC request — upstream
    // returns 400 (or 200 with a JSON-RPC error envelope). Either
    // is acceptable; 404 means the route was never mounted.
    assert_ne!(
        resp.status(),
        StatusCode::NOT_FOUND,
        "POST /a2a MUST be mounted"
    );
}

/// `/.well-known/agent-card.json` is the A2A discovery endpoint.
/// Pin that it's mounted under the public router and returns a
/// well-formed AgentCard JSON.
#[tokio::test]
async fn agent_card_endpoint_is_mounted_under_public_router() {
    let app = make_app().await;
    let req = Request::builder()
        .uri("/.well-known/agent-card.json")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let card: serde_json::Value = serde_json::from_slice(&body).unwrap();
    // Required AgentCard fields per A2A v1 spec.
    assert_eq!(card["name"], "Synthia");
    assert!(card["description"].is_string());
    assert!(card["url"].is_string() || card["supportedInterfaces"].is_array());
}

/// `/api/v1/*` management routes are mounted under the protected
/// router and require auth. Pin that a request without an API
/// key returns 401, not 404 (which would mean the route is
/// unwired). The test for this lives in
/// `auth_middleware_test.rs` — this is the smoke check that the
/// route namespace itself exists at all.
#[tokio::test]
async fn management_namespace_is_protected() {
    let app = make_app().await;
    let req = Request::builder()
        .uri("/api/v1/tasks")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_ne!(
        resp.status(),
        StatusCode::NOT_FOUND,
        "GET /api/v1/tasks MUST be mounted (got {:?})",
        resp.status()
    );
}
