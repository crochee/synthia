//! A2A 端点集成测试。
//!
//! 验证 A2A 协议端点符合标准：
//! - `GET /.well-known/agent-card.json` — AgentCard 发现
//! - `POST /a2a` — JSON-RPC 方法处理

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
async fn test_agent_card_endpoint_returns_valid_card() {
    let app = make_app().await;

    let req = Request::builder()
        .uri("/.well-known/agent-card.json")
        .body(axum::body::Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let card: serde_json::Value = serde_json::from_slice(&body).unwrap();

    // Verify required AgentCard fields
    assert_eq!(card["name"], "Synthia");
    assert!(card["description"].is_string());
    assert!(card["version"].is_string());
    assert!(card["skills"].is_array());
    assert!(card["capabilities"]["streaming"].is_boolean());
    assert!(card["defaultInputModes"].is_array());
    assert!(card["defaultOutputModes"].is_array());
    assert!(card["supportedInterfaces"].is_array());

    // Verify CORS header
    // (No Origin header → access-control-allow-origin: *)
}

#[tokio::test]
async fn test_agent_card_endpoint_with_origin_returns_cors() {
    let app = make_app().await;

    let req = Request::builder()
        .uri("/.well-known/agent-card.json")
        .header("origin", "https://example.com")
        .body(axum::body::Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let origin = resp
        .headers()
        .get("access-control-allow-origin")
        .unwrap()
        .to_str()
        .unwrap();
    assert_eq!(origin, "https://example.com");
}

#[tokio::test]
async fn test_a2a_jsonrpc_invalid_method_returns_error() {
    let app = make_app().await;

    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": "1",
        "method": "unknown.method",
        "params": null
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

    assert!(result["error"].is_object());
    assert_eq!(result["error"]["code"], -32601); // Method not found
}

#[tokio::test]
async fn test_a2a_jsonrpc_invalid_version_returns_error() {
    let app = make_app().await;

    let body = serde_json::json!({
        "jsonrpc": "1.0",
        "id": "1",
        "method": "message/send",
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

    assert!(result["error"].is_object());
    assert_eq!(result["error"]["code"], -32600); // Invalid request
}

#[tokio::test]
async fn test_a2a_jsonrpc_get_task_not_found() {
    let app = make_app().await;

    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": "1",
        "method": "GetTask",
        "params": {
            "id": "nonexistent-task"
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

    assert!(result["error"].is_object());
    assert_eq!(result["error"]["code"], -32001); // Task not found
}

#[tokio::test]
async fn test_a2a_jsonrpc_empty_method_returns_error() {
    let app = make_app().await;

    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": "1",
        "method": "",
        "params": null
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

    assert!(result["error"].is_object());
}
