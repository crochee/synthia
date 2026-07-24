use std::{path::PathBuf, sync::Arc};

use axum::{
    Router,
    http::HeaderValue,
    routing::{delete, get, post},
};
use tower_http::cors::CorsLayer;

use crate::{
    approval::{list_approvals, resolve_approval, ws_approvals_handler},
    auth::auth_middleware,
    config::server::CorsConfig,
    middleware::{auth::AuthLayer, tracing::RequestTracingLayer},
    routes,
    state::AppState,
};

/// Build a CORS layer from server configuration.
fn build_cors_layer(config: &CorsConfig) -> CorsLayer {
    if !config.enabled {
        return CorsLayer::new();
    }

    let origins: Vec<HeaderValue> = config
        .allowed_origins
        .iter()
        .filter_map(|o| o.parse().ok())
        .collect();

    let methods: Vec<axum::http::Method> = config
        .allowed_methods
        .iter()
        .filter_map(|m| m.parse().ok())
        .collect();

    CorsLayer::new()
        .allow_origin(origins)
        .allow_methods(methods)
        .allow_headers(
            config
                .allowed_headers
                .iter()
                .filter_map(|h| h.parse().ok())
                .collect::<Vec<_>>(),
        )
}

pub async fn create_server(workspace_root: PathBuf) -> Router {
    let state = AppState::new(workspace_root).await;
    create_router(state).await
}

/// Build a Router with a given AppState. Useful for testing.
///
/// # Route Layout
///
/// Agent interaction is provided **exclusively** via the A2A protocol
/// (JSON-RPC + REST/HTTP+JSON). All agent communication flows through `/a2a`.
///
/// Infrastructure management endpoints are flat under `/api/`
/// (no versioning — fast iteration phase).
pub async fn create_router(state: Arc<AppState>) -> Router {
    // --- Management routes (auth-protected) ---
    let api_routes = Router::new()
        // Models listing
        .route("/models", get(routes::health::list_models))
        // Provider management
        .route(
            "/providers",
            get(routes::providers::list_providers)
                .post(routes::providers::create_provider),
        )
        .route(
            "/providers/{name}",
            get(routes::providers::get_provider)
                .delete(routes::providers::delete_provider),
        )
        // Skill management
        .route(
            "/skills",
            get(routes::skills::list_skills).post(routes::skills::create_skill),
        )
        .route(
            "/skills/{name}",
            get(routes::skills::get_skill).delete(routes::skills::delete_skill),
        )
        .route("/skills/reload", post(routes::skills::reload_skills))
        // Memory search
        .route("/memory/search", get(routes::memory::search_memory))
        // Job scheduling
        .route(
            "/jobs",
            get(routes::job::list_jobs).post(routes::job::schedule_job),
        )
        .route("/jobs/{key}", delete(routes::job::remove_job))
        .route("/jobs/{key}/execute", post(routes::job::execute_job))
        .route("/jobs/{key}/pause", post(routes::job::toggle_pause))
        // MCP management
        .route("/mcp", post(routes::mcp::handle_jsonrpc))
        .route("/mcp/servers", get(routes::mcp_servers::list_mcp_servers))
        .route(
            "/mcp/servers",
            post(routes::mcp_servers::register_mcp_server),
        )
        .route(
            "/mcp/servers/{id}",
            get(routes::mcp_servers::get_mcp_server),
        )
        .route(
            "/mcp/servers/{id}/discover",
            post(routes::mcp_servers::discover_mcp_tools),
        )
        .route(
            "/mcp/servers/{id}",
            delete(routes::mcp_servers::delete_mcp_server),
        )
        // Tool management
        .route("/tools", get(routes::list_tools))
        .route("/tools", post(routes::register_tool))
        .route("/tools/{name}", get(routes::get_tool))
        .route("/tools/{name}", delete(routes::delete_tool))
        // Command management
        .route("/commands", get(routes::commands::list_commands))
        .route("/commands/{name}", get(routes::commands::get_command))
        .route(
            "/commands/{name}",
            delete(routes::commands::delete_command),
        )
        // Settings (per-user overrides)
        .route(
            "/settings",
            get(routes::settings::get_settings).put(routes::settings::put_settings),
        )
        .layer(axum::middleware::from_fn(auth_middleware));

    // Approval routes
    let approval_routes = Router::new()
        .route("/", get(list_approvals))
        .route("/{id}/resolve", post(resolve_approval))
        .layer(axum::middleware::from_fn(auth_middleware));

    // A2A protocol: initialize the service eagerly so we can
    // extract the merged JSON-RPC + REST router for nest_service.
    // Use empty string as base URL to generate relative URLs in Agent Card.
    // This ensures the SDK uses the same origin as the frontend (through Vite proxy).
    let a2a_service = state.a2a_service("".to_string()).await;

    Router::new()
        // --- Infrastructure management (flat /api/) ---
        .nest("/api", api_routes)
        .nest("/api/approvals", approval_routes)
        .route("/ws/approvals", get(ws_approvals_handler))
        .route("/health", get(routes::health::health_check))
        // --- A2A protocol: sole agent interaction interface ---
        .route(
            "/.well-known/agent-card.json",
            get(routes::a2a::get_agent_card),
        )
        .nest_service("/a2a", a2a_service.a2a_app())
        .layer(build_cors_layer(&state.cors_config))
        .layer(RequestTracingLayer)
        .layer(AuthLayer::new(state.auth_config.clone()))
        .with_state(state)
}
