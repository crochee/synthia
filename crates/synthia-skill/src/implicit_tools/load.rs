use std::sync::Arc;

use async_trait::async_trait;
use synthia_core::Error;
use synthia_tool::{
    traits::Tool,
    types::{ToolInput, ToolOutput},
};

use crate::registry::SkillRegistry;

// ── LoadSkillTool ─────────────────────────────────────────────────────────

pub struct LoadSkillTool {
    registry: Arc<SkillRegistry>,
}

impl LoadSkillTool {
    pub fn new(registry: Arc<SkillRegistry>) -> Self {
        Self { registry }
    }
}

#[async_trait]
impl Tool for LoadSkillTool {
    fn name(&self) -> &str {
        "load_skill"
    }

    fn description(&self) -> &str {
        "Load a skill's full instructions into context. Use when you need detailed guidance from a skill listed in Available Skills."
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["name"],
            "properties": {
                "name": {
                    "type": "string",
                    "description": "The skill name (must appear in Available Skills list)"
                }
            }
        })
    }

    async fn call(&self, input: ToolInput) -> ToolOutput {
        let name = match input.input.get("name").and_then(|v| v.as_str()) {
            Some(n) => n,
            None => {
                return ToolOutput::error(
                    "Missing required 'name' parameter".to_string(),
                );
            }
        };

        match self.registry.activate_skill(name) {
            Ok(skill) => ToolOutput::text(format!(
                "Loaded skill: {} ({} tokens added to context)",
                name, skill.token_count.level1
            )),
            Err(Error::NotFound(_)) => {
                ToolOutput::error(format!("Skill not found: {}", name))
            }
            Err(Error::InvalidItem(msg)) if msg.contains("disabled") => {
                ToolOutput::error(format!("Skill is disabled: {}", name))
            }
            Err(e) => ToolOutput::error(format!(
                "Failed to load skill '{}': {}",
                name, e
            )),
        }
    }
}
