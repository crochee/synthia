//! Tool HTTP handlers
//!
//! Tool handlers using Service layer.

mod service;
mod types;

use std::collections::HashMap;

use axum::{
    Json,
    extract::{Path, State},
};
use serde::Serialize;
pub use service::ToolService;
pub use types::{
    ToolAnnotations,
    ToolExecuteRequest,
    ToolExecuteResponse,
    ToolInfo,
};

use crate::{AppState, error::ServerError};

#[derive(Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub mcps: HashMap<String, bool>,
}

pub async fn health(State(state): State<AppState>) -> Json<HealthResponse> {
    let mcps = state.mcp_module.health_check_all().await;
    let status = if mcps.values().all(|v| *v) {
        "ok".to_string()
    } else {
        "degraded".to_string()
    };
    Json(HealthResponse { status, mcps })
}

pub async fn list_tools(
    State(state): State<AppState>,
) -> Result<Json<Vec<ToolInfo>>, ServerError> {
    let service = ToolService::new(state.tool_registry.clone());
    let tools = service.list();
    Ok(Json(tools))
}

pub async fn get_tool(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<ToolInfo>, ServerError> {
    let service = ToolService::new(state.tool_registry.clone());
    let tool_info = service
        .get(&name)
        .ok_or_else(|| ServerError::not_found("Tool", &name))?;
    Ok(Json(tool_info))
}

pub async fn execute_tool(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(req): Json<ToolExecuteRequest>,
) -> Result<Json<ToolExecuteResponse>, ServerError> {
    let service = ToolService::new(state.tool_registry.clone());
    let result = service.execute(&name, req.arguments).await?;

    Ok(Json(ToolExecuteResponse {
        success: true,
        result,
    }))
}
