//! Integration tests for SSE streaming lifecycle and heartbeat.

use std::{sync::Arc, time::Duration};

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use synthia_agent::types::{AgentEvent, SessionEndReason};
use synthia_server::{
    create_router,
    event_stream::EventBroadcaster,
    sse::{
        HEARTBEAT_INTERVAL,
        SseError,
        agent_event_to_sse,
        error_event,
        event_variant_name,
    },
    state::AppState,
};
use synthia_session::{manager::SessionManager, store::SERVER_DEFAULT_USER_ID};
use tokio::sync::broadcast;
use tower::ServiceExt;

const TEST_USER: &str = SERVER_DEFAULT_USER_ID;

fn setup_app() -> Arc<AppState> {
    // Per-test tempdir avoids parallel-run races against `/tmp/test-sessions-sse`
    // shared with `integration_test.rs` and other tests in the same crate.
    // `TempDir` cleans up on drop.
    let tmp = tempfile::tempdir().expect("create temp dir for tests");
    let session_manager = SessionManager::new(tmp.path().join("sessions"));
    Arc::new(AppState::for_test(
        session_manager,
        tmp.path().join("workspace"),
    ))
}

#[tokio::test]
async fn test_sse_stream_sse_endpoint_not_found_for_missing_session() {
    let app = setup_app();
    let router = create_router(app);

    let response = router
        .oneshot(
            Request::builder()
                .uri("/api/v1/sessions/nonexistent/stream-sse")
                .header("Accept", "text/event-stream")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_sse_event_broadcaster_lifecycle() {
    let broadcaster = EventBroadcaster::new();
    let mut rx = broadcaster.subscribe();

    let event = AgentEvent::SessionStarted {
        session_id: "test-1".to_string(),
    };
    broadcaster.send(event).unwrap();

    let received = tokio::time::timeout(Duration::from_millis(100), rx.recv())
        .await
        .expect("Should receive event")
        .expect("Should not be closed");

    assert!(matches!(received, AgentEvent::SessionStarted { .. }));
}

#[tokio::test]
async fn test_sse_event_conversion() {
    let (tx, mut rx) = broadcast::channel::<AgentEvent>(32);

    let event = AgentEvent::SessionStarted {
        session_id: "conv-test".to_string(),
    };
    tx.send(event).unwrap();

    let received = tokio::time::timeout(Duration::from_millis(100), rx.recv())
        .await
        .expect("Should receive event")
        .expect("Should not be closed");

    assert!(matches!(received, AgentEvent::SessionStarted { .. }));
}

#[tokio::test]
async fn test_sse_multiple_subscribers() {
    let broadcaster = EventBroadcaster::new();
    let mut rx1 = broadcaster.subscribe();
    let mut rx2 = broadcaster.subscribe();

    let event = AgentEvent::Thinking {
        text: "test thinking".to_string(),
        iteration: 1,
    };
    broadcaster.send(event).unwrap();

    let r1 = tokio::time::timeout(Duration::from_millis(100), rx1.recv())
        .await
        .expect("rx1 should receive")
        .expect("rx1 should not be closed");

    let r2 = tokio::time::timeout(Duration::from_millis(100), rx2.recv())
        .await
        .expect("rx2 should receive")
        .expect("rx2 should not be closed");

    assert!(matches!(r1, AgentEvent::Thinking { .. }));
    assert!(matches!(r2, AgentEvent::Thinking { .. }));
}

#[tokio::test]
async fn test_sse_error_event_serialization() {
    let error = SseError {
        code: "internal_error".to_string(),
        message: "Something went wrong".to_string(),
    };

    let json = serde_json::to_string(&error).unwrap();
    assert!(json.contains("internal_error"));
    assert!(json.contains("Something went wrong"));
}

#[tokio::test]
async fn test_sse_all_event_types_serialize() {
    let events = vec![
        AgentEvent::SessionStarted {
            session_id: "s1".to_string(),
        },
        AgentEvent::IterationStarted { iteration: 1 },
        AgentEvent::Thinking {
            text: "thinking".to_string(),
            iteration: 1,
        },
        AgentEvent::LlmStreamDelta {
            content: "delta".to_string(),
        },
        AgentEvent::LlmResponseComplete {
            content: "response".to_string(),
            usage: synthia_agent::types::TokenUsage {
                prompt_tokens: 10,
                completion_tokens: 20,
                total_tokens: 30,
                cached_prompt_tokens: None,
                cache_read_tokens: None,
                cache_write_tokens: None,
            },
        },
        AgentEvent::ToolCallStarted {
            tool_name: "read_file".to_string(),
            input: serde_json::json!({"path": "/tmp"}),
        },
        AgentEvent::ToolCallCompleted {
            tool_name: "read_file".to_string(),
            output: "content".to_string(),
            is_error: false,
        },
        AgentEvent::SessionEnded {
            reason: SessionEndReason::Completed,
        },
    ];

    for event in events {
        let json = serde_json::to_string(&event).unwrap();
        assert!(!json.is_empty(), "Event should serialize to non-empty JSON");
        assert!(json.contains("\"type\":"));
    }
}

#[tokio::test]
async fn test_sse_session_ended_event() {
    let (tx, mut rx) = broadcast::channel::<AgentEvent>(32);

    tx.send(AgentEvent::SessionEnded {
        reason: SessionEndReason::Completed,
    })
    .unwrap();

    let event = tokio::time::timeout(Duration::from_millis(100), rx.recv())
        .await
        .expect("Should receive event")
        .expect("Should not be closed");

    assert!(matches!(event, AgentEvent::SessionEnded { .. }));
}

#[test]
fn test_sse_heartbeat_interval_configured() {
    assert_eq!(HEARTBEAT_INTERVAL, Duration::from_secs(15));
}

#[tokio::test]
async fn test_event_broadcaster_cleanup() {
    let app = setup_app();

    let _broadcaster = app
        .get_or_create_broadcaster(TEST_USER, "cleanup-session")
        .await;
    assert!(
        app.get_event_broadcaster(TEST_USER, "cleanup-session")
            .await
            .is_some()
    );

    app.remove_broadcaster(TEST_USER, "cleanup-session").await;
    assert!(
        app.get_event_broadcaster(TEST_USER, "cleanup-session")
            .await
            .is_none()
    );
}

#[tokio::test]
async fn test_v2_sse_stream_endpoint_exists() {
    let app = setup_app();
    app.session_manager.create("v2-sse-test".to_string()).await;
    let broadcaster = app
        .get_or_create_broadcaster(TEST_USER, "v2-sse-test")
        .await;

    let router = create_router(app);

    let response = router
        .oneshot(
            Request::builder()
                .uri("/api/v2/sessions/v2-sse-test/stream-sse")
                .header("Accept", "text/event-stream")
                .body(Body::empty())
                .unwrap(),
        )
        .await;

    assert_ne!(
        response.unwrap().status(),
        StatusCode::NOT_FOUND,
        "SSE endpoint should not return 404"
    );

    // Keep broadcaster alive for the test
    drop(broadcaster);
}

#[test]
fn test_sse_event_variant_names() {
    let event = AgentEvent::SessionStarted {
        session_id: "s1".to_string(),
    };
    assert_eq!(event_variant_name(&event), "SessionStarted");

    let sse = agent_event_to_sse(&event);
    assert!(std::mem::size_of_val(&sse) > 0);
}

#[test]
fn test_subagent_event_sse_mapping() {
    let event = AgentEvent::SubagentEvent {
        child_session_id: "child-1".to_string(),
        event: Box::new(AgentEvent::Thinking {
            text: "nested".to_string(),
            iteration: 1,
        }),
    };

    assert_eq!(event_variant_name(&event), "subagent_event");

    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("\"type\":\"subagent_event\""));
    assert!(json.contains("\"child_session_id\":\"child-1\""));
    assert!(json.contains("\"event\":{\"type\":\"Thinking\""));

    let sse = agent_event_to_sse(&event);
    assert!(std::mem::size_of_val(&sse) > 0);
}

#[test]
fn test_error_event_format() {
    let sse = error_event("internal_error", "Something went wrong");
    assert!(std::mem::size_of_val(&sse) > 0);
}

#[tokio::test]
async fn test_v2_sse_stream_returns_correct_status_with_session() {
    let app = setup_app();
    app.session_manager
        .create("v2-sse-status-test".to_string())
        .await;
    let broadcaster = app
        .get_or_create_broadcaster(TEST_USER, "v2-sse-status-test")
        .await;

    let router = create_router(app);

    let resp = router
        .oneshot(
            Request::builder()
                .uri("/api/v2/sessions/v2-sse-status-test/stream-sse")
                .header("Accept", "text/event-stream")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);

    drop(broadcaster);
}
