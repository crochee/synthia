//! Integration tests for the V2 session controller REST API.
//!
//! Covers session lifecycle, prompts, steering, cancel, messages
//! pagination, and the events SSE endpoint.

use std::{sync::Arc, time::Duration};

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::json;
use synthia_server::{create_router, state::AppState};
use synthia_session::manager::SessionManager;
use tower::ServiceExt;

fn setup_app() -> Arc<AppState> {
    let tmp = tempfile::tempdir().expect("create temp dir for tests");
    let session_manager = SessionManager::new(tmp.path().join("sessions"));
    Arc::new(AppState::for_test(
        session_manager,
        tmp.path().join("workspace"),
    ))
}

async fn body_to_json(body: Body) -> serde_json::Value {
    let bytes = body.collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn test_create_session_v2_returns_201_with_location() {
    let app = setup_app();
    let router = create_router(app);

    let response = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v2/sessions")
                .header("content-type", "application/json")
                .body(Body::from(json!({"title": "test"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
    let location = response
        .headers()
        .get("location")
        .expect("Location header should be present")
        .to_str()
        .unwrap();
    assert!(location.starts_with("/api/v2/sessions/"));

    let body = body_to_json(response.into_body()).await;
    assert!(body["data"]["id"].as_str().is_some());
    assert_eq!(body["data"]["title"], "test");
}

#[tokio::test]
async fn test_list_sessions_v2_pagination() {
    let app = setup_app();
    app.session_manager
        .create_with_user("s1".to_string(), "_legacy_".to_string())
        .await
        .unwrap();
    app.session_manager
        .create_with_user("s2".to_string(), "_legacy_".to_string())
        .await
        .unwrap();

    let router = create_router(app);
    let response = router
        .oneshot(
            Request::builder()
                .uri("/api/v2/sessions?limit=1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = body_to_json(response.into_body()).await;
    assert_eq!(body["data"].as_array().unwrap().len(), 1);
    assert!(body["meta"]["has_next"].as_bool().unwrap());
    assert!(body["links"]["next"].as_str().is_some());
}

#[tokio::test]
async fn test_get_session_v2_detail() {
    let app = setup_app();
    app.session_manager
        .create_with_user("s1".to_string(), "_legacy_".to_string())
        .await
        .unwrap();

    let router = create_router(app);
    let response = router
        .oneshot(
            Request::builder()
                .uri("/api/v2/sessions/s1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = body_to_json(response.into_body()).await;
    assert_eq!(body["data"]["id"], "s1");
}

#[tokio::test]
async fn test_delete_session_v2_returns_204() {
    let app = setup_app();
    app.session_manager
        .create_with_user("s1".to_string(), "_legacy_".to_string())
        .await
        .unwrap();

    let router = create_router(app);
    let response = router
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/v2/sessions/s1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn test_create_prompt_returns_202() {
    let app = setup_app();
    app.session_manager
        .create_with_user("s1".to_string(), "_legacy_".to_string())
        .await
        .unwrap();

    let router = create_router(app);
    let response = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v2/sessions/s1/prompts")
                .header("content-type", "application/json")
                .body(Body::from(json!({"content": "hello"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let body = body_to_json(response.into_body()).await;
    assert!(body["admitted"].as_bool().unwrap());
}

#[tokio::test]
async fn test_create_steering_returns_202() {
    let app = setup_app();
    app.session_manager
        .create_with_user("s1".to_string(), "_legacy_".to_string())
        .await
        .unwrap();

    let router = create_router(app);
    let response = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v2/sessions/s1/steering")
                .header("content-type", "application/json")
                .body(Body::from(json!({"content": "turn left"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let body = body_to_json(response.into_body()).await;
    assert!(body["admitted"].as_bool().unwrap());
}

#[tokio::test]
async fn test_cancel_session_returns_200() {
    let app = setup_app();
    app.session_manager
        .create_with_user("s1".to_string(), "_legacy_".to_string())
        .await
        .unwrap();

    let router = create_router(app);
    let response = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v2/sessions/s1/cancel")
                .header("content-type", "application/json")
                .body(Body::from(json!({"reason": "test"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = body_to_json(response.into_body()).await;
    assert!(body["cancelled"].as_bool().unwrap());
}

#[tokio::test]
async fn test_list_messages_v2_pagination() {
    let app = setup_app();
    app.session_manager
        .create_with_user("s1".to_string(), "_legacy_".to_string())
        .await
        .unwrap();

    let message = synthia_provider::Message::user("hello");
    app.session_manager
        .store()
        .append_message("_legacy_", "s1", &message)
        .unwrap();

    let router = create_router(app);
    let response = router
        .oneshot(
            Request::builder()
                .uri("/api/v2/sessions/s1/messages?limit=10")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = body_to_json(response.into_body()).await;
    let messages = body["data"].as_array().unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0]["seq"], 1);
    assert_eq!(messages[0]["content"], "hello");
}

#[tokio::test]
async fn test_list_subagents_returns_children() {
    let app = setup_app();
    let parent = app
        .session_manager
        .create_with_user("parent".to_string(), "_legacy_".to_string())
        .await
        .unwrap();
    app.session_manager.save_metadata(&parent).unwrap();

    app.session_manager
        .create_child(
            "_legacy_".to_string(),
            "parent".to_string(),
            Some("child-a".to_string()),
        )
        .await
        .unwrap();
    app.session_manager
        .create_child(
            "_legacy_".to_string(),
            "parent".to_string(),
            Some("child-b".to_string()),
        )
        .await
        .unwrap();

    let router = create_router(app);
    let response = router
        .oneshot(
            Request::builder()
                .uri("/api/v2/sessions/parent/subagents")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = body_to_json(response.into_body()).await;
    let children = body["data"].as_array().unwrap();
    assert_eq!(children.len(), 2);
    assert!(children.iter().any(|c| c["id"] == "child-a"));
    assert!(children.iter().any(|c| c["id"] == "child-b"));
    assert!(children.iter().all(|c| c["parent_id"] == "parent"));
}

#[tokio::test]
async fn test_list_subagents_pagination() {
    let app = setup_app();
    let parent = app
        .session_manager
        .create_with_user("parent".to_string(), "_legacy_".to_string())
        .await
        .unwrap();
    app.session_manager.save_metadata(&parent).unwrap();

    for i in 0..3 {
        app.session_manager
            .create_child(
                "_legacy_".to_string(),
                "parent".to_string(),
                Some(format!("child-{}", i)),
            )
            .await
            .unwrap();
    }

    let router = create_router(app);
    let response = router
        .oneshot(
            Request::builder()
                .uri("/api/v2/sessions/parent/subagents?limit=2")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = body_to_json(response.into_body()).await;
    assert_eq!(body["data"].as_array().unwrap().len(), 2);
    assert!(body["meta"]["has_next"].as_bool().unwrap());
    assert!(body["links"]["next"].as_str().is_some());
}

#[tokio::test]
async fn test_list_subagents_isolation_returns_404_for_non_owner() {
    let app = setup_app();
    let parent = app
        .session_manager
        .create_with_user("parent".to_string(), "bob".to_string())
        .await
        .unwrap();
    app.session_manager.save_metadata(&parent).unwrap();

    let router = create_router(app);
    let response = router
        .oneshot(
            Request::builder()
                .uri("/api/v2/sessions/parent/subagents")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_session_summary_includes_parent_id() {
    let app = setup_app();
    let parent = app
        .session_manager
        .create_with_user("parent".to_string(), "_legacy_".to_string())
        .await
        .unwrap();
    app.session_manager.save_metadata(&parent).unwrap();

    app.session_manager
        .create_child(
            "_legacy_".to_string(),
            "parent".to_string(),
            Some("child-a".to_string()),
        )
        .await
        .unwrap();

    let router = create_router(app);
    let response = router
        .oneshot(
            Request::builder()
                .uri("/api/v2/sessions")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = body_to_json(response.into_body()).await;
    let sessions = body["data"].as_array().unwrap();
    let child = sessions.iter().find(|s| s["id"] == "child-a").unwrap();
    assert_eq!(child["parent_id"], "parent");
}

#[tokio::test]
async fn test_events_sse_stream_emits_sync_caught_up() {
    let app = setup_app();
    app.session_manager
        .create_with_user("s1".to_string(), "_legacy_".to_string())
        .await
        .unwrap();

    let router = create_router(app);
    let response = router
        .oneshot(
            Request::builder()
                .uri("/api/v2/sessions/s1/events?last_seq=0")
                .header("accept", "text/event-stream")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let content_type = response
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(content_type.contains("text/event-stream"));

    // SSE stream stays open for live events; read frames until the
    // SyncCaughtUp event arrives or a short timeout passes.
    let mut body = response.into_body();
    let mut bytes = Vec::new();
    while let Ok(Some(Ok(frame))) =
        tokio::time::timeout(Duration::from_millis(200), body.frame()).await
    {
        if let Some(data) = frame.data_ref() {
            bytes.extend_from_slice(data);
            if String::from_utf8_lossy(&bytes).contains("event: SyncCaughtUp") {
                break;
            }
        }
    }
    let text = String::from_utf8_lossy(&bytes);
    assert!(text.contains("event: SyncCaughtUp"));
}
