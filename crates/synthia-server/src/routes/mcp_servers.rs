use std::sync::Arc;

use axum::{Json, extract::State};
use synthia_core::{ApiResponse, ErrorCode, UserError};

use crate::state::AppState;

/// GET /api/mcp/servers - List MCP server configurations.
pub async fn list_mcp_servers(
    State(state): State<Arc<AppState>>,
) -> Json<ApiResponse<serde_json::Value>> {
    let configs = state.mcp_registry.list_configs().await;
    let servers: Vec<_> = configs
        .iter()
        .map(|c| {
            serde_json::json!({
                "name": c.name,
                "command": c.command,
                "args": c.args,
            })
        })
        .collect();
    Json(ApiResponse::ok(serde_json::json!({
        "servers": servers,
        "count": servers.len(),
    })))
}

/// POST /api/mcp/servers - Register a new MCP server configuration.
pub async fn register_mcp_server(
    State(state): State<Arc<AppState>>,
    Json(req): Json<McpServerRegisterRequest>,
) -> Json<ApiResponse<serde_json::Value>> {
    let config = synthia_mcp::McpServerConfig {
        name: req.name.clone(),
        command: req.command,
        args: req.args,
        env: std::collections::HashMap::new(),
    };
    state.mcp_registry.add_config(config).await;

    let tool_registry = state.tool_registry.read().await;
    let _ = state
        .mcp_registry
        .register_tools_for_server(
            &req.name,
            &tool_registry,
            &state.tool_resolver,
        )
        .await;
    drop(tool_registry);

    Json(ApiResponse::ok(serde_json::json!({
        "registered": true,
        "name": req.name
    })))
}

#[derive(serde::Deserialize)]
pub struct McpServerRegisterRequest {
    name: String,
    command: String,
    args: Vec<String>,
}

/// POST /api/mcp/servers/{id}/discover - Discover and register tools from a server.
pub async fn discover_mcp_tools(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Json<ApiResponse<serde_json::Value>> {
    let tool_registry = state.tool_registry.read().await;
    match state
        .mcp_registry
        .register_tools_for_server(&id, &tool_registry, &state.tool_resolver)
        .await
    {
        Ok(names) => Json(ApiResponse::ok(serde_json::json!({
            "server": id,
            "registered": names.len(),
            "tools": names,
        }))),
        Err(e) => Json(ApiResponse::err(UserError::new(
            ErrorCode::InternalServerError,
            format!("Failed to discover/register MCP tools: {}", e),
        ))),
    }
}

/// GET /api/mcp/servers/{id} - Get a single MCP server.
pub async fn get_mcp_server(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Json<ApiResponse<serde_json::Value>> {
    match state.mcp_registry.get_config(&id).await {
        Some(config) => Json(ApiResponse::ok(serde_json::json!({
            "id": id,
            "name": config.name,
            "command": config.command,
            "args": config.args,
        }))),
        None => Json(ApiResponse::err(UserError::new(
            ErrorCode::NotFound,
            format!("MCP server '{}' not found", id),
        ))),
    }
}

/// DELETE /api/mcp/servers/{id} - Stop and remove an MCP server.
pub async fn delete_mcp_server(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Json<ApiResponse<serde_json::Value>> {
    state.mcp_registry.remove_config(&id).await;
    Json(ApiResponse::ok(
        serde_json::json!({ "unregistered": true, "id": id }),
    ))
}
