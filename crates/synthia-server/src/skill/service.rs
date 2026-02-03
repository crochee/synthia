//! Skill service for skill management logic

use std::sync::Arc;

use synthia_agent::tools::Tool;

use super::types::{SkillInfo, SkillLoadResult};
use crate::{
    AppState,
    error::ServerError,
    utils::{extract_skills_from_description, extract_text_content},
};

pub struct SkillService {
    skill_tool: Arc<dyn Tool>,
    config: Arc<tokio::sync::RwLock<crate::config::ServerConfig>>,
}

impl SkillService {
    pub fn new(
        skill_tool: Arc<dyn Tool>,
        config: Arc<tokio::sync::RwLock<crate::config::ServerConfig>>,
    ) -> Self {
        Self { skill_tool, config }
    }

    pub fn from_state(state: &AppState) -> Self {
        Self::new(
            state.agent.deps.skills.clone() as Arc<dyn Tool>,
            state.config.clone(),
        )
    }

    pub fn list(&self) -> Vec<SkillInfo> {
        let desc = self.skill_tool.description();
        extract_skills_from_description(desc)
    }

    pub fn get(&self, name: &str) -> Option<SkillInfo> {
        self.list().into_iter().find(|s| s.name == name)
    }

    pub async fn load(
        &self,
        name: &str,
    ) -> Result<SkillLoadResult, ServerError> {
        let params = serde_json::json!({ "name": name });
        let result = self.skill_tool.call(params).await;

        if result.is_error == Some(true) {
            Err(ServerError::BadRequest(extract_text_content(
                &result.content,
            )))
        } else {
            Ok(SkillLoadResult {
                name: name.to_string(),
                status: "loaded".to_string(),
                content: Some(extract_text_content(&result.content)),
            })
        }
    }

    pub async fn add(
        &self,
        req: super::types::AddSkillRequest,
    ) -> Result<SkillInfo, ServerError> {
        if req.name.is_empty() {
            return Err(ServerError::missing_field("name"));
        }
        if req.path.is_empty() {
            return Err(ServerError::missing_field("path"));
        }

        let mut config = self.config.write().await;

        if config.skills.iter().any(|s| s.name == req.name) {
            return Err(ServerError::already_exists("Skill", &req.name));
        }

        let skill_config = crate::config::SkillConfig {
            name: req.name.clone(),
            path: req.path.clone(),
        };

        config.skills.push(skill_config);

        Ok(SkillInfo {
            name: req.name,
            description: req.description.unwrap_or_default(),
        })
    }

    pub async fn delete(&self, name: &str) -> Result<bool, ServerError> {
        let mut config = self.config.write().await;

        let original_len = config.skills.len();
        config.skills.retain(|s| s.name != name);

        if config.skills.len() == original_len {
            return Err(ServerError::not_found("Skill", name));
        }

        Ok(true)
    }
}
