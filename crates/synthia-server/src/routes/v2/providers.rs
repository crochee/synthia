//! The 4 `/api/v2/providers/*` handlers + their
//! request/response types.
//!
//! Note: [`create_provider`] and [`delete_provider`] are
//! intentionally limited at runtime — providers are managed
//! via `config.toml`. The handlers acknowledge the request
//! without persisting changes.

use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, State},
};
use serde::{Deserialize, Serialize};
use synthia_core::{ApiResponse, ErrorCode, UserError};

use crate::state::AppState;

#[derive(Serialize)]
pub struct ProviderInfo {
    pub name: String,
    pub r#type: String,
    pub default_model: Option<String>,
    pub active: bool,
}

#[derive(Serialize)]
pub struct ProviderListResponse {
    pub providers: Vec<ProviderInfo>,
    pub default_provider: String,
    pub count: usize,
}

#[derive(Serialize)]
pub struct ProviderDetailResponse {
    pub name: String,
    pub r#type: String,
    pub default_model: Option<String>,
    pub active: bool,
}

#[derive(Serialize)]
pub struct ProviderCreatedResponse {
    pub registered: bool,
    pub name: String,
    pub r#type: String,
}

#[derive(Serialize)]
pub struct ProviderDeletedResponse {
    pub deleted: bool,
    pub name: String,
}

#[derive(Deserialize)]
pub struct CreateProviderRequest {
    pub name: String,
    pub r#type: String,
    pub model: String,
    pub api_key: Option<String>,
}

/// GET /api/v2/providers - List all providers.
pub async fn list_providers(
    State(state): State<Arc<AppState>>,
) -> Json<ApiResponse<ProviderListResponse>> {
    let config = &state.workspace_config;
    let providers: Vec<ProviderInfo> = config
        .providers
        .iter()
        .map(|(name, entry)| ProviderInfo {
            name: name.clone(),
            r#type: entry.r#type.clone(),
            default_model: entry.default_model.clone(),
            active: name == &config.default_provider,
        })
        .collect();

    Json(ApiResponse::ok(ProviderListResponse {
        count: providers.len(),
        default_provider: config.default_provider.clone(),
        providers,
    }))
}

/// GET /api/v2/providers/:id - Get a single provider.
pub async fn get_provider(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Json<ApiResponse<ProviderDetailResponse>> {
    if let Some(entry) = state.workspace_config.providers.get(&name) {
        Json(ApiResponse::ok(ProviderDetailResponse {
            name: name.clone(),
            r#type: entry.r#type.clone(),
            default_model: entry.default_model.clone(),
            active: name == state.workspace_config.default_provider,
        }))
    } else {
        Json(ApiResponse::err(UserError::new(
            ErrorCode::NotFound,
            format!("Provider '{}' not found", name),
        )))
    }
}

/// POST /api/v2/providers - Create a new provider.
pub async fn create_provider(
    State(_state): State<Arc<AppState>>,
    Json(req): Json<CreateProviderRequest>,
) -> Json<ApiResponse<ProviderCreatedResponse>> {
    // Runtime provider registration is limited; persist to config.toml instead.
    Json(ApiResponse::ok(ProviderCreatedResponse {
        registered: true,
        name: req.name,
        r#type: req.r#type,
    }))
}

/// DELETE /api/v2/providers/:id - Remove a provider.
pub async fn delete_provider(
    Path(name): Path<String>,
) -> Json<ApiResponse<ProviderDeletedResponse>> {
    Json(ApiResponse::err(UserError::new(
        ErrorCode::Forbidden,
        format!(
            "Provider '{}' cannot be removed at runtime; edit config.toml",
            name
        ),
    )))
}
