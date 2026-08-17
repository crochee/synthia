use std::{path::PathBuf, sync::Arc};

use axum::{
    Router,
    http::{HeaderName, HeaderValue, Method},
    middleware::from_fn,
    routing::get,
};
use tower_http::cors::CorsLayer;

use crate::{
    config::server::CorsConfig,
    middleware::{
        auth::AuthLayer,
        error_handler::error_handler_middleware,
        response_headers::response_headers_middleware,
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

pub async fn create_server(
    workspace_root: PathBuf,
    config_path: Option<&PathBuf>,
) -> Result<Router, String> {
    let state = AppState::new(workspace_root, config_path).await?;
    Ok(create_router(state).await)
}

/// Build a Router with a given AppState. Useful for testing.
///
/// # Route Layout
///
/// Agent interaction is provided **exclusively** via the A2A protocol
/// (JSON-RPC + REST/HTTP+JSON). All agent communication flows through `/a2a`.
///
/// Infrastructure management endpoints are versioned under `/api/v1/`.
pub async fn create_router(state: Arc<AppState>) -> Router {
    // --- Management routes (auth-protected) ---
    let api_routes = Router::new()
        // Models listing
        .route("/models", get(routes::health::list_models))
        // A2A task list (management view backed by the shared task store)
        .route("/tasks", get(routes::tasks::list_tasks))
        .route("/tasks/{id}", get(routes::tasks::get_task))
        // Skill management (CRUD restored in turn 13 to address Task 3).
        // GET    /skills            → list_skills (cursor-paginated)
        // POST   /skills            → create_skill (write SKILL.md)
        // GET    /skills/{name}     → get_skill (frontmatter + body)
        // DELETE /skills/{name}     → delete_skill (remove directory)
        // POST   /skills/reload     → reload_skills (rescan)
        .route(
            "/skills",
            get(routes::skills::list_skills).post(routes::skills::create_skill),
        )
        .route("/skills/reload", axum::routing::post(routes::skills::reload_skills))
        // Agent management (descriptor registration + lifecycle)
        .route(
            "/agents",
            get(routes::agents::list_agents)
                .post(routes::agents::create_agent),
        )
        .route(
            "/agents/{name}",
            get(routes::agents::get_agent)
                .delete(routes::agents::delete_agent),
        )
        .route(
            "/skills/{name}",
            get(routes::skills::get_skill)
                .delete(routes::skills::delete_skill),
        )
        // Memory search
        .route("/memory/search", get(routes::memory::search_memory))
        // Tool management (CRUD restored in turn 13 to address Task 3).
        // GET    /tools            → list_tools
        // POST   /tools            → register_tool
        // GET    /tools/{name}     → get_tool
        // DELETE /tools/{name}     → unregister_tool
        .route(
            "/tools",
            get(routes::list_tools).post(routes::register_tool),
        )
        .route(
            "/tools/{name}",
            get(routes::get_tool).delete(routes::unregister_tool),
        );

    // A2A protocol: initialize the service eagerly so we can
    // extract the merged JSON-RPC + REST router for nest_service.
    // Use empty string as base URL to generate relative URLs in Agent Card.
    // This ensures the SDK uses the same origin as the frontend (through Vite proxy).
    let a2a_service = state.a2a_service(None).await;

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
        // --- Infrastructure management (versioned /api/v1/) ---
        .nest("/api/v1", api_routes)
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
        .merge(protected)
        // Outermost layer: convert any handler panic into a
        // well-formed JSON 500 response with the standard
        // `{ status: "error", error: { code, message } }` envelope
        // — without this, a panic leaks an opaque "Internal Server
        // Error" page and skips the project's normal error
        // envelope, breaking the front-end ApiClient.toError
        // contract (it expects the envelope to extract a
        // human-readable message). Placed above
        // `response_headers_middleware` so the synthesized 500
        // response still receives `Cache-Control: no-store` on
        // `/api/v1/*` and `Server-Timing` elsewhere.
        .layer(from_fn(error_handler_middleware))
        // Outermost layer: stamp `Cache-Control: no-store` on
        // `/api/v1/*` responses and `Server-Timing` on all
        // non-streaming responses. Placed *outside* the
        // `RequestTracingLayer` so the header is appended after
        // the tracing span is finalized — the timing measurement
        // therefore reflects end-to-end handler latency, not just
        // the inner middleware stack.
        //
        // Cheap to apply broadly: the middleware only does two
        // prefix comparisons and one `Instant::now()` roundtrip
        // per request, so leaving it mounted on every route is
        // cheaper than conditionally routing it.
        .layer(from_fn(response_headers_middleware))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::server::CorsConfig;

    // -- build_cors_layer -----------------------------------------

    /// `build_cors_layer` MUST accept an empty config (all Any)
    /// without panicking.
    #[test]
    fn build_cors_empty_config_does_not_panic() {
        let cfg = CorsConfig::default();
        let _layer = build_cors_layer(&cfg);
    }

    /// `build_cors_layer` MUST accept a fully-locked-down config
    /// (all explicit) without panicking.
    #[test]
    fn build_cors_explicit_config_does_not_panic() {
        let cfg = CorsConfig {
            allowed_origins: vec!["https://app.example.com".to_string()],
            allowed_methods: vec!["GET".to_string(), "POST".to_string()],
            allowed_headers: vec!["authorization".to_string()],
        };
        let _layer = build_cors_layer(&cfg);
    }

    /// `build_cors_layer` MUST silently skip malformed origin
    /// strings (filter_map to None).
    #[test]
    fn build_cors_skips_malformed_origins() {
        let cfg = CorsConfig {
            // Newline and space chars are invalid in HTTP header values.
            allowed_origins: vec!["bad\norigin".to_string(), "".to_string()],
            allowed_methods: vec![],
            allowed_headers: vec![],
        };
        // Should not panic even if the filter_map drops everything.
        let _layer = build_cors_layer(&cfg);
    }

    /// `build_cors_layer` MUST silently skip malformed method names.
    #[test]
    fn build_cors_skips_malformed_methods() {
        let cfg = CorsConfig {
            allowed_origins: vec![],
            allowed_methods: vec!["not a method".to_string()],
            allowed_headers: vec![],
        };
        let _layer = build_cors_layer(&cfg);
    }

    /// `build_cors_layer` MUST silently skip malformed header names.
    #[test]
    fn build_cors_skips_malformed_headers() {
        let cfg = CorsConfig {
            allowed_origins: vec![],
            allowed_methods: vec![],
            allowed_headers: vec!["not a header".to_string()],
        };
        let _layer = build_cors_layer(&cfg);
    }

    /// `build_cors_layer` MUST preserve dimension independence —
    /// explicit origins with empty methods/headers still produces a
    /// valid layer.
    #[test]
    fn build_cors_origin_only() {
        let cfg = CorsConfig {
            allowed_origins: vec!["https://a.com".to_string()],
            allowed_methods: vec![],
            allowed_headers: vec![],
        };
        let _layer = build_cors_layer(&cfg);
    }

    /// `build_cors_layer` MUST preserve dimension independence —
    /// empty origins with explicit methods/headers still produces a
    /// valid layer.
    #[test]
    fn build_cors_methods_headers_only() {
        let cfg = CorsConfig {
            allowed_origins: vec![],
            allowed_methods: vec!["GET".to_string()],
            allowed_headers: vec!["x-tenant".to_string()],
        };
        let _layer = build_cors_layer(&cfg);
    }

    /// Multiple valid origins MUST all be accepted.
    #[test]
    fn build_cors_multiple_origins() {
        let cfg = CorsConfig {
            allowed_origins: vec![
                "https://a.com".to_string(),
                "https://b.com".to_string(),
                "http://localhost:3000".to_string(),
            ],
            allowed_methods: vec![],
            allowed_headers: vec![],
        };
        let _layer = build_cors_layer(&cfg);
    }

    /// `CorsConfig::default()` MUST be empty (all Any).
    #[test]
    fn cors_config_default_is_empty() {
        let cfg = CorsConfig::default();
        assert!(cfg.allowed_origins.is_empty());
        assert!(cfg.allowed_methods.is_empty());
        assert!(cfg.allowed_headers.is_empty());
    }
}
