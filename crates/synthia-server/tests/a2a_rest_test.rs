//! A2A REST 端点集成测试。
//!
//! 验证 A2A 协议的 REST/HTTP+JSON 绑定端点：
//! - `POST /a2a/message:send` — 发送消息
//! - `GET /a2a/tasks` — 列出任务
//! - `GET /a2a/tasks/{id}` — 获取任务
//! - `POST /a2a/tasks/{id}:cancel` — 取消任务
//! - `GET /a2a/tasks/{id}:subscribe` — 订阅任务
//! - `GET /a2a/extendedAgentCard` — 扩展 Agent Card

use std::sync::Arc;

use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use synthia_server::{create_router, state::AppState};
use tower::ServiceExt;

async fn make_app() -> axum::Router {
    let temp = tempfile::TempDir::new().unwrap();
    let session_manager = synthia_session::manager::SessionManager::new(
        temp.path().to_path_buf(),
    );
    let state =
        AppState::for_test(session_manager, temp.path().to_path_buf()).await;
    create_router(Arc::new(state)).await
}

#[tokio::test]
async fn test_a2a_rest_send_message() {
    let app = make_app().await;

    let body = serde_json::json!({
        "message": {
            "messageId": "m1",
            "role": "ROLE_USER",
            "parts": [{"text": "hello"}]
        }
    });

    let req = Request::builder()
        .uri("/a2a/message:send")
        .method("POST")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(
            serde_json::to_string(&body).unwrap(),
        ))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    // Even if the executor can't really run without an LLM,
    // the REST routing should work (200 or a proper error).
    assert!(
        resp.status() == StatusCode::OK
            || resp.status() == StatusCode::INTERNAL_SERVER_ERROR
    );
}

#[tokio::test]
async fn test_a2a_rest_send_message_legacy_path() {
    let app = make_app().await;

    let body = serde_json::json!({
        "message": {
            "messageId": "m2",
            "role": "ROLE_USER",
            "parts": [{"text": "hello"}]
        }
    });

    let req = Request::builder()
        .uri("/a2a/message/send")
        .method("POST")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(
            serde_json::to_string(&body).unwrap(),
        ))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert!(
        resp.status() == StatusCode::OK
            || resp.status() == StatusCode::INTERNAL_SERVER_ERROR
    );
}

#[tokio::test]
async fn test_a2a_rest_list_tasks() {
    let app = make_app().await;

    let req = Request::builder()
        .uri("/a2a/tasks")
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let result: serde_json::Value = serde_json::from_slice(&body).unwrap();
    // protojson format: empty tasks list may be omitted, but pageSize should exist
    assert!(
        result["pageSize"].is_number(),
        "expected ListTasksResponse, got: {result}"
    );
}

#[tokio::test]
async fn test_a2a_rest_get_task_not_found() {
    let app = make_app().await;

    let req = Request::builder()
        .uri("/a2a/tasks/nonexistent-task")
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_a2a_rest_cancel_task_not_found() {
    let app = make_app().await;

    let req = Request::builder()
        .uri("/a2a/tasks/nonexistent-task:cancel")
        .method("POST")
        .header("content-type", "application/json")
        .body(axum::body::Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_a2a_rest_subscribe_task_not_found() {
    let app = make_app().await;

    let req = Request::builder()
        .uri("/a2a/tasks/nonexistent-task:subscribe")
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert!(resp.status().is_client_error());
}

#[tokio::test]
async fn test_a2a_rest_extended_agent_card() {
    let app = make_app().await;

    let req = Request::builder()
        .uri("/a2a/extendedAgentCard")
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    // Extended card is not configured by default → BAD_REQUEST
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_a2a_jsonrpc_send_message() {
    let app = make_app().await;

    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": "1",
        "method": "SendMessage",
        "params": {
            "message": {
                "messageId": "m1",
                "role": "ROLE_USER",
                "parts": [{"text": "hello"}]
            }
        }
    });

    let req = Request::builder()
        .uri("/a2a")
        .method("POST")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(
            serde_json::to_string(&body).unwrap(),
        ))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let resp_body = resp.into_body().collect().await.unwrap().to_bytes();
    let result: serde_json::Value = serde_json::from_slice(&resp_body).unwrap();
    // SendMessage returns either a result or an error (no LLM available)
    assert!(result["result"].is_object() || result["error"].is_object());
}

#[tokio::test]
async fn test_a2a_jsonrpc_list_tasks() {
    let app = make_app().await;

    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": "2",
        "method": "ListTasks",
        "params": {}
    });

    let req = Request::builder()
        .uri("/a2a")
        .method("POST")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(
            serde_json::to_string(&body).unwrap(),
        ))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let resp_body = resp.into_body().collect().await.unwrap().to_bytes();
    let result: serde_json::Value = serde_json::from_slice(&resp_body).unwrap();
    // JSON-RPC wraps in result; protojson may omit empty tasks list
    let inner = &result["result"];
    assert!(
        inner["pageSize"].is_number(),
        "expected ListTasksResponse in result, got: {result}"
    );
}

#[tokio::test]
async fn test_agent_card_declares_both_interfaces() {
    let app = make_app().await;

    let req = Request::builder()
        .uri("/.well-known/agent-card.json")
        .body(axum::body::Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let card: serde_json::Value = serde_json::from_slice(&body).unwrap();

    let interfaces = card["supportedInterfaces"].as_array().unwrap();
    let bindings: Vec<&str> = interfaces
        .iter()
        .filter_map(|i| i["protocolBinding"].as_str())
        .collect();
    assert!(
        bindings.contains(&"JSONRPC"),
        "AgentCard should declare JSONRPC interface, got: {bindings:?}"
    );
    assert!(
        bindings.contains(&"HTTP+JSON"),
        "AgentCard should declare HTTP+JSON interface, got: {bindings:?}"
    );
}
