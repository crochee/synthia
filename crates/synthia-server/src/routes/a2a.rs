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

use a2a::AgentCard;
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode, header},
    response::IntoResponse,
};

use crate::state::AppState;

/// Build an absolute base URL from the incoming request's Host + scheme.
///
/// Falls back to "http" if the scheme cannot be determined (e.g. raw HTTP
/// from curl during local testing). This ensures the `AgentCard.url` field
/// is always absolute, which is required by the v1.0 A2A SDK.
fn absolute_base_url(headers: &HeaderMap) -> String {
    let host = headers
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("localhost:8080");
    let scheme = if headers
        .get("X-Forwarded-Proto")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.eq_ignore_ascii_case("https"))
        .unwrap_or(false)
    {
        "https"
    } else {
        "http"
    };
    format!("{scheme}://{host}")
}

/// `GET /.well-known/agent-card.json` — 返回 A2A AgentCard。
///
/// A2A 协议发现端点，返回此 agent 的能力描述。
/// 跨域头由顶层 `CorsLayer` 统一注入（默认允许全部 origin），本处理器
/// 不再手动设置 CORS 响应头，避免与全局层冲突。
///
/// `supportedInterfaces[].url` 字段使用请求的 Host 头构造绝对 URL，
/// 这样 v1.0 SDK 的 `JsonRpcTransport` 可以直接 fetch。
pub async fn get_agent_card(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    // Build absolute URL from the incoming Host header so the SDK
    // can fetch directly (Node `fetch` does not accept relative URLs).
    let base_url = absolute_base_url(&headers);
    let _ = state.a2a_service(base_url.clone()).await;
    let card: AgentCard = synthia_a2a::card::build_agent_card(
        "Synthia".to_string(),
        "AI coding assistant powered by Synthia".to_string(),
        env!("CARGO_PKG_VERSION").to_string(),
        format!("{}/a2a", base_url.trim_end_matches('/')),
        crate::a2a::card_builder::collect_skills(&state).await,
    );

    (StatusCode::OK, Json(card))
}
