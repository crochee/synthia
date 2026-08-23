use std::{path::PathBuf, sync::Arc};

use axum::{
    Router,
    http::{HeaderName, HeaderValue, Method},
    middleware::from_fn,
    routing::get,
};
use tower_http::cors::CorsLayer;

use crate::{
    api::error::AppError,
    config::server::CorsConfig,
    middleware::{
        auth::AuthLayer,
        error_handler::error_handler_middleware,
        response_headers::response_headers_middleware,
        trace_context::trace_context_middleware,
        tracing::RequestTracingLayer,
        track_metrics,
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
/// Agent interaction is provided via the REST + SSE chat
/// surface at `/api/v1/chat/*`. All agent communication —
/// session creation, message dispatch, streaming,
/// cancel/regenerate/edit/feedback, and usage telemetry —
/// flows through that surface.
///
/// Infrastructure management endpoints (agents / skills /
/// tools / memory / sessions / models) are versioned under
/// `/api/v1/`.
pub async fn create_router(state: Arc<AppState>) -> Router {
    // --- Management routes (auth-protected) ---
    let api_routes = Router::new()
        // Models listing
        .route("/models", get(routes::health::list_models))
        // Session list (management view backed by the shared session store)
        .route("/sessions", get(routes::sessions::list_sessions))
        .route("/sessions/{id}", get(routes::sessions::get_session))
        // --- Chat (REST + SSE) interface ---
        // POST   /chat/sessions                        → create_session
        // POST   /chat/sessions/{id}/messages          → send_message
        // GET    /chat/sessions/{id}/messages/stream   → stream_messages (SSE)
        // POST   /chat/sessions/{id}/cancel            → cancel_session
        // POST   /chat/sessions/{id}/regenerate        → regenerate
        // PATCH  /chat/sessions/{id}/messages/{mid}    → edit_message
        // POST   /chat/messages/{mid}/feedback         → feedback
        // GET    /chat/usage                           → usage
        //
        // Listing / detail live on the management surface
        // (`/api/v1/sessions`, `/api/v1/sessions/{id}`) so there
        // is exactly one canonical SessionSummary / SessionDetail
        // shape on the wire.
        .route("/chat/usage", get(routes::chat::usage))
        .route(
            "/chat/sessions",
            axum::routing::post(routes::chat::create_session),
        )
        .route(
            "/chat/sessions/{id}/messages",
            axum::routing::post(routes::chat::send_message),
        )
        .route(
            "/chat/sessions/{id}/messages/stream",
            get(routes::chat::stream_messages),
        )
        .route(
            "/chat/sessions/{id}/cancel",
            axum::routing::post(routes::chat::cancel_session),
        )
        .route(
            "/chat/sessions/{id}/regenerate",
            axum::routing::post(routes::chat::regenerate),
        )
        .route(
            "/chat/sessions/{id}/messages/{message_id}",
            axum::routing::patch(routes::chat::edit_message),
        )
        .route(
            "/chat/messages/{message_id}/feedback",
            axum::routing::post(routes::chat::feedback),
        )
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
            "/agents/default",
            get(routes::agents::get_default_agent),
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

    // Public infrastructure endpoints.
    //
    // These are intentionally mounted OUTSIDE the protected router:
    // - `/livez` and `/readyz` are probes hit by orchestrators
    //   (k8s, load balancers) every second; emitting an access
    //   log / traceparent on every probe floods the log pipeline
    //   and burns trace ids. `/livez` answers liveness (process
    //   serves HTTP ⇒ 200), `/readyz` answers readiness
    //   (in-process dependencies initialized ⇒ 200, else 503).
    // - `/metrics` is scraped by Prometheus every scrape interval;
    //   tracking the scrape itself would skew the histograms with
    //   self-referential noise (and a per-second scrape on
    //   `/metrics` would dwarf real API traffic).
    //
    // Skipping the AuthLayer, trace-context middleware, and
    // access-log span keeps these endpoints predictable and cheap.
    // The CORS layer is still applied so cross-origin browsers
    // can still call them.
    let public = Router::new()
        .route("/livez", get(routes::health::livez))
        .route("/readyz", get(routes::health::readyz))
        .layer(build_cors_layer(&state.cors_config));

    // Append the `/metrics` endpoint. Built unconditionally — the
    // `metrics` feature gate was removed; the endpoint is always
    // available so a Prometheus scrape can hit a stable URL.
    let public = public.route("/metrics", get(routes::health::metrics));

    let protected = Router::new()
        // --- Infrastructure management (versioned /api/v1/) ---
        .nest("/api/v1", api_routes)
        // Chat interaction surface — already nested under
        // `/api/v1` via the chat routes declared in the
        // `api_routes` block above. We keep a separate
        // `chat_router()` builder for the standalone server
        // crate (`server-bin`) which mounts it directly
        // without the v1 prefix.
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

    // RED metrics middleware: records per-route request count +
    // latency on the Prometheus vectors. Placed as the OUTERMOST
    // layer so every request that reaches a handler contributes a
    // sample — including ones short-circuited by AuthLayer /
    // trace-context (the wrap order means `from_fn` runs last when
    // added last). Always-on now that the `metrics` feature gate
    // has been removed.
    let protected = protected.route_layer(from_fn(track_metrics));
    Router::new()
        .merge(public)
        .merge(protected)
        // Outermost layer: convert any handler panic into a
        // well-formed JSON 500 response with the standard
        // envelope — without this, a panic leaks an opaque
        // "Internal Server Error" page and skips the project's
        // normal error envelope, breaking the front-end
        // ApiClient.toError contract.
        .layer(from_fn(error_handler_middleware))
        // Outermost layer: stamp `Cache-Control: no-store` on
        // `/api/v1/*` responses and `Server-Timing` on all
        // non-streaming responses. Placed *outside* the
        // `RequestTracingLayer` so the header is appended after
        // the tracing span is finalized.
        .layer(from_fn(response_headers_middleware))
        // Catch-all: routes that fall through every registered
        // handler return axum's default empty 404. We map that
        // to the standard envelope so the front-end `ApiError`
        // parser never sees an empty body.
        .fallback(not_found_handler)
        .with_state(state)
}

/// Fallback handler for routes that don't match any registered
/// route. Returns the same `{"code","message"}` envelope as the
/// main adapter so the front-end `ApiClient.toError` parser
/// can extract a uniform human-readable message regardless of
/// which layer rejected the request.
async fn not_found_handler() -> AppError {
    AppError::from(synthia_core::Error::not_found("route"))
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
