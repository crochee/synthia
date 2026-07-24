use axum::response::IntoResponse;

pub mod a2a;
pub mod commands;
pub mod health;
pub mod helpers;
pub mod job;
pub mod mcp;
pub mod mcp_servers;
pub mod memory;
pub mod providers;
pub mod settings;
pub mod skills;
pub mod tool;

pub use tool::{delete_tool, get_tool, list_tools, register_tool};

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
