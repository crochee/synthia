use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, State},
};
use synthia_core::{ApiResponse, ErrorCode, UserError};

use crate::state::AppState;

/// GET /api/skills - List available skills.
pub async fn list_skills(
    State(state): State<Arc<AppState>>,
) -> Json<ApiResponse<serde_json::Value>> {
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
                skills.push(serde_json::json!({
                    "name": name,
                    "description": description,
                }));
            }
        }
    }

    Json(ApiResponse::ok(
        serde_json::json!({ "skills": skills, "count": skills.len() }),
    ))
}

/// POST /api/skills - Register a skill by path.
pub async fn register_skill(
    State(_state): State<Arc<AppState>>,
    Json(req): Json<SkillRegisterRequest>,
) -> Json<ApiResponse<serde_json::Value>> {
    let skill_path = std::path::PathBuf::from(&req.path);
    if !skill_path.exists() {
        return Json(ApiResponse::err(UserError::new(
            ErrorCode::BadRequest,
            format!("Path '{}' not found", req.path),
        )));
    }
    Json(ApiResponse::ok(serde_json::json!({
        "registered": true,
        "name": req.name,
        "path": req.path,
    })))
}

/// GET /api/skills/{name} - Get a single skill.
pub async fn get_skill(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Json<ApiResponse<serde_json::Value>> {
    let skill_path = state
        .workspace_root
        .join(".agents")
        .join("skills")
        .join(&name)
        .join("SKILL.md");
    if skill_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&skill_path) {
            Json(ApiResponse::ok(serde_json::json!({
                "name": name,
                "description": content.lines().take(5).collect::<Vec<_>>().join(" "),
                "path": skill_path.to_string_lossy(),
            })))
        } else {
            Json(ApiResponse::err(UserError::new(
                ErrorCode::InternalServerError,
                "Failed to read skill file",
            )))
        }
    } else {
        Json(ApiResponse::err(UserError::new(
            ErrorCode::NotFound,
            format!("Skill '{}' not found", name),
        )))
    }
}

/// DELETE /api/skills/{name} - Unregister a skill.
pub async fn delete_skill(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Json<ApiResponse<serde_json::Value>> {
    let skill_path = state
        .workspace_root
        .join(".agents")
        .join("skills")
        .join(&name);
    if skill_path.exists() {
        if std::fs::remove_dir_all(&skill_path).is_ok() {
            Json(ApiResponse::ok(
                serde_json::json!({ "unregistered": true, "name": name }),
            ))
        } else {
            Json(ApiResponse::err(UserError::new(
                ErrorCode::InternalServerError,
                "Failed to remove skill",
            )))
        }
    } else {
        Json(ApiResponse::err(UserError::new(
            ErrorCode::NotFound,
            format!("Skill '{}' not found", name),
        )))
    }
}

#[derive(serde::Deserialize)]
pub struct SkillRegisterRequest {
    pub name: String,
    pub path: String,
}
