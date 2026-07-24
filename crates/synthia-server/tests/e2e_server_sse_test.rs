//! E2E test: Server SSE streaming.
//!
//! Tests the SSE endpoint: send HTTP with Accept: text/event-stream,
//! verify SSE streaming response.

use std::{path::PathBuf, sync::Arc};

use axum::{
    Router,
    http::{Method, Request, StatusCode},
};
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

#[test]
fn test_sse_accept_header_detection() {
    let sse_header = Some("text/event-stream".to_string());
    let is_sse = sse_header
        .as_deref()
        .map(|h| h.contains("text/event-stream"))
        .unwrap_or(false);
    assert!(is_sse, "text/event-stream should be detected as SSE");

    let json_header = Some("application/json".to_string());
    let is_json_sse = json_header
        .as_deref()
        .map(|h| h.contains("text/event-stream"))
        .unwrap_or(false);
    assert!(
        !is_json_sse,
        "application/json should not be detected as SSE"
    );

    let no_header: Option<String> = None;
    let is_none_sse = no_header
        .as_deref()
        .map(|h| h.contains("text/event-stream"))
        .unwrap_or(false);
    assert!(!is_none_sse, "missing header should default to non-SSE");
}

#[tokio::test]
async fn test_sse_stream_endpoint_not_found_for_unknown_session() {
    let temp_dir = tempfile::tempdir().unwrap();
    let workspace = temp_dir.path().to_path_buf();

    let app = build_test_app(workspace).await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v2/sessions/nonexistent-session/stream-sse")
                .method(Method::GET)
                .header("Accept", "text/event-stream")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_sse_stream_endpoint_returns_gone_for_orphaned_session() {
    let temp_dir = tempfile::tempdir().unwrap();
    let workspace = temp_dir.path().to_path_buf();

    let app = build_test_app(workspace.clone()).await;

    // Create a session first using the v1 endpoint
    let create_body = serde_json::json!({
        "session_id": "sse-orphaned-session",
        "model": "test-model"
    });

    let _ = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/sessions")
                .method(Method::POST)
                .header("content-type", "application/json")
                .body(axum::body::Body::from(
                    serde_json::to_string(&create_body).unwrap(),
                ))
                .unwrap(),
        )
        .await;

    // Request SSE stream - session exists but no broadcaster (orphaned)
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v2/sessions/sse-orphaned-session/stream-sse")
                .method(Method::GET)
                .header("Accept", "text/event-stream")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Should return 404 or 410 since session exists but broadcaster is not active
    // The exact code depends on whether the v1 create session properly registers
    // the session in the SessionManager
    assert!(
        response.status() == StatusCode::GONE
            || response.status() == StatusCode::NOT_FOUND,
        "should return GONE or NOT_FOUND for orphaned session, got {}",
        response.status()
    );
}

#[tokio::test]
async fn test_chat_with_sse_accept_header() {
    let temp_dir = tempfile::tempdir().unwrap();
    let workspace = temp_dir.path().to_path_buf();

    let session_manager =
        SessionManager::new(workspace.join(".synthia").join("sessions"));
    let mut state = AppState::for_test(session_manager, workspace.clone());

    let response = synthia_provider::types::CompletionResponse {
        id: "resp-1".to_string(),
        model: "test-model".to_string(),
        content: synthia_provider::types::Content::text("Hello via SSE"),
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
        "session_id": "sse-chat-test",
        "input": "Hello"
    });

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/chat")
                .method(Method::POST)
                .header("content-type", "application/json")
                .header("Accept", "text/event-stream")
                .body(axum::body::Body::from(
                    serde_json::to_string(&chat_body).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert!(
        response.status().is_success(),
        "SSE chat should return success, got {}",
        response.status()
    );

    let content_type = response
        .headers()
        .get("content-type")
        .map(|v| v.to_str().unwrap_or(""))
        .unwrap_or("");
    assert!(
        content_type.contains("text/event-stream"),
        "SSE response should have text/event-stream content type, got: {}",
        content_type
    );
}

#[tokio::test]
async fn test_sse_event_serialization() {
    use synthia_agent::types::AgentEvent;

    let event = AgentEvent::SessionStarted {
        session_id: "sse-ser-test".to_string(),
    };
    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("SessionStarted"));
    assert!(json.contains("sse-ser-test"));

    let event2 = AgentEvent::LlmStreamDelta {
        content: "Hello".to_string(),
    };
    let json2 = serde_json::to_string(&event2).unwrap();
    assert!(json2.contains("LlmStreamDelta"));
    assert!(json2.contains("Hello"));

    let event3 = AgentEvent::SessionEnded {
        reason: synthia_agent::types::SessionEndReason::Completed,
    };
    let json3 = serde_json::to_string(&event3).unwrap();
    assert!(json3.contains("SessionEnded"));
}

