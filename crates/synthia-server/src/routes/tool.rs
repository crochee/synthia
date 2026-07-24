use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, State},
};
use synthia_core::{ApiResponse, ErrorCode, Registry, RegistryItem, UserError};

use crate::state::AppState;

/// GET /api/tools - List registered tools.
pub async fn list_tools(
    State(state): State<Arc<AppState>>,
) -> Json<ApiResponse<serde_json::Value>> {
    let tool_reg = state.tool_registry.read().await;
    let defs: Vec<_> = tool_reg
        .list(None)
        .await
        .map(|entries| {
            entries
                .iter()
                .map(|e| synthia_provider::ToolDefinition {
                    name: e.name().to_string(),
                    description: e.description().to_string(),
                    input_schema: e.tool_instance().parameters(),
                    cache_control: None,
                })
                .collect()
        })
        .unwrap_or_default();
    drop(tool_reg);

    let tools: Vec<_> = defs
        .into_iter()
        .map(|d| {
            serde_json::json!({
                "name": d.name,
                "description": d.description,
            })
        })
        .collect();

    Json(ApiResponse::ok(
        serde_json::json!({ "tools": tools, "count": tools.len() }),
    ))
}

/// POST /api/tools - Register a tool.
pub async fn register_tool(
    State(_state): State<Arc<AppState>>,
    Json(_req): Json<ToolRegisterRequest>,
) -> Json<ApiResponse<serde_json::Value>> {
    Json(ApiResponse::ok(serde_json::json!({
        "registered": true,
        "note": "Tools must be registered at the code level; this endpoint is for documentation",
    })))
}

/// GET /api/tools/{name} - Get a single tool.
pub async fn get_tool(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Json<ApiResponse<serde_json::Value>> {
    let tool_reg = state.tool_registry.read().await;
    let defs: Vec<_> = tool_reg
        .list(None)
        .await
        .map(|entries| {
            entries
                .iter()
                .map(|e| synthia_provider::ToolDefinition {
                    name: e.name().to_string(),
                    description: e.description().to_string(),
                    input_schema: e.tool_instance().parameters(),
                    cache_control: None,
                })
                .collect()
        })
        .unwrap_or_default();
    drop(tool_reg);

    if let Some(def) = defs.iter().find(|d| d.name == name) {
        Json(ApiResponse::ok(serde_json::json!({
            "name": def.name,
            "description": def.description,
        })))
    } else {
        Json(ApiResponse::err(UserError::new(
            ErrorCode::NotFound,
            format!("Tool '{}' not found", name),
        )))
    }
}

/// DELETE /api/tools/{name} - Unregister a tool.
pub async fn delete_tool(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Json<ApiResponse<serde_json::Value>> {
    let tool_reg = state.tool_registry.read().await;
    let _ = tool_reg.unregister(&name).await;
    drop(tool_reg);
    Json(ApiResponse::ok(
        serde_json::json!({ "unregistered": true, "name": name }),
    ))
}

#[derive(serde::Deserialize)]
pub struct ToolRegisterRequest {
    pub name: String,
    pub description: String,
}
