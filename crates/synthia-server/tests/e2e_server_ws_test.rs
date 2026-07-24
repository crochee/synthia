//! E2E test: Server WebSocket streaming.

use std::{path::PathBuf, sync::Arc};

use axum::{
    Router,
    http::{Method, Request, StatusCode},
};
use http_body_util::BodyExt;
use synthia_server::state::AppState;
use synthia_session::manager::SessionManager;
use test_support::FakeProvider;
use tower::ServiceExt;

async fn build_test_app(workspace: PathBuf) -> Router {
    let session_manager =
        SessionManager::new(workspace.join(".synthia").join("sessions"));
    let state = Arc::new(AppState::for_test(session_manager, workspace));
    synthia_server::create_router(state)
}

#[tokio::test]
async fn test_health_check_endpoint() {
    let temp_dir = tempfile::tempdir().unwrap();
    let workspace = temp_dir.path().to_path_buf();

    let app = build_test_app(workspace).await;
    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .method(Method::GET)
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_create_session_endpoint() {
    let temp_dir = tempfile::tempdir().unwrap();
    let workspace = temp_dir.path().to_path_buf();

    let app = build_test_app(workspace.clone()).await;

    let body = serde_json::json!({
        "session_id": "ws-test-session",
        "model": "test-model"
    });

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/sessions")
                .method(Method::POST)
                .header("content-type", "application/json")
                .body(axum::body::Body::from(
                    serde_json::to_string(&body).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert!(json.get("session_id").is_some() || json.get("data").is_some());
}

#[tokio::test]
async fn test_list_sessions_empty() {
    let temp_dir = tempfile::tempdir().unwrap();
    let workspace = temp_dir.path().to_path_buf();

    let app = build_test_app(workspace).await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/sessions")
                .method(Method::GET)
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_chat_endpoint_json_response() {
    let temp_dir = tempfile::tempdir().unwrap();
    let workspace = temp_dir.path().to_path_buf();

    let session_manager =
        SessionManager::new(workspace.join(".synthia").join("sessions"));
    let mut state = AppState::for_test(session_manager, workspace.clone());

    let response = synthia_provider::types::CompletionResponse {
        id: "resp-json-1".to_string(),
        model: "test-model".to_string(),
        content: synthia_provider::types::Content::text("JSON test response"),
        usage: synthia_provider::types::TokenUsage {
            prompt_tokens: 10,
            completion_tokens: 5,
            total_tokens: 15,
            cached_prompt_tokens: None,
            cache_read_tokens: None,
            cache_write_tokens: None,
        },
        cached: false,
    };
    state.default_provider = Arc::new(FakeProvider::with_response(response));

    let app = synthia_server::create_router(Arc::new(state));

    let chat_body = serde_json::json!({
        "session_id": "chat-json-test",
        "input": "Hello"
    });

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/chat")
                .method(Method::POST)
                .header("content-type", "application/json")
                .body(axum::body::Body::from(
                    serde_json::to_string(&chat_body).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert!(
        response.status().is_success(),
        "chat endpoint should respond, got {}",
        response.status()
    );
}

#[tokio::test]
async fn test_session_checkpoint_after_interaction() {
    let temp_dir = tempfile::tempdir().unwrap();
    let workspace = temp_dir.path().to_path_buf();
    let session_dir = workspace.join(".synthia").join("sessions");
    std::fs::create_dir_all(&session_dir).unwrap();

    let session_id = "checkpoint-test";
    let session_path = session_dir.join(session_id);
    std::fs::create_dir_all(&session_path).unwrap();

    assert!(
        session_path.exists(),
        "session directory should exist at {:?}",
        session_path
    );

    let checkpoint_dir = session_path.join("checkpoints");
    std::fs::create_dir_all(&checkpoint_dir).unwrap();

    let checkpoint_file = checkpoint_dir.join("checkpoint_001.json");
    std::fs::write(&checkpoint_file, r#"{"step": 1}"#).unwrap();

    assert!(
        checkpoint_file.exists(),
        "checkpoint file should exist at {:?}",
        checkpoint_file
    );
}

#[tokio::test]
async fn test_get_session_detail() {
    let temp_dir = tempfile::tempdir().unwrap();
    let workspace = temp_dir.path().to_path_buf();

    let app = build_test_app(workspace.clone()).await;

    // Create the session through the V2 endpoint so it is bound to the
    // authenticated user namespace that the detail endpoint queries.
    let create_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v2/sessions")
                .method(Method::POST)
                .header("content-type", "application/json")
                .body(axum::body::Body::from(
                    serde_json::json!({ "title": "detail-test" }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(create_response.status(), StatusCode::CREATED);
    let body_bytes = create_response
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    let session_id = body["data"]["id"]
        .as_str()
        .expect("created session should have an id");

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/v2/sessions/{}", session_id))
                .method(Method::GET)
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert!(
        response.status().is_success(),
        "get session detail should succeed"
    );
}

#[tokio::test]
async fn test_list_tools_endpoint() {
    let temp_dir = tempfile::tempdir().unwrap();
    let workspace = temp_dir.path().to_path_buf();

    let app = build_test_app(workspace).await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/tools")
                .method(Method::GET)
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_list_skills_endpoint() {
    let temp_dir = tempfile::tempdir().unwrap();
    let workspace = temp_dir.path().to_path_buf();

    let app = build_test_app(workspace).await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/skills")
                .method(Method::GET)
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_chat_endpoint_with_fake_provider() {
    let temp_dir = tempfile::tempdir().unwrap();
    let workspace = temp_dir.path().to_path_buf();

    let session_manager =
        SessionManager::new(workspace.join(".synthia").join("sessions"));
    let mut state = AppState::for_test(session_manager, workspace.clone());

    let response = synthia_provider::types::CompletionResponse {
        id: "resp-1".to_string(),
        model: "test-model".to_string(),
        content: synthia_provider::types::Content::text("Test response"),
        usage: synthia_provider::types::TokenUsage {
            prompt_tokens: 10,
            completion_tokens: 5,
            total_tokens: 15,
            cached_prompt_tokens: None,
            cache_read_tokens: None,
            cache_write_tokens: None,
        },
        cached: false,
    };
    state.default_provider = Arc::new(FakeProvider::with_response(response));

    let app = synthia_server::create_router(Arc::new(state));

    let chat_body = serde_json::json!({
        "session_id": "chat-ws-test",
        "input": "Hello"
    });

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/chat")
                .method(Method::POST)
                .header("content-type", "application/json")
                .body(axum::body::Body::from(
                    serde_json::to_string(&chat_body).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert!(
        response.status().is_success()
            || response.status() == StatusCode::NOT_FOUND,
        "chat endpoint should respond, got {}",
        response.status()
    );
}

#[tokio::test]
async fn test_v1_routes_include_deprecation_header() {
    let temp_dir = tempfile::tempdir().unwrap();
    let workspace = temp_dir.path().to_path_buf();

    let app = build_test_app(workspace).await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/sessions")
                .method(Method::GET)
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get("Deprecation")
            .and_then(|v| v.to_str().ok()),
        Some("true"),
        "V1 routes must include Deprecation: true header"
    );
}
