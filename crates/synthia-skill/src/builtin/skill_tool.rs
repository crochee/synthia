use async_trait::async_trait;
use serde_json::json;
use synthia_tool::{
    traits::Tool,
    types::{ToolInput, ToolOutput},
};

pub struct SkillTool;

impl Default for SkillTool {
    fn default() -> Self {
        Self::new()
    }
}

impl SkillTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for SkillTool {
    fn name(&self) -> &str {
        "Skill"
    }

    fn description(&self) -> &str {
        "Executes a skill within the main conversation"
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "name": { "type": "string", "description": "The skill name (no arguments). E.g., \"pdf\" or \"xlsx\"" }
            },
            "required": ["name"]
        })
    }

    fn requires_permission(&self) -> bool {
        true
    }

    async fn call(&self, input: ToolInput) -> ToolOutput {
        let name = match input.input.get("name").and_then(|v| v.as_str()) {
            Some(n) => n,
            None => return ToolOutput::error("name is required".to_string()),
        };
        ToolOutput::text(format!("Skill '{}' executed", name))
    }
}
