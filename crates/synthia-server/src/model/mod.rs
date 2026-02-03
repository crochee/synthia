//! Model HTTP handlers
//!
//! Handlers for model management using ModelService.

mod service;
mod types;

use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
pub use service::ModelService;
pub use types::{
    AddModelProviderRequest,
    ModelInfo,
    ProviderInfo,
    UpdateModelRequest,
};

use crate::{AppState, error::ServerError};

pub async fn list_models(
    State(state): State<AppState>,
) -> Result<Json<Vec<ProviderInfo>>, ServerError> {
    let service = ModelService::new(Arc::new(state));
    let providers = service.list_providers().await;
    Ok(Json(providers))
}

pub async fn get_model(
    State(state): State<AppState>,
    Path((provider_name, model_name)): Path<(String, String)>,
) -> Result<Json<ModelInfo>, ServerError> {
    let service = ModelService::new(Arc::new(state));
    let model = service.get_model(&provider_name, &model_name).await?;
    Ok(Json(model))
}

pub async fn update_model(
    State(state): State<AppState>,
    Path((provider_name, model_name)): Path<(String, String)>,
    Json(req): Json<UpdateModelRequest>,
) -> Result<Json<ModelInfo>, ServerError> {
    let service = ModelService::new(Arc::new(state));
    let model = service
        .update_model(&provider_name, &model_name, req)
        .await?;
    Ok(Json(model))
}

pub async fn add_model_provider(
    State(state): State<AppState>,
    Json(req): Json<AddModelProviderRequest>,
) -> Result<Json<ProviderInfo>, ServerError> {
    let service = ModelService::new(Arc::new(state));
    let provider = service.add_provider(req).await?;
    Ok(Json(provider))
}

pub async fn delete_model(
    State(state): State<AppState>,
    Path(provider): Path<String>,
) -> Result<StatusCode, ServerError> {
    let service = ModelService::new(Arc::new(state));
    service.delete_provider(&provider).await?;
    Ok(StatusCode::NO_CONTENT)
}
