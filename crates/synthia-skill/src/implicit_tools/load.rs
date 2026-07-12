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

    /// `load_skill` is LLM-callable but hidden from user-facing /help
    /// listings. Skills are activated primarily by the skill system; the
    /// LLM uses this tool to explicitly load a skill's full instructions
    /// when it needs detailed guidance, but the entry would only clutter
    /// the user-facing tool list. P3 (lazy loading): keep the schema out
    /// of the LLM-facing /help output to reduce prompt noise.
    fn is_hidden(&self) -> bool {
        true
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::SkillPaths;

    /// `load_skill` must be hidden from user-facing /help listings per
    /// Phase 1 task 1.1.6 + Phase 2 task 2.1.2. The LLM still needs to
    /// call it to load skills explicitly; only the /help output is
    /// affected.
    #[test]
    fn load_skill_is_hidden_from_user_facing_help() {
        let paths = SkillPaths {
            user_dir: std::env::temp_dir(),
            project_dir: std::env::temp_dir(),
            builtin_dir: std::env::temp_dir(),
        };
        let registry = Arc::new(SkillRegistry::new(paths));
        let tool = LoadSkillTool::new(registry);
        assert!(
            tool.is_hidden(),
            "load_skill must return is_hidden() == true so it does \
             not appear in user-facing /help listings"
        );
        assert!(
            tool.is_user_invocable(),
            "load_skill must remain LLM-invocable even when hidden"
        );
    }
}
