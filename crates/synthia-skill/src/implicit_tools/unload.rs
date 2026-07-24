use std::sync::Arc;

use async_trait::async_trait;
use synthia_tool::{
    traits::Tool,
    types::{ToolInput, ToolOutput},
};

use crate::registry::SkillRegistry;

// ── UnloadSkillTool ───────────────────────────────────────────────────────

pub struct UnloadSkillTool {
    registry: Arc<SkillRegistry>,
}

impl UnloadSkillTool {
    pub fn new(registry: Arc<SkillRegistry>) -> Self {
        Self { registry }
    }
}

#[async_trait]
impl Tool for UnloadSkillTool {
    fn name(&self) -> &str {
        "unload_skill"
    }

    fn description(&self) -> &str {
        "Remove a previously loaded skill from context to free up tokens."
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["name"],
            "properties": {
                "name": {
                    "type": "string",
                    "description": "The skill name to unload"
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

        let skill_exists = self.registry.get_skill_sync(name).is_ok();
        if !skill_exists {
            return ToolOutput::error(format!("Skill not found: {}", name));
        }

        let is_active = self.registry.is_active(name);
        if !is_active {
            return ToolOutput::text(format!(
                "Skill '{}' is not currently loaded (no-op)",
                name
            ));
        }

        match self.registry.deactivate_skill(name) {
            Ok(_) => {
                let tokens = self
                    .registry
                    .get_skill_sync(name)
                    .ok()
                    .map(|s| s.token_count.level1)
                    .unwrap_or(0);
                ToolOutput::text(format!(
                    "Unloaded skill: {} ({} tokens freed)",
                    name, tokens
                ))
            }
            Err(e) => ToolOutput::error(format!(
                "Failed to unload skill '{}': {}",
                name, e
            )),
        }
    }
}
