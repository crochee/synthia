//! API routes definition
//!
//! Defines all HTTP routes for the Synthia server.

use axum::{
    Router,
    middleware,
    routing::{delete, get, post, put},
};
use tower_http::cors::{Any, CorsLayer};

use crate::{AppState, auth, chat, mcp, model, session, skill, tool, ws};

pub fn build_routes(state: AppState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        .route("/health", get(tool::health))
        .route("/tools", get(tool::list_tools))
        .route("/tools/{name}", get(tool::get_tool))
        .route("/tools/{name}/execute", post(tool::execute_tool))
        .route("/chat", post(chat::chat))
        .route("/chat/stream", post(chat::chat_stream))
        .route("/sessions", get(session::list_sessions))
        .route("/sessions", post(session::create_session))
        .route("/sessions/{id}", get(session::get_session))
        .route("/sessions/{id}", delete(session::delete_session))
        .route("/sessions/{id}/compact", post(session::compact_session))
        .route(
            "/sessions/{id}/messages",
            get(session::get_session_messages),
        )
        .route("/skills", get(skill::list_skills))
        .route("/skills", post(skill::add_skill))
        .route("/skills/{name}", get(skill::get_skill))
        .route("/skills/{name}", delete(skill::delete_skill))
        .route("/skills/{name}/load", post(skill::load_skill))
        .route("/mcp/servers", get(mcp::list_mcp_servers))
        .route("/mcp/servers", post(mcp::register_mcp_server))
        .route("/mcp/servers/{name}", get(mcp::get_mcp_server))
        .route("/mcp/servers/{name}", delete(mcp::unregister_mcp_server))
        .route("/mcp/servers/{name}/tools", get(mcp::list_mcp_tools))
        .route("/mcp/servers/{name}/start", post(mcp::start_mcp_server))
        .route("/mcp/servers/{name}/stop", post(mcp::stop_mcp_server))
        .route("/models", get(model::list_models))
        .route("/models", post(model::add_model_provider))
        .route("/models/{provider}", delete(model::delete_model))
        .route("/models/{provider}/{name}", get(model::get_model))
        .route("/models/{provider}/{name}", put(model::update_model))
        .route("/ws/{session_id}", get(ws::websocket))
        .with_state(state.clone())
        .layer(middleware::from_fn_with_state(state, auth::auth_middleware))
        .layer(cors)
}
