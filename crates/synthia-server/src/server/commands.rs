use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, State},
};
use synthia_core::{ApiResponse, ErrorCode, Registry, UserError};

use crate::state::AppState;

/// GET /api/commands - List available commands.
pub(super) async fn list_commands(
    State(state): State<Arc<AppState>>,
) -> Json<ApiResponse<serde_json::Value>> {
    let cmd_reg = state.command_registry.read().await;
    let commands: Vec<String> = cmd_reg
        .list(None)
        .await
        .unwrap()
        .into_iter()
        .map(|d| d.name)
        .collect();
    drop(cmd_reg);

    Json(ApiResponse::ok(serde_json::json!({
        "commands": commands,
        "count": commands.len()
    })))
}

/// GET /api/commands/{name} - Get a single command.
pub(super) async fn get_command(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> Json<ApiResponse<serde_json::Value>> {
    let cmd_reg = state.command_registry.read().await;
    let found = cmd_reg.get(&name).await.unwrap().is_some();
    drop(cmd_reg);

    if found {
        Json(ApiResponse::ok(
            serde_json::json!({ "name": name, "registered": true }),
        ))
    } else {
        Json(ApiResponse::err(UserError::new(
            ErrorCode::NotFound,
            format!("Command '{}' not found", name),
        )))
    }
}

/// DELETE /api/commands/{name} - Unregister a command.
pub(super) async fn delete_command(
    Path(name): Path<String>,
) -> Json<ApiResponse<serde_json::Value>> {
    Json(ApiResponse::err(UserError::new(
        ErrorCode::Forbidden,
        format!("Command '{}' cannot be removed at runtime", name),
    )))
}
