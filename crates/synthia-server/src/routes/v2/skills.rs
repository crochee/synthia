//! The 5 `/api/v2/skills/*` handlers + their
//! request/response types.
//!
//! Skills are stored under
//! `<workspace>/.agents/skills/<name>/SKILL.md`.
//! [`create_skill`] copies a directory into the workspace;
//! [`delete_skill`] removes it.
//! [`super::helpers::copy_dir_all`] is the private recursive
//! copy helper.

use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, State},
};
use serde::{Deserialize, Serialize};
use synthia_core::{ApiResponse, ErrorCode, UserError};

use super::helpers::copy_dir_all;
use crate::state::AppState;

#[derive(Serialize)]
pub struct SkillInfo {
    pub name: String,
    pub description: String,
}

#[derive(Serialize)]
pub struct SkillListResponse {
    pub skills: Vec<SkillInfo>,
    pub count: usize,
}

#[derive(Serialize)]
pub struct SkillDetailResponse {
    pub name: String,
    pub description: String,
    pub path: String,
}

#[derive(Serialize)]
pub struct SkillCreatedResponse {
    pub registered: bool,
    pub name: String,
    pub path: String,
}

#[derive(Serialize)]
pub struct SkillDeletedResponse {
    pub deleted: bool,
    pub name: String,
}

#[derive(Serialize)]
pub struct SkillReloadResponse {
    pub reloaded: bool,
    pub count: usize,
}

#[derive(Deserialize)]
pub struct CreateSkillRequest {
    pub name: String,
    pub path: String,
}

/// GET /api/v2/skills - List all skills.
pub async fn list_skills(
    State(state): State<Arc<AppState>>,
) -> Json<ApiResponse<SkillListResponse>> {
    let skills_dir = state.workspace_root.join(".agents").join("skills");
    let mut skills = Vec::new();

    if skills_dir.exists()
        && let Ok(entries) = std::fs::read_dir(&skills_dir)
    {
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                let path = entry.path();
                let description = if path.is_dir() {
                    let skill_md = path.join("SKILL.md");
                    if skill_md.exists() {
                        if let Ok(content) = std::fs::read_to_string(&skill_md)
                        {
                            content
                                .lines()
                                .take(3)
                                .collect::<Vec<_>>()
                                .join(" ")
                        } else {
                            "Skill directory".to_string()
                        }
                    } else {
                        "Skill directory".to_string()
                    }
                } else {
                    "Skill file".to_string()
                };
                skills.push(SkillInfo {
                    name: name.to_string(),
                    description,
                });
            }
        }
    }

    Json(ApiResponse::ok(SkillListResponse {
        count: skills.len(),
        skills,
    }))
}

/// GET /api/v2/skills/:id - Get a single skill.
pub async fn get_skill(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Json<ApiResponse<SkillDetailResponse>> {
    let skill_path = state
        .workspace_root
        .join(".agents")
        .join("skills")
        .join(&name)
        .join("SKILL.md");

    if skill_path.exists() {
        match std::fs::read_to_string(&skill_path) {
            Ok(content) => Json(ApiResponse::ok(SkillDetailResponse {
                name: name.clone(),
                description: content
                    .lines()
                    .take(5)
                    .collect::<Vec<_>>()
                    .join(" "),
                path: skill_path.to_string_lossy().to_string(),
            })),
            Err(_) => Json(ApiResponse::err(UserError::new(
                ErrorCode::InternalServerError,
                "Failed to read skill file",
            ))),
        }
    } else {
        Json(ApiResponse::err(UserError::new(
            ErrorCode::NotFound,
            format!("Skill '{}' not found", name),
        )))
    }
}

/// POST /api/v2/skills - Register a new skill.
pub async fn create_skill(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateSkillRequest>,
) -> Json<ApiResponse<SkillCreatedResponse>> {
    let skill_path = std::path::PathBuf::from(&req.path);
    if !skill_path.exists() {
        return Json(ApiResponse::err(UserError::new(
            ErrorCode::BadRequest,
            format!("Path '{}' does not exist", req.path),
        )));
    }

    // Copy or symlink the skill into the workspace skills directory
    let target_dir = state
        .workspace_root
        .join(".agents")
        .join("skills")
        .join(&req.name);

    if target_dir.exists() {
        return Json(ApiResponse::err(UserError::new(
            ErrorCode::Conflict,
            format!("Skill '{}' already exists", req.name),
        )));
    }

    if let Err(e) = copy_dir_all(&skill_path, &target_dir) {
        return Json(ApiResponse::err(UserError::new(
            ErrorCode::InternalServerError,
            format!("Failed to install skill: {}", e),
        )));
    }

    Json(ApiResponse::ok(SkillCreatedResponse {
        registered: true,
        name: req.name,
        path: req.path,
    }))
}

/// DELETE /api/v2/skills/:id - Remove a skill.
pub async fn delete_skill(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Json<ApiResponse<SkillDeletedResponse>> {
    let skill_path = state
        .workspace_root
        .join(".agents")
        .join("skills")
        .join(&name);

    if skill_path.exists() {
        if std::fs::remove_dir_all(&skill_path).is_ok() {
            Json(ApiResponse::ok(SkillDeletedResponse {
                deleted: true,
                name,
            }))
        } else {
            Json(ApiResponse::err(UserError::new(
                ErrorCode::InternalServerError,
                "Failed to remove skill directory",
            )))
        }
    } else {
        Json(ApiResponse::err(UserError::new(
            ErrorCode::NotFound,
            format!("Skill '{}' not found", name),
        )))
    }
}

/// POST /api/v2/skills/reload - Reload skills from disk.
pub async fn reload_skills(
    State(state): State<Arc<AppState>>,
) -> Json<ApiResponse<SkillReloadResponse>> {
    let skills_dir = state.workspace_root.join(".agents").join("skills");
    let count = if skills_dir.exists() {
        std::fs::read_dir(&skills_dir)
            .map(|entries| {
                entries
                    .filter(|e| {
                        e.as_ref()
                            .map(|e| {
                                e.file_type()
                                    .map(|t| t.is_dir())
                                    .unwrap_or(false)
                            })
                            .unwrap_or(false)
                    })
                    .count()
            })
            .unwrap_or(0)
    } else {
        0
    };

    Json(ApiResponse::ok(SkillReloadResponse {
        reloaded: true,
        count,
    }))
}
