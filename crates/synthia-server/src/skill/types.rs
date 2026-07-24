//! Skill types for API requests and responses

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct AddSkillRequest {
    pub name: String,
    pub path: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillInfo {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillLoadResult {
    pub name: String,
    pub status: String,
    pub content: Option<String>,
}
