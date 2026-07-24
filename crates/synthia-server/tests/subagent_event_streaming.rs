//! Integration tests for subagent event streaming through the V2 API.
//!
//! These tests verify that events emitted by a child (subagent) session
//! appear in the parent session's event stream over HTTP SSE, are
//! replayed on reconnect, are observed by multiple clients, and that the
//! child stream itself contains raw events rather than `SubagentEvent`
//! wrappers.

use std::{sync::Arc, time::Duration};

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::json;
use synthia_agent::types::AgentEvent;
use synthia_server::{
    create_router,
    session::controller::SessionController,
    state::AppState,
};
use synthia_session::manager::SessionManager;
use tower::ServiceExt;

const TEST_USER: &str = synthia_session::store::SERVER_DEFAULT_USER_ID;

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

/// Create a parent session and a child session wired to forward events
/// back to the parent controller.
async fn create_parent_child_pair(
    app: &AppState,
    parent_id: &str,
    child_id: &str,
) -> (Arc<SessionController>, Arc<SessionController>) {
    app.session_manager
        .create_with_user(parent_id.to_string(), TEST_USER.to_string())
        .await
        .unwrap();

    let parent = app
        .get_or_create_session_controller(TEST_USER, parent_id)
        .await
        .unwrap();

    app.session_manager
        .create_child(
            TEST_USER.to_string(),
            parent_id.to_string(),
            Some(child_id.to_string()),
        )
        .await
        .unwrap();

    let child = app
        .get_or_create_session_controller_with_parent(
            TEST_USER,
            child_id,
            Some(parent.event_sender()),
            None,
        )
        .await
        .unwrap();

    (parent, child)
}

/// Read the SSE response body until `timeout` elapses or `predicate`
/// returns true for the accumulated text.
async fn read_sse_until<F>(
    body: Body,
    timeout: Duration,
    mut predicate: F,
) -> String
where
    F: FnMut(&str) -> bool,
{
    let mut body = body;
    let mut text = String::new();
    let deadline = tokio::time::Instant::now() + timeout;

    while tokio::time::Instant::now() < deadline {
        match tokio::time::timeout_at(deadline, body.frame()).await {
            Ok(Some(Ok(frame))) => {
                if let Some(data) = frame.data_ref() {
                    text.push_str(&String::from_utf8_lossy(data));
                    if predicate(&text) {
                        return text;
                    }
                }
            }
            _ => break,
        }
    }
    text
}

