//! Integration tests for the server-side HTTP approval API.

use std::{sync::Arc, time::Duration};

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::json;
use synthia_permission::ApprovalOutcome;
use synthia_server::{create_router, state::AppState};
use synthia_session::manager::SessionManager;
use tower::ServiceExt;

async fn setup_app() -> Arc<AppState> {
    let tmp = tempfile::tempdir().expect("create temp dir for tests");
    let session_manager = SessionManager::new(tmp.path().join("sessions"));
    Arc::new(
        AppState::for_test(session_manager, tmp.path().join("workspace")).await,
    )
}

async fn body_to_json(body: Body) -> serde_json::Value {
    let bytes = body.collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn test_resolve_approval_via_http() {
    let app = setup_app().await;
    let router = create_router(app.clone()).await;

    // Submit an approval request directly through the shared state.
    let (request_id, outcome_rx) = app.approval_state.submit(
        "write_file",
        json!({ "path": "foo.txt", "content": "hello" }),
    );

    // List pending approvals.
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/approvals")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = body_to_json(response.into_body()).await;
    let approvals = body["data"].as_array().unwrap();
    assert_eq!(approvals.len(), 1);
    assert_eq!(approvals[0]["request_id"], request_id);
    assert_eq!(approvals[0]["tool_name"], "write_file");

    // Resolve the approval via HTTP.
    let response = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/approvals/{request_id}/resolve"))
                .header("content-type", "application/json")
                .body(Body::from(json!({ "outcome": "approve" }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = body_to_json(response.into_body()).await;
    assert!(body["data"]["resolved"].as_bool().unwrap());

    // The oneshot receiver should receive the approved outcome.
    let outcome = tokio::time::timeout(Duration::from_secs(1), outcome_rx)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(outcome, ApprovalOutcome::Approve);
}

#[tokio::test]
async fn test_resolve_unknown_approval_returns_404() {
    let app = setup_app().await;
    let router = create_router(app).await;

    let response = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/approvals/nonexistent/resolve")
                .header("content-type", "application/json")
                .body(Body::from(json!({ "outcome": "approve" }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_resolve_with_invalid_outcome_returns_400() {
    let app = setup_app().await;
    let router = create_router(app.clone()).await;

    let (request_id, _rx) = app
        .approval_state
        .submit("read_file", json!({ "path": "bar.txt" }));

    let response = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/approvals/{request_id}/resolve"))
                .header("content-type", "application/json")
                .body(Body::from(json!({ "outcome": "maybe" }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
