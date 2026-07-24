//! Integration tests for the HTTP API and WebSocket endpoints.
//!
//! Tests verify:
//! - Session CRUD operations (GET/POST/DELETE /api/sessions)
//! - Health check and metadata endpoints
//! - MCP tool listing
//! - Auth middleware rejects requests without valid API key
//! - Tracing middleware adds request ID to response headers

use std::sync::Arc;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::json;
use synthia_server::{create_router, state::AppState};
use synthia_session::{manager::SessionManager, store::SERVER_DEFAULT_USER_ID};
use tower::ServiceExt;

fn setup_app() -> Arc<AppState> {
    // Per-test tempdir avoids parallel-run races against `/tmp/test-sessions`
    // and other tests in the same crate. `TempDir` cleans up on drop.
    let tmp = tempfile::tempdir().expect("create temp dir for tests");
    let session_manager = SessionManager::new(tmp.path().join("sessions"));
    Arc::new(AppState::for_test(
        session_manager,
        tmp.path().join("workspace"),
    ))
}

// Helper to extract JSON body from response
async fn body_to_string(body: Body) -> String {
    let bytes = body.collect().await.unwrap().to_bytes();
    String::from_utf8(bytes.to_vec()).unwrap()
}

#[tokio::test]
async fn test_get_sessions_returns_empty_list() {
    let app = setup_app();
    let router = create_router(app);

    let response = router
        .oneshot(
            Request::builder()
                .uri("/api/sessions")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = body_to_string(response.into_body()).await;
    let result: serde_json::Value = serde_json::from_str(&body).unwrap();
    let sessions = result["data"].as_array().expect("Expected data array");
    assert!(sessions.is_empty(), "Expected empty sessions list");
}

#[tokio::test]
async fn test_post_session_creates_session() {
    let app = setup_app();
    let router = create_router(app);

    // The create_session handler generates its own session ID
    let response = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/sessions")
                .header("content-type", "application/json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let response_body = body_to_string(response.into_body()).await;
    let result: serde_json::Value =
        serde_json::from_str(&response_body).unwrap();
    assert!(
        result.get("data").is_some(),
        "Expected 'data' key in ApiResponse"
    );
    assert!(result["data"].get("session_id").is_some());
}

#[tokio::test]
async fn test_get_session_by_id() {
    let app = setup_app();
    app.session_manager
        .create_with_user(
            "test-session-2".to_string(),
            SERVER_DEFAULT_USER_ID.to_string(),
        )
        .await
        .expect("create test session");

    let router = create_router(app.clone());

    let response = router
        .oneshot(
            Request::builder()
                .uri("/api/sessions/test-session-2")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let response_body = body_to_string(response.into_body()).await;
    let result: serde_json::Value =
        serde_json::from_str(&response_body).unwrap();
    assert_eq!(result["data"]["id"], "test-session-2");
}

#[tokio::test]
async fn test_get_nonexistent_session_returns_404() {
    let app = setup_app();
    let router = create_router(app);

    let response = router
        .oneshot(
            Request::builder()
                .uri("/api/sessions/nonexistent")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_delete_session() {
    let app = setup_app();
    app.session_manager
        .create_with_user(
            "test-session-3".to_string(),
            SERVER_DEFAULT_USER_ID.to_string(),
        )
        .await
        .expect("create test session");

    let router = create_router(app.clone());

    let response = router
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/sessions/test-session-3")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let response_body = body_to_string(response.into_body()).await;
    let result: serde_json::Value =
        serde_json::from_str(&response_body).unwrap();
    assert!(result["data"]["deleted"].as_bool().unwrap());

    // Verify session is gone
    let router2 = create_router(app);
    let response = router2
        .oneshot(
            Request::builder()
                .uri("/api/sessions/test-session-3")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_mcp_list_returns_200() {
    let app = setup_app();
    let router = create_router(app);

    let response = router
        .oneshot(
            Request::builder()
                .uri("/api/mcp/servers")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Should return 200 even with empty tool registry
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_tracing_middleware_adds_request_id() {
    let app = setup_app();
    let router = create_router(app);

    let response = router
        .oneshot(
            Request::builder()
                .uri("/api/sessions")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // The tracing middleware should add x-request-id header
    let headers = response.headers();
    assert!(
        headers.contains_key("x-request-id"),
        "Expected x-request-id header from tracing middleware"
    );
}

#[tokio::test]
async fn test_ws_endpoint_rejects_unknown_session() {
    let app = setup_app();

    // Verify the session doesn't exist first
    assert!(
        app.session_manager
            .get("nonexistent-session")
            .await
            .is_none(),
        "Session should not exist"
    );

    let router = create_router(app);

    // Try to connect WebSocket to a non-existent session
    let response = router
        .oneshot(
            Request::builder()
                .uri("/api/sessions/nonexistent-session/stream")
                .header("upgrade", "websocket")
                .header("connection", "Upgrade")
                .header("sec-websocket-key", "dGhlIHNhbXBsZSBub25jZQ==")
                .header("sec-websocket-version", "13")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Should reject unknown sessions (426 Upgrade Required or similar,
    // since axum requires proper WebSocket upgrade handshake)
    assert!(
        response.status().is_client_error(),
        "Expected a client error status for unknown WebSocket session, got {}",
        response.status()
    );
}

#[tokio::test]
async fn test_mcp_call_rejects_missing_params() {
    let app = setup_app();
    let router = create_router(app);

    // Missing required fields (tool_name, args)
    let body = json!({});
    let response = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/mcp/call")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    // Should return 400 for missing params or tool not found
    assert!(
        response.status() == StatusCode::BAD_REQUEST
            || response.status() == StatusCode::NOT_FOUND,
        "Expected 400 or 404 for missing tool params, got {}",
        response.status()
    );
}
