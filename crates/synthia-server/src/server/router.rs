use std::{path::PathBuf, sync::Arc};

use axum::{
    Router,
    http::{HeaderName, HeaderValue, Method},
    middleware::from_fn,
    routing::{delete, get, post},
};
use tower_http::cors::CorsLayer;

use crate::{
    approval::{list_approvals, resolve_approval, ws_approvals_handler},
    config::server::CorsConfig,
    middleware::{
        auth::AuthLayer,
        trace_context::trace_context_middleware,
        tracing::RequestTracingLayer,
    },
    routes,
    state::AppState,
};

/// Build a CORS layer from server configuration.
///
/// Per-dimension semantics (independent):
/// - `allowed_origins` empty → `Any`
/// - `allowed_methods`  empty → `Any`
/// - `allowed_headers`  empty → `Any`
///
/// Operators can opt into a fully locked-down policy by listing explicit
/// values in all three lists.
fn build_cors_layer(config: &CorsConfig) -> CorsLayer {
    use tower_http::cors::{AllowHeaders, AllowMethods, AllowOrigin};

    let origins: Vec<HeaderValue> = config
        .allowed_origins
        .iter()
        .filter_map(|o| o.parse().ok())
        .collect();
    let methods: Vec<Method> = config
        .allowed_methods
        .iter()
        .filter_map(|m| m.parse().ok())
        .collect();
    let headers: Vec<HeaderName> = config
        .allowed_headers
        .iter()
        .filter_map(|h| h.parse().ok())
        .collect();

    let origin = if origins.is_empty() {
        AllowOrigin::any()
    } else {
        AllowOrigin::list(origins)
    };
    let method = if methods.is_empty() {
        AllowMethods::any()
    } else {
        AllowMethods::list(methods)
    };
    let header = if headers.is_empty() {
        AllowHeaders::any()
    } else {
        AllowHeaders::list(headers)
    };

    CorsLayer::new()
        .allow_origin(origin)
        .allow_methods(method)
        .allow_headers(header)
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
        // A2A task list (management view backed by the shared task store)
        .route("/tasks", get(routes::tasks::list_tasks))
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
        );

    // Approval routes
    let approval_routes = Router::new()
        .route("/", get(list_approvals))
        .route("/{id}/resolve", post(resolve_approval));

    // A2A protocol: initialize the service eagerly so we can
    // extract the merged JSON-RPC + REST router for nest_service.
    // Use empty string as base URL to generate relative URLs in Agent Card.
    // This ensures the SDK uses the same origin as the frontend (through Vite proxy).
    let a2a_service = state.a2a_service("".to_string()).await;

    // WebSocket routes perform their own auth via `?token=xxx` query
    // parameter (browser WS clients cannot set Authorization headers),
    // so they must NOT be wrapped by the HTTP-style AuthLayer.
    let ws_routes =
        Router::new().route("/ws/approvals", get(ws_approvals_handler));

    // Public infrastructure endpoints.
    //
    // These are intentionally mounted OUTSIDE the protected router:
    // - `/health` is a liveness probe hit by orchestrators (k8s,
    //   load balancers) every second; emitting an access log /
    //   traceparent on every probe floods the log pipeline and
    //   burns trace ids.
    // - `/.well-known/agent-card.json` is fetched *by external
    //   agents / scanners* to discover the A2A interface; it has
    //   no caller identity to authenticate and no per-request
    //   work worth tracing.
    //
    // Skipping the AuthLayer, trace-context middleware, and
    // access-log span keeps both endpoints predictable and cheap.
    // The CORS layer is still applied so cross-origin browsers
    // can still call them.
    let public = Router::new()
        .route("/health", get(routes::health::health_check))
        .route(
            "/.well-known/agent-card.json",
            get(routes::a2a::get_agent_card),
        )
        .layer(build_cors_layer(&state.cors_config));

    let protected = Router::new()
        // --- Infrastructure management (flat /api/) ---
        .nest("/api", api_routes)
        .nest("/api/approvals", approval_routes)
        // --- A2A protocol: sole agent interaction interface ---
        .nest_service("/a2a", a2a_service.a2a_app())
        .layer(build_cors_layer(&state.cors_config))
        .layer(AuthLayer::new(state.auth_config.clone()))
        // Inner middleware: extracts the W3C `traceparent` header
        // (or mints a fresh one), records `trace_id` / `span_id`
        // on the surrounding span, then re-emits a `traceparent`
        // on the response so the caller can stitch their side of
        // the trace.
        .layer(from_fn(trace_context_middleware))
        // Outer middleware: creates the `http_request` span and
        // emits an access log line for every response. Because
        // axum layers compose with later layers wrapping earlier
        // ones, adding `RequestTracingLayer` last makes it the
        // outermost layer — so the trace-context middleware runs
        // *inside* the span and the `trace_id` / `span_id` fields
        // it records land on every log line emitted by handlers.
        .layer(RequestTracingLayer);

    Router::new()
        .merge(public)
        .merge(ws_routes)
        .merge(protected)
        .with_state(state)
}
