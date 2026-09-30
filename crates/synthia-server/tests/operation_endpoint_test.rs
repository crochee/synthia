//! `POST /api/v1/chat/sessions/{id}/operation` gating (R29).
//!
//! The endpoint is opt-in via `[operations] enabled = true`. The
//! router consults `AppState.operations_config` at build time, so
//! these tests boot through the real `AppState::new` +
//! `create_router` path rather than calling the handler directly —
//! a regression that registers the route unconditionally, or that
//! drops the flag read, would pass a handler-level unit test and
//! still ship the wrong surface.

use std::sync::Arc;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use synthia_server::{create_router, state::AppState};
use tower::ServiceExt;

mod support;

use support::install_test_provider;

/// Boot a workspace whose `config.toml` carries `config_json`,
/// then hand back the state.
async fn boot(
    config_json: serde_json::Value,
) -> (tempfile::TempDir, Arc<AppState>) {
    let temp = tempfile::TempDir::new().unwrap();
    install_test_provider(&temp);
    std::fs::write(
        temp.path().join("config.toml"),
        serde_json::to_string(&config_json).unwrap(),
    )
    .unwrap();
    let state = AppState::new(temp.path().to_path_buf(), None)
        .await
        .expect("boot must succeed");
    (temp, state)
}

/// POST one `OperationRequest` body at the operation endpoint and
/// return `(status, body)`.
async fn post_operation(
    state: Arc<AppState>,
    session_id: &str,
    body: serde_json::Value,
) -> (StatusCode, serde_json::Value) {
    let router = create_router(state).await;
    let response = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/chat/sessions/{session_id}/operation"))
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value =
        serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    (status, json)
}

/// Flag off (the default): the route is not registered, so the
/// request falls through to the standard 404 envelope.
#[tokio::test]
async fn operation_endpoint_is_404_when_disabled() {
    // No server config at all → default (disabled).
    let temp = tempfile::TempDir::new().unwrap();
    install_test_provider(&temp);
    let state = AppState::new(temp.path().to_path_buf(), None)
        .await
        .expect("boot must succeed");
    let (status, body) = post_operation(
        state,
        "sess-off",
        serde_json::json!({ "kind": "prompt", "text": "hi" }),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["code"], "not_found");
}

/// Flag on: `Prompt` dispatches to the existing prompt path and
/// answers the standard queued-turn envelope.
#[tokio::test]
async fn operation_prompt_queues_a_turn_when_enabled() {
    let (_temp, state) =
        boot(serde_json::json!({ "operations": { "enabled": true } })).await;
    let (status, body) = post_operation(
        state,
        "sess-on",
        serde_json::json!({ "kind": "prompt", "text": "hi" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert_eq!(body["queued"], true);
    assert!(body["message_id"].is_string());
}

/// Flag on: the declared-but-unimplemented variants answer 501
/// with the standard envelope so a client can distinguish "not
/// built yet" from "bad request".
#[tokio::test]
async fn operation_unimplemented_kind_answers_501() {
    let (_temp, state) =
        boot(serde_json::json!({ "operations": { "enabled": true } })).await;
    let (status, body) = post_operation(
        state,
        "sess-501",
        serde_json::json!({ "kind": "compaction", "force": true }),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_IMPLEMENTED);
    assert_eq!(body["code"], "not_implemented");
    assert!(
        body["message"]
            .as_str()
            .is_some_and(|m| m.contains("compaction")),
        "message must name the refused kind; got: {body}"
    );
}

/// An unknown `kind` tag is a malformed body, not a 501 — the
/// extractor rejects it before the handler runs.
#[tokio::test]
async fn operation_unknown_kind_is_rejected_at_the_extractor() {
    let (_temp, state) =
        boot(serde_json::json!({ "operations": { "enabled": true } })).await;
    let (status, _body) = post_operation(
        state,
        "sess-bad",
        serde_json::json!({ "kind": "teleport" }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}
