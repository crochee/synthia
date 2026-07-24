use axum::response::IntoResponse;

pub mod chat;
pub mod job;
pub mod mcp;
pub mod session;
pub mod skill;
pub mod tool;
pub mod v2;
pub mod ws;

pub use chat::chat_handler;
pub use session::{
    create_session,
    delete_session,
    get_session,
    get_session_messages,
    get_session_tools,
    list_sessions,
    send_message,
};
pub use skill::{delete_skill, get_skill, list_skills, register_skill};
pub use tool::{delete_tool, get_tool, list_tools, register_tool};
pub use v2::{
    create_provider,
    create_skill,
    delete_provider,
    delete_session_v2,
    delete_skill as delete_skill_v2,
    get_provider,
    get_session_detail,
    get_skill as get_skill_v2,
    list_providers as list_providers_v2,
    list_skills as list_skills_v2,
    reload_skills,
    search_memory,
};
pub use ws::stream_handler;

pub use crate::sse_stream::stream_sse_handler;

pub async fn ok_response() -> impl axum::response::IntoResponse {
    axum::Json(serde_json::json!({ "status": "ok" }))
}

pub fn not_found<T>(
    resource: &str,
    name: &str,
) -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({
        "error": "not_found",
        "message": format!("{} '{}' not found", resource, name)
    }))
}

pub fn error_response(status: u16, message: &str) -> axum::response::Response {
    (
        axum::http::StatusCode::from_u16(status)
            .unwrap_or(axum::http::StatusCode::INTERNAL_SERVER_ERROR),
        axum::Json(serde_json::json!({ "error": message })),
    )
        .into_response()
}
