use async_trait::async_trait;
use serde_json::json;
use synthia_tool::{
    traits::Tool,
    types::{ToolInput, ToolOutput},
};

pub struct AskUserQuestionTool;

#[async_trait]
impl Tool for AskUserQuestionTool {
    fn name(&self) -> &str {
        "ask_user_question"
    }

    fn description(&self) -> &str {
        "Asks multiple-choice questions to gather requirements or clarify ambiguity"
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "questions": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "question": { "type": "string", "description": "The question to ask" },
                            "header": { "type": "string", "description": "Short label displayed as a chip/tag" },
                            "options": {
                                "type": "array",
                                "items": {
                                    "type": "object",
                                    "properties": {
                                        "label": { "type": "string", "description": "Display text for the option" },
                                        "description": { "type": "string", "description": "Explanation of the option" }
                                    },
                                    "required": ["label", "description"]
                                },
                                "minItems": 2,
                                "maxItems": 4,
                                "description": "The available choices (2-4 options)"
                            },
                            "multiSelect": { "type": "boolean", "description": "Allow selecting multiple options" }
                        },
                        "required": ["question", "header", "options"]
                    },
                    "minItems": 1,
                    "maxItems": 4,
                    "description": "Questions to ask the user (1-4 questions)"
                }
            },
            "required": ["questions"]
        })
    }

    async fn call(&self, _input: ToolInput) -> ToolOutput {
        ToolOutput::text("User response required".to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DoomLoopResponse {
    AllowOnce,
    Deny,
    Cancel,
}

pub struct DefaultDoomLoopHandler;

impl DefaultDoomLoopHandler {
    pub async fn handle_doom_loop(
        &self,
        tool_name: &str,
        _input_json: &str,
        iteration: usize,
    ) -> DoomLoopResponse {
        tracing::warn!(
            tool_name = %tool_name,
            iteration = %iteration,
            "Doom loop detected - user permission required to continue"
        );
        DoomLoopResponse::Cancel
    }
}
