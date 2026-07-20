use std::sync::Arc;

use axum::{Json, extract::State};
use synthia_core::ApiResponse;

use crate::state::AppState;

pub async fn health_check() -> Json<ApiResponse<serde_json::Value>> {
    Json(ApiResponse::ok(serde_json::json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION")
    })))
}

pub async fn list_models(
    State(state): State<Arc<AppState>>,
) -> Json<ApiResponse<serde_json::Value>> {
    let config = &state.workspace_config;
    let mut models = Vec::new();
    for (name, entry) in &config.providers {
        models.push(serde_json::json!({
            "provider": name,
            "model": entry.default_model.clone().unwrap_or_else(|| "unknown".to_string()),
            "context_window": entry.context_window.unwrap_or(128_000),
            "supports_tools": entry.supports_tools.unwrap_or(true),
            "supports_streaming": entry.supports_streaming.unwrap_or(true),
        }));
    }
    Json(ApiResponse::ok(serde_json::json!({
        "models": models,
        "default_provider": config.default_provider,
        "default_model": config.default_model,
    })))
}
