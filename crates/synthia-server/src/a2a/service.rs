//! A2aService — 组装 A2A handler 和 AgentCard producer。
//!
//! 将 `SynthiaExecutor`、`InMemoryTaskStore`、`DefaultRequestHandler` 和
//! `StaticAgentCard` 组装为可用的 A2A 服务组件。
//!
//! 提供 `a2a_app()` 方法返回合并了 JSON-RPC 和 REST 绑定的 axum Router，
//! 可直接用于 `nest_service("/a2a", ...)`.

use std::sync::Arc;

use a2a_server::{DefaultRequestHandler, InMemoryTaskStore, StaticAgentCard};

use super::{card_builder::build_card_from_state, executor::SynthiaExecutor};
use crate::state::AppState;

/// A2A 服务容器，持有 handler、agent card producer 和合并路由。
pub struct A2aService {
    handler: Arc<DefaultRequestHandler>,
    card: Arc<StaticAgentCard>,
    /// 合并后的 A2A 路由（JSON-RPC + REST）。
    router: axum::Router,
}

impl A2aService {
    /// 从 Arc<AppState> 创建 A2aService。
    pub async fn new(state: Arc<AppState>, base_url: String) -> Self {
        let executor = SynthiaExecutor::new(state.clone());
        let task_store = InMemoryTaskStore::new();
        let handler =
            Arc::new(DefaultRequestHandler::new(executor, task_store));

        let card =
            build_card_from_state(&state, format!("{base_url}/a2a")).await;
        let card_producer = Arc::new(StaticAgentCard::new(card));

        // 合并 JSON-RPC 和 REST 路由为单个 Router。
        // JSON-RPC 端点：POST / → 挂载后 POST /a2a
        // REST 端点：/message:send, /tasks, /tasks/{id} 等
        let router = a2a_server::jsonrpc::jsonrpc_router(handler.clone())
            .merge(a2a_server::rest::rest_router(handler.clone()));

        Self {
            handler,
            card: card_producer,
            router,
        }
    }

    /// 获取 A2A JSON-RPC handler。
    pub fn handler_arc(&self) -> Arc<DefaultRequestHandler> {
        self.handler.clone()
    }

    /// 获取 AgentCard producer（用于 `agent_card_router`）。
    pub fn card_producer(&self) -> Arc<StaticAgentCard> {
        self.card.clone()
    }

    /// 获取合并了 JSON-RPC 和 REST 绑定的 axum Router（state 已内化）。
    ///
    /// 可直接用于 `axum::Router::nest_service("/a2a", service)`.
    /// JSON-RPC 端点为 `POST /`（挂载后对应 `POST /a2a`），
    /// REST 端点为 `/message:send`、`/tasks`、`/tasks/{id}` 等
    /// （挂载后对应 `/a2a/message:send`、`/a2a/tasks` 等）。
    pub fn a2a_app(&self) -> axum::Router {
        self.router.clone()
    }
}