#[tokio::test]
async fn test_parent_events_receive_subagent_events() {
    let app = setup_app();
    let router = create_router(app.clone());

    let (_parent, child) =
        create_parent_child_pair(&app, "parent-events", "child-events").await;

    let response = router
        .oneshot(
            Request::builder()
                .uri("/api/v2/sessions/parent-events/events?last_seq=0")
                .header("accept", "text/event-stream")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body();

    // Emit a raw event from the child. The parent stream should receive
    // it wrapped as a SubagentEvent.
    let raw_event = AgentEvent::ToolCallStarted {
        tool_name: "read_file".to_string(),
        input: json!({"path": "/tmp/test"}),
    };
    child.event_sender().send(raw_event).await.unwrap();

    let text = read_sse_until(body, Duration::from_millis(500), |t| {
        t.contains("event: subagent_event")
    })
    .await;

    assert!(
        text.contains("event: subagent_event"),
        "parent SSE should contain subagent_event, got: {text}"
    );
    assert!(
        text.contains("child-events"),
        "parent SSE should include child_session_id, got: {text}"
    );
    assert!(
        text.contains("ToolCallStarted"),
        "parent SSE should include wrapped raw event type, got: {text}"
    );
}

#[tokio::test]
async fn test_child_events_are_raw() {
    let app = setup_app();
    let router = create_router(app.clone());

    let (_parent, child) =
        create_parent_child_pair(&app, "parent-raw", "child-raw").await;

    let response = router
        .oneshot(
            Request::builder()
                .uri("/api/v2/sessions/child-raw/events?last_seq=0")
                .header("accept", "text/event-stream")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body();

    let raw_event = AgentEvent::ToolCallStarted {
        tool_name: "read_file".to_string(),
        input: json!({"path": "/tmp/test"}),
    };
    child.event_sender().send(raw_event).await.unwrap();

    let text = read_sse_until(body, Duration::from_millis(500), |t| {
        t.contains("event: ToolCallStarted")
    })
    .await;

    assert!(
        text.contains("event: ToolCallStarted"),
        "child SSE should contain raw ToolCallStarted, got: {text}"
    );
    assert!(
        !text.contains("event: subagent_event"),
        "child SSE should NOT contain subagent_event wrapper, got: {text}"
    );
}

#[tokio::test]
async fn test_parent_replay_includes_subagent_events() {
    let app = setup_app();

    let (_parent, child) =
        create_parent_child_pair(&app, "parent-replay", "child-replay").await;

    // Emit the child event before opening the stream so it must come
    // from replay rather than the live broadcast.
    let raw_event = AgentEvent::ToolCallStarted {
        tool_name: "read_file".to_string(),
        input: json!({"path": "/tmp/test"}),
    };
    child.event_sender().send(raw_event).await.unwrap();

    // Give the controller a moment to persist the forwarded event.
    tokio::time::sleep(Duration::from_millis(50)).await;

    let router = create_router(app.clone());
    let response = router
        .oneshot(
            Request::builder()
                .uri("/api/v2/sessions/parent-replay/events?last_seq=0")
                .header("accept", "text/event-stream")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let text =
        read_sse_until(response.into_body(), Duration::from_millis(500), |t| {
            t.contains("event: SyncCaughtUp")
        })
        .await;

    assert!(
        text.contains("event: subagent_event"),
        "replayed parent events should include subagent_event, got: {text}"
    );
    assert!(
        text.contains("child-replay"),
        "replayed subagent_event should include child_session_id, got: {text}"
    );
}

#[tokio::test]
async fn test_multi_client_subagent_observation() {
    let app = setup_app();
    let router = create_router(app.clone());

    let (_parent, child) =
        create_parent_child_pair(&app, "parent-multi", "child-multi").await;

    let response1 = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v2/sessions/parent-multi/events?last_seq=0")
                .header("accept", "text/event-stream")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response1.status(), StatusCode::OK);
    let body1 = response1.into_body();

    let response2 = router
        .oneshot(
            Request::builder()
                .uri("/api/v2/sessions/parent-multi/events?last_seq=0")
                .header("accept", "text/event-stream")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response2.status(), StatusCode::OK);
    let body2 = response2.into_body();

    let raw_event = AgentEvent::ToolCallStarted {
        tool_name: "read_file".to_string(),
        input: json!({"path": "/tmp/test"}),
    };
    child.event_sender().send(raw_event).await.unwrap();

    let (text1, text2) = tokio::join!(
        read_sse_until(body1, Duration::from_millis(500), |t| {
            t.contains("event: subagent_event")
        }),
        read_sse_until(body2, Duration::from_millis(500), |t| {
            t.contains("event: subagent_event")
        }),
    );

    assert!(
        text1.contains("event: subagent_event"),
        "client 1 should observe subagent_event, got: {text1}"
    );
    assert!(
        text2.contains("event: subagent_event"),
        "client 2 should observe subagent_event, got: {text2}"
    );
    assert!(
        text1.contains("child-multi"),
        "client 1 should include child_session_id, got: {text1}"
    );
    assert!(
        text2.contains("child-multi"),
        "client 2 should include child_session_id, got: {text2}"
    );
}

#[tokio::test]
async fn test_subagents_endpoint_lists_children() {
    let app = setup_app();
    let router = create_router(app.clone());

    app.session_manager
        .create_with_user("parent-list".to_string(), TEST_USER.to_string())
        .await
        .unwrap();

    let child = app
        .session_manager
        .create_child(
            TEST_USER.to_string(),
            "parent-list".to_string(),
            Some("child-list".to_string()),
        )
        .await
        .unwrap();

    let response = router
        .oneshot(
            Request::builder()
                .uri("/api/v2/sessions/parent-list/subagents")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = body_to_json(response.into_body()).await;

    let data = body["data"].as_array().expect("data should be an array");
    assert_eq!(data.len(), 1, "expected one child session");
    assert_eq!(data[0]["id"], child.id);
    assert_eq!(data[0]["parent_id"], "parent-list");
}