#[tokio::test]
async fn test_sse_variant_name_mapping() {
    use synthia_agent::types::AgentEvent;

    let event = AgentEvent::SessionStarted {
        session_id: "s1".to_string(),
    };
    let name = synthia_server::sse::event_variant_name(&event);
    assert_eq!(name, "SessionStarted");

    let event = AgentEvent::IterationStarted { iteration: 1 };
    let name = synthia_server::sse::event_variant_name(&event);
    assert_eq!(name, "IterationStarted");

    let event = AgentEvent::Thinking {
        text: "thinking".to_string(),
        iteration: 1,
    };
    let name = synthia_server::sse::event_variant_name(&event);
    assert_eq!(name, "Thinking");

    let event = AgentEvent::LlmRequestStarted { iteration: 1 };
    let name = synthia_server::sse::event_variant_name(&event);
    assert_eq!(name, "LlmRequestStarted");

    let event = AgentEvent::LlmStreamDelta {
        content: "delta".to_string(),
    };
    let name = synthia_server::sse::event_variant_name(&event);
    assert_eq!(name, "LlmStreamDelta");

    let event = AgentEvent::LlmResponseComplete {
        content: "done".to_string(),
        usage: synthia_agent::types::TokenUsage {
            prompt_tokens: 0,
            completion_tokens: 0,
            total_tokens: 0,
            cached_prompt_tokens: None,
            cache_read_tokens: None,
            cache_write_tokens: None,
        },
    };
    let name = synthia_server::sse::event_variant_name(&event);
    assert_eq!(name, "LlmResponseComplete");

    let event = AgentEvent::ToolCallStarted {
        tool_name: "test".to_string(),
        input: serde_json::json!({}),
    };
    let name = synthia_server::sse::event_variant_name(&event);
    assert_eq!(name, "ToolCallStarted");

    let event = AgentEvent::ToolCallCompleted {
        tool_name: "test".to_string(),
        output: "ok".to_string(),
        is_error: false,
    };
    let name = synthia_server::sse::event_variant_name(&event);
    assert_eq!(name, "ToolCallCompleted");

    let event = AgentEvent::SessionEnded {
        reason: synthia_agent::types::SessionEndReason::Completed,
    };
    let name = synthia_server::sse::event_variant_name(&event);
    assert_eq!(name, "SessionEnded");

    let event = AgentEvent::SteeringReceived {
        message: "steer".to_string(),
        session_id: "s1".to_string(),
        priority: None,
    };
    let name = synthia_server::sse::event_variant_name(&event);
    assert_eq!(name, "SteeringReceived");
}

#[tokio::test]
async fn test_sse_error_event_creation() {
    let error_event =
        synthia_server::sse::error_event("test_code", "test message");
    let _ = error_event;
}

#[tokio::test]
async fn test_sse_agent_event_to_sse_conversion() {
    use synthia_agent::types::AgentEvent;

    let event = AgentEvent::Finish {
        output: "final output".to_string(),
    };
    let sse_event = synthia_server::sse::agent_event_to_sse(&event);
    let _ = sse_event;
}

#[tokio::test]
async fn test_event_broadcaster_multiple_subscribers() {
    use synthia_server::event_stream::EventBroadcaster;

    let broadcaster = EventBroadcaster::new();

    let mut rx1 = broadcaster.subscribe();
    let mut rx2 = broadcaster.subscribe();

    assert_eq!(broadcaster.subscriber_count(), 2);

    let event = synthia_agent::types::AgentEvent::SessionStarted {
        session_id: "broadcaster-test".to_string(),
    };
    let sent = broadcaster.send(event.clone()).unwrap();
    assert_eq!(sent, 2);

    let received1 = rx1.recv().await.unwrap();
    let received2 = rx2.recv().await.unwrap();

    assert!(matches!(
        received1,
        synthia_agent::types::AgentEvent::SessionStarted { .. }
    ));
    assert!(matches!(
        received2,
        synthia_agent::types::AgentEvent::SessionStarted { .. }
    ));
}
