//! MCP HTTP handlers
//!
//! Handlers for MCP server management using McpService.

mod server;
mod service;
mod types;

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
pub use service::McpService;
pub use types::{McpServerConfig, McpServerRequest, McpServerStatus};

use crate::{AppState, error::ServerError};

pub async fn list_mcp_servers(
    State(state): State<AppState>,
) -> Result<Json<Vec<McpServerStatus>>, ServerError> {
    let service = &state.mcp_module;
    let servers = service.list();
    Ok(Json(servers))
}

pub async fn register_mcp_server(
    State(state): State<AppState>,
    Json(req): Json<McpServerRequest>,
) -> Result<Json<McpServerStatus>, ServerError> {
    let config: McpServerConfig = req.into();

    let service = &state.mcp_module;
    let status = service.register(config).await?;

    Ok(Json(status))
}

pub async fn unregister_mcp_server(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<StatusCode, ServerError> {
    let service = &state.mcp_module;
    service.unregister(&name).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn list_mcp_tools(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<Vec<serde_json::Value>>, ServerError> {
    let service = &state.mcp_module;
    let tools = service.list_tools(&name).await?;

    let tool_infos: Vec<serde_json::Value> = tools
        .into_iter()
        .map(|t| {
            serde_json::json!({
                "name": t.name,
                "description": t.description,
                "inputSchema": t.input_schema,
            })
        })
        .collect();

    Ok(Json(tool_infos))
}

pub async fn get_mcp_server(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<McpServerStatus>, ServerError> {
    let service = &state.mcp_module;
    match service.get(&name) {
        Some(status) => Ok(Json(status)),
        None => Err(ServerError::not_found("MCP server", &name)),
    }
}

pub async fn start_mcp_server(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<StatusCode, ServerError> {
    let service = &state.mcp_module;
    service.start(&name).await?;
    Ok(StatusCode::OK)
}

pub async fn stop_mcp_server(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<StatusCode, ServerError> {
    let service = &state.mcp_module;
    service.stop(&name).await?;
    Ok(StatusCode::OK)
}
