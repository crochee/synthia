use std::{path::PathBuf, sync::Arc};

use axum::{
    Router,
    routing::{delete, get, post},
};

use super::{
    commands::{delete_command, get_command, list_commands},
    handlers::{health_check, list_models},
    mcp_handlers::{
        delete_mcp_server,
        discover_mcp_tools,
        get_mcp_server,
        list_mcp_servers,
        register_mcp_server,
    },
    providers::{
        delete_provider,
        get_provider,
        list_providers,
        register_provider,
    },
};
use crate::{
    approval::{list_approvals, resolve_approval, ws_approvals_handler},
    auth::auth_middleware,
    middleware::{auth::AuthLayer, tracing::RequestTracingLayer},
    routes,
    routes::v2,
    state::AppState,
};

pub async fn create_server(workspace_root: PathBuf) -> Router {
    let state = AppState::new(workspace_root);
    create_router(state)
}

/// Build a Router with a given AppState. Useful for testing.
pub fn create_router(state: Arc<AppState>) -> Router {
    let v2_routes = Router::new()
        .route(
            "/providers",
            get(v2::list_providers).post(v2::create_provider),
        )
        .route(
            "/providers/{name}",
            get(v2::get_provider).delete(v2::delete_provider),
        )
        .route("/skills", get(v2::list_skills).post(v2::create_skill))
        .route(
            "/skills/{name}",
            get(v2::get_skill).delete(v2::delete_skill),
        )
        .route("/skills/reload", post(v2::reload_skills))
        .route("/memory/search", get(v2::search_memory))
        .route(
            "/sessions",
            get(v2::list_sessions_v2).post(v2::create_session_v2),
        )
        .route(
            "/sessions/{id}",
            get(v2::get_session_detail).delete(v2::delete_session_v2),
        )
        .route("/sessions/{id}/prompts", post(v2::create_prompt))
        .route("/sessions/{id}/steering", post(v2::create_steering))
        .route("/sessions/{id}/cancel", post(v2::cancel_session))
        .route("/sessions/{id}/events", get(v2::session_events))
        .route("/sessions/{id}/messages", get(v2::list_messages))
        .route("/sessions/{id}/subagents", get(v2::list_subagents))
        .route("/sessions/{id}/stream-sse", get(routes::stream_sse_handler))
        .route(
            "/jobs",
            get(routes::job::list_jobs).post(routes::job::schedule_job),
        )
        .route("/jobs/{key}", delete(routes::job::remove_job))
        .route("/jobs/{key}/execute", post(routes::job::execute_job))
        .route("/jobs/{key}/pause", post(routes::job::toggle_pause))
        .layer(axum::middleware::from_fn(auth_middleware));

    // Versioned API routes under /api/v1/
    let v1_routes = Router::new()
        .route("/chat", post(routes::chat_handler))
        .route("/sessions", get(routes::list_sessions))
        .route("/sessions", post(routes::create_session))
        .route("/sessions/{id}", get(routes::get_session))
        .route("/sessions/{id}", delete(routes::delete_session))
        .route("/sessions/{id}/stream", get(routes::stream_handler))
        .route("/sessions/{id}/stream-sse", get(routes::stream_sse_handler))
        .route("/sessions/{id}/messages", get(routes::get_session_messages))
        .route("/sessions/{id}/messages", post(routes::send_message))
        .route("/sessions/{id}/tools", get(routes::get_session_tools))
        .route("/skills", get(routes::list_skills))
        .route("/skills", post(routes::register_skill))
        .route("/skills/{name}", get(routes::get_skill))
        .route("/skills/{name}", delete(routes::delete_skill))
        .route("/tools", get(routes::list_tools))
        .route("/tools", post(routes::register_tool))
        .route("/tools/{name}", get(routes::get_tool))
        .route("/tools/{name}", delete(routes::delete_tool))
        .route("/mcp", post(routes::mcp::handle_jsonrpc))
        .route("/mcp/servers", get(list_mcp_servers))
        .route("/mcp/servers", post(register_mcp_server))
        .route("/mcp/servers/{id}", get(get_mcp_server))
        .route("/mcp/servers/{id}/discover", post(discover_mcp_tools))
        .route("/mcp/servers/{id}", delete(delete_mcp_server))
        .route("/commands", get(list_commands))
        .route("/commands/{name}", get(get_command))
        .route("/commands/{name}", delete(delete_command))
        .route("/providers", get(list_providers))
        .route("/providers", post(register_provider))
        .route("/providers/{name}", get(get_provider))
        .route("/providers/{name}", delete(delete_provider))
        .route("/ws/session/{id}", get(routes::ws::ws_handler))
        .route("/health", get(health_check))
        .route("/models", get(list_models))
        .layer(axum::middleware::from_fn(deprecated_middleware));

    let deprecated_routes = Router::new()
        .route("/chat", post(routes::chat_handler))
        .route("/sessions", get(routes::list_sessions))
        .route("/sessions", post(routes::create_session))
        .route("/sessions/{id}", get(routes::get_session))
        .route("/sessions/{id}", delete(routes::delete_session))
        .route("/sessions/{id}/stream", get(routes::stream_handler))
        .route("/sessions/{id}/messages", get(routes::get_session_messages))
        .route("/sessions/{id}/messages", post(routes::send_message))
        .route("/sessions/{id}/tools", get(routes::get_session_tools))
        .route("/skills", get(routes::list_skills))
        .route("/tools", get(routes::list_tools))
        .route("/mcp", post(routes::mcp::handle_jsonrpc))
        .route("/mcp/servers", get(list_mcp_servers))
        .route("/mcp/servers", post(register_mcp_server))
        .route("/mcp/servers/{id}/discover", post(discover_mcp_tools))
        .route("/commands", get(list_commands))
        .route("/providers", get(list_providers))
        .route("/health", get(health_check))
        .route("/models", get(list_models))
        .layer(axum::middleware::from_fn(deprecated_middleware));

    // Approval routes under /api/approvals
    let approval_routes = Router::new()
        .route("/", get(list_approvals))
        .route("/{id}/resolve", post(resolve_approval))
        .layer(axum::middleware::from_fn(auth_middleware));

    let api_v1 = Router::new().nest("/v1", v1_routes);
    let api_v2 = Router::new().nest("/v2", v2_routes);
    let api_deprecated = Router::new().merge(deprecated_routes);

    // Apply middleware layers and assemble the router
    Router::new()
        .nest("/api", api_v1)
        .nest("/api", api_v2)
        .nest("/api", api_deprecated)
        .nest("/api/approvals", approval_routes)
        .route("/ws/approvals", get(ws_approvals_handler))
        .route("/health", get(health_check))
        .layer(RequestTracingLayer)
        .layer(AuthLayer::new(state.auth_config.clone()))
        .with_state(state)
}

/// Middleware that adds a Deprecation header to responses from old routes.
async fn deprecated_middleware(
    request: axum::http::Request<axum::body::Body>,
    next: axum::middleware::Next,
) -> axum::http::Response<axum::body::Body> {
    let mut response = next.run(request).await;
    response
        .headers_mut()
        .insert("Deprecation", "true".parse().unwrap());
    response
}
