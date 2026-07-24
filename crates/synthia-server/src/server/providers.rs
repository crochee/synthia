use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, State},
};
use synthia_core::{ApiResponse, ErrorCode, UserError};

use crate::state::AppState;

/// GET /api/providers - List available model providers.
pub(super) async fn list_providers(
    State(state): State<Arc<AppState>>,
) -> Json<ApiResponse<serde_json::Value>> {
    let config = &state.workspace_config;
    let providers: Vec<_> = config
        .providers
        .iter()
        .map(|(name, entry)| {
            serde_json::json!({
                "name": name,
                "type": entry.r#type,
                "default_model": entry.default_model.clone().unwrap_or_else(|| "unknown".to_string()),
                "active": name == &config.default_provider,
            })
        })
        .collect();

    Json(ApiResponse::ok(serde_json::json!({
        "providers": providers,
        "default_provider": config.default_provider,
        "count": providers.len(),
    })))
}

/// POST /api/providers - Register a new model provider.
pub(super) async fn register_provider(
    State(_state): State<Arc<AppState>>,
    Json(req): Json<ProviderRegisterRequest>,
) -> Json<ApiResponse<serde_json::Value>> {
    Json(ApiResponse::ok(serde_json::json!({
        "registered": true,
        "name": req.name,
        "model": req.model,
    })))
}

/// GET /api/providers/{name} - Get a single provider by name.
pub(super) async fn get_provider(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> Json<ApiResponse<serde_json::Value>> {
    if let Some(entry) = state.workspace_config.providers.get(&name) {
        Json(ApiResponse::ok(serde_json::json!({
            "name": name,
            "type": entry.r#type,
            "default_model": entry.default_model,
            "active": name == state.workspace_config.default_provider,
        })))
    } else {
        Json(ApiResponse::err(UserError::new(
            ErrorCode::NotFound,
            format!("Provider '{}' not found", name),
        )))
    }
}

/// DELETE /api/providers/{name} - Unregister a provider.
pub(super) async fn delete_provider(
    Path(name): Path<String>,
) -> Json<ApiResponse<serde_json::Value>> {
    Json(ApiResponse::err(UserError::new(
        ErrorCode::Forbidden,
        format!(
            "Provider '{}' cannot be removed at runtime; edit config.toml",
            name
        ),
    )))
}

#[derive(serde::Deserialize)]
pub(super) struct ProviderRegisterRequest {
    name: String,
    _type: String,
    model: String,
    _api_key: Option<String>,
}
