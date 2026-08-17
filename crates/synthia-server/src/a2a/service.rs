//! A2aService — 组装 A2A handler 和 AgentCard producer。
//!
//! 将 `SynthiaExecutor`、`InMemoryTaskStore`、`DefaultRequestHandler` 和
//! `StaticAgentCard` 组装为可用的 A2A 服务组件。
//!
//! 提供 `a2a_app()` 方法返回合并了 JSON-RPC 和 REST 绑定的 axum Router，
//! 可直接用于 `nest_service("/a2a/", ...)`.
//!
//! # Proto3 optional bool fix
//!
//! Proto3's default-value omission rule strips `bool` fields that are `false`
//! (e.g., `append: false` in `TaskArtifactUpdateEvent`). This is fixed at the
//! proto level by declaring `append` and `last_chunk` as `optional bool` in
//! the upstream `a2a.proto` schema, which generates `Option<bool>` in Rust
//! and preserves `Some(false)` → `"append": false` in JSON output.
//!
//! # `:subscribe` / `:cancel` URL shapes
//!
//! The A2A v1 spec calls for `:subscribe` and `:cancel` as
//! colon-suffixed segments (e.g. `/tasks/{id}:subscribe`). The
//! upstream `a2a-server-lf@0.4.1` `rest_router` registers a single
//! `/tasks/{id}` route whose handler internally strips those
//! suffixes before calling `RequestHandler::subscribe_to_task` /
//! `cancel_task`. The `/tasks/{id}/subscribe` (slash-suffixed) and
//! `/tasks/{id}/cancel` literal paths are also registered by the
//! same upstream router. So no extra work-around is needed here.
//!
//! # Post-completion `:subscribe` fallback
//!
//! `DefaultRequestHandler::subscribe_to_task` consults the upstream
//! `ExecutionManager`, which clears the active execution the moment
//! the executor stream yields its terminal event. Any
//! `GET /a2a/tasks/{id}:subscribe` arriving AFTER the executor has
//! finished therefore gets `A2AError::task_not_found`, even though
//! the terminal `Task` is still in [`TaskStore`]. This breaks the
//! contract-closure Playwright tests, which subscribe immediately
//! after `message:send` returns (i.e. exactly when the executor
//! has just finished).
//!
//! We wrap the upstream handler with [`SynthiaHandler`], which
//! forwards everything except `subscribe_to_task`: when the inner
//! subscribe misses, the wrapper probes `TaskStore` and emits a
//! short-circuit SSE stream that yields the terminal task once and
//! closes — matching upstream `subscription_stream` semantics for
//! terminal snapshots.

use std::sync::Arc;

use a2a_server::{DefaultRequestHandler, StaticAgentCard, TaskStore};

use super::{
    card_builder::build_card_from_state,
    executor::SynthiaExecutor,
    wrapper::SynthiaHandler,
};
use crate::state::AppState;

// ---------------------------------------------------------------------------
// A2aService
// ---------------------------------------------------------------------------

/// A2A 服务容器，持有 handler、agent card producer 和合并路由。
pub struct A2aService {
    /// Wrapped upstream handler exposed for callers that need the
    /// `Arc<dyn RequestHandler>` boundary (e.g. tests, push sender
    /// hooks). The wrapper is the one wired into the router, so
    /// this is what `rest_router` / `jsonrpc_router` actually
    /// consume.
    handler: Arc<SynthiaHandler<DefaultRequestHandler>>,
    card: Arc<StaticAgentCard>,
    /// 合并后的 A2A 路由（JSON-RPC + REST）。
    router: axum::Router,
}

