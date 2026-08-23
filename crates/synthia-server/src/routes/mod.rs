pub mod agents;
pub mod chat;
pub mod health;
pub mod helpers;
pub mod memory;
pub mod sessions;
pub mod skills;
pub mod tool;

use std::sync::Arc;

pub use agents::{create_agent, delete_agent, get_agent, list_agents};
use axum::{
    Router,
    routing::{get, patch, post},
};
pub use tool::{get_tool, list_tools, register_tool, unregister_tool};

use crate::state::AppState;

/// REST + SSE surface for chat. Mounted at `/api/v1/chat` by
/// `server/router.rs::create_router`. The same paths are also
/// nested under the v1 management router to preserve the
/// `/api/v1/chat/*` URL space — see
/// `server/router.rs::create_router` for the nest point.
///
/// Session listing and detail live on the management surface
/// (`routes::sessions` mounted at `/api/v1/sessions`) so there
/// is exactly one canonical SessionSummary / SessionDetail
/// shape on the wire.
pub fn chat_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/usage", get(chat::usage))
        .route("/sessions", post(chat::create_session))
        .route("/sessions/{id}/messages", post(chat::send_message))
        .route("/sessions/{id}/messages/stream", get(chat::stream_messages))
        .route("/sessions/{id}/cancel", post(chat::cancel_session))
        .route("/sessions/{id}/regenerate", post(chat::regenerate))
        .route(
            "/sessions/{id}/messages/{message_id}",
            patch(chat::edit_message),
        )
        .route("/messages/{message_id}/feedback", post(chat::feedback))
}
