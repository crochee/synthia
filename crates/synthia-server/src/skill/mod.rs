//! Skill HTTP handlers
//!
//! Handlers for skill management using SkillService.

mod service;
mod types;

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
pub use service::SkillService;
pub use types::{AddSkillRequest, SkillInfo, SkillLoadResult};

use crate::{AppState, error::ServerError};

pub async fn list_skills(
    State(state): State<AppState>,
) -> Result<Json<Vec<SkillInfo>>, ServerError> {
    let service = SkillService::from_state(&state);
    let skills = service.list();
    Ok(Json(skills))
}

pub async fn get_skill(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<SkillInfo>, ServerError> {
    let service = SkillService::from_state(&state);
    match service.get(&name) {
        Some(info) => Ok(Json(info)),
        None => Err(ServerError::not_found("Skill", &name)),
    }
}

pub async fn load_skill(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<SkillLoadResult>, ServerError> {
    let service = SkillService::from_state(&state);
    let result = service.load(&name).await?;
    Ok(Json(result))
}

pub async fn add_skill(
    State(state): State<AppState>,
    Json(req): Json<AddSkillRequest>,
) -> Result<Json<SkillInfo>, ServerError> {
    let service = SkillService::from_state(&state);
    let info = service.add(req).await?;
    Ok(Json(info))
}

pub async fn delete_skill(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<StatusCode, ServerError> {
    let service = SkillService::from_state(&state);
    service.delete(&name).await?;
    Ok(StatusCode::NO_CONTENT)
}