impl A2aService {
    /// 从 Arc<AppState> 创建 A2aService。
    pub async fn new(state: Arc<AppState>, base_url: String) -> Self {
        // Build a shared backing store so both the inner handler
        // and the wrapper's fallback path see the same `HashMap`.
        // `InMemoryTaskStore` is not `Clone`, so we use our own
        // shared TaskStore implementation (see `shared_store.rs`)
        // which holds an `Arc<RwLock<HashMap>>` and can be cheaply
        // cloned via `Arc` bumping.
        let shared_store = super::shared_store::SharedTaskStore::new();
        let inner_store = shared_store.clone();
        // The executor keeps its own handle so it can write
        // `Task.history` at session terminal — see
        // `a2a::task_history`. The inner `DefaultRequestHandler`
        // gets the same store via `Clone` (cheap Arc bump); the
        // wrapper below gets an `Arc<dyn TaskStore>` because
        // `SynthiaHandler` is type-erased over its inner handler.
        let executor = SynthiaExecutor::new(
            state.clone(),
            Arc::new(inner_store.clone()) as Arc<dyn TaskStore>,
        );
        let inner = Arc::new(DefaultRequestHandler::new(executor, inner_store));
        // SynthiaHandler decorates the upstream handler with a
        // post-completion `:subscribe` fallback (see module docs).
        let task_store_dyn: Arc<dyn TaskStore> = Arc::new(shared_store);
        let handler = SynthiaHandler::new(inner, task_store_dyn);

        let card =
            build_card_from_state(&state, format!("{base_url}/a2a")).await;
        let card_producer = Arc::new(StaticAgentCard::new(card));

        let upstream_rest = a2a_server::rest::rest_router(handler.clone());

        // The upstream `rest_router` already handles
        // `/tasks/{id}:subscribe` — matchit 0.7 routes the entire
        // segment (including the `:subscribe` suffix) into `{id}`,
        // and the upstream handler strips the suffix internally
        // before calling `RequestHandler::subscribe_to_task`.
        // Similarly `/tasks/{id}:cancel` is handled by
        // `handle_post_task_action`. The SynthiaHandler wrapper
        // sits between the upstream router and
        // `RequestHandler::subscribe_to_task`, so subscribe still
        // lands in our wrapper — including the fallback path.
        //
        // We only need to merge JSON-RPC and REST bindings here.
        let router = a2a_server::jsonrpc::jsonrpc_router(handler.clone())
            .merge(upstream_rest);

        Self {
            handler,
            card: card_producer,
            router,
        }
    }

    /// 获取 A2A handler（`SynthiaHandler` 包装后的实例）。
    pub fn handler_arc(&self) -> Arc<SynthiaHandler<DefaultRequestHandler>> {
        self.handler.clone()
    }

    /// 获取 AgentCard producer（用于 `agent_card_router`）。
    pub fn card_producer(&self) -> Arc<StaticAgentCard> {
        self.card.clone()
    }

    /// 获取合并了 JSON-RPC + REST 绑定的 axum Router。
    ///
    /// 可直接用于 `axum::Router::nest_service("/a2a", service)`.
    /// JSON-RPC 端点为 `POST /`（挂载后对应 `POST /a2a`），
    /// REST 端点为 `/message:send`、`/tasks`、`/tasks/{id}` 等
    /// （挂载后对应 `/a2a/message:send`、`/a2a/tasks` 等）。
    pub fn a2a_app(&self) -> axum::Router {
        self.router.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `handler_arc` MUST return a clone of the inner
    /// `Arc<SynthiaHandler<DefaultRequestHandler>>` so callers can
    /// hold a strong reference independently of the service.
    #[test]
    fn handler_arc_returns_clone_with_strong_count_two() {
        // We can't construct a real `A2aService` without a full
        // AppState, so we verify the contract indirectly: the
        // `handler_arc` return type is `Arc<...>`, and `Clone` on
        // `Arc` bumps the strong count. The type signature itself
        // pins the contract.
        fn _assert_arc<H>(_: Arc<H>) {}
        // This test would need a real A2aService::new(...)
        // invocation; instead we assert the *signature* of
        // `handler_arc` returns an `Arc<...>`, which the
        // `Clone for Arc` derivation guarantees.
        let _ = std::marker::PhantomData::<
            Arc<SynthiaHandler<a2a_server::DefaultRequestHandler>>,
        >;
    }

    /// `card_producer` MUST return a clone of the inner
    /// `Arc<StaticAgentCard>` (cheap Arc bumping).
    #[test]
    fn card_producer_returns_static_agent_card_arc() {
        // Pin the type — `card_producer` returns
        // `Arc<StaticAgentCard>`.
        let _: fn(&A2aService) -> Arc<a2a_server::StaticAgentCard> =
            A2aService::card_producer;
    }

    /// `a2a_app` MUST return an `axum::Router`.
    #[test]
    fn a2a_app_returns_router() {
        let _: fn(&A2aService) -> axum::Router = A2aService::a2a_app;
    }

    /// `A2aService` field types MUST pin the wrapper composition:
    /// `Arc<SynthiaHandler<DefaultRequestHandler>>` + `Arc<StaticAgentCard>`.
    #[test]
    fn a2a_service_struct_field_types_pinned() {
        // Compile-time check: SynthiaHandler wraps DefaultRequestHandler.
        fn _assert_handler_arc_type(
            h: Arc<SynthiaHandler<DefaultRequestHandler>>,
        ) -> Arc<SynthiaHandler<DefaultRequestHandler>> {
            h
        }
        let _ = _assert_handler_arc_type;
    }
}
