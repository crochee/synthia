//! A2A 协议路由处理函数。
//!
//! 提供 `GET /.well-known/agent-card.json` 端点返回 A2A AgentCard，
//! 以及 JSON-RPC 和 REST 协议端点的初始化辅助函数。
//!
//! JSON-RPC 端点（`POST /a2a`）和 REST 端点（`/a2a/message:send`、
//! `/a2a/tasks` 等）通过 `A2aService::jsonrpc_app()` 和
//! `A2aService::rest_app()` 在 `create_router()` 中使用
//! `nest_service` 挂载，而非手动转发。

use std::sync::Arc;

use a2a_server::AgentCardProducer;
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode, header},
    response::IntoResponse,
};

use crate::state::AppState;

/// `GET /.well-known/agent-card.json` — 返回 A2A AgentCard。
///
/// A2A 协议发现端点，返回此 agent 的能力描述。
/// 包含 CORS 头以支持跨域发现。
pub async fn get_agent_card(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let a2a_service =
        state.a2a_service("http://localhost:3000".to_string()).await;
    let card = a2a_service.card_producer().card();

    let mut resp_headers = HeaderMap::new();

    // CORS headers for public discovery (mirrors a2a-server-lf behavior)
    let origin = headers
        .get(header::ORIGIN)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("*");

    if origin != "*" {
        resp_headers.insert(
            header::ACCESS_CONTROL_ALLOW_ORIGIN,
            origin.parse().unwrap_or_else(|_| "*".parse().unwrap()),
        );
        resp_headers.insert(
            header::ACCESS_CONTROL_ALLOW_CREDENTIALS,
            "true".parse().unwrap(),
        );
        resp_headers.insert(header::VARY, "Origin".parse().unwrap());
    } else {
        resp_headers
            .insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*".parse().unwrap());
    }

    (StatusCode::OK, resp_headers, Json(card.clone()))
}
