//! AskUserQuestion tool implementation
//!
//! Ask the user questions during execution.

use std::sync::Arc;

use async_trait::async_trait;
use rmcp::model::{CallToolResult, Content};
use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;

use super::{
    QuestionSender,
    types::{Question, QuestionOption, QuestionRequest},
};
use crate::tools::Tool;

#[derive(Debug, Clone, Deserialize)]
struct QuestionParam {
    question: String,
    #[serde(default)]
    header: String,
    options: Vec<QuestionOptionParam>,
    #[serde(default)]
    multi_select: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct QuestionOptionParam {
    label: String,
    #[serde(default)]
    description: String,
}

#[derive(Debug, Clone, Deserialize)]
struct AskQuestionRequest {
    questions: Vec<QuestionParam>,
}

#[derive(Debug, Clone)]
pub struct AskUserQuestionTool<T: QuestionSender> {
    sender: Arc<T>,
}

impl<T: QuestionSender> AskUserQuestionTool<T> {
    pub fn new(sender: Arc<T>) -> Self {
        Self { sender }
    }
}

#[async_trait]
impl<T: QuestionSender> Tool for AskUserQuestionTool<T> {
    fn name(&self) -> &str {
        "askUserQuestion"
    }

    fn description(&self) -> &str {
        "Ask user multiple choice questions (1-4 questions, 2-4 options)."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "questions": {
                    "type": "array",
                    "minItems": 1,
                    "maxItems": 4,
                    "items": {
                        "type": "object",
                        "properties": {
                            "question": {
                                "type": "string",
                                "description": "Question"
                            },
                            "header": {
                                "type": "string",
                                "description": "Label (max 12 chars)"
                            },
                            "multiSelect": {
                                "type": "boolean",
                                "default": false,
                                "description": "Multi-select"
                            },
                            "options": {
                                "type": "array",
                                "minItems": 2,
                                "maxItems": 4,
                                "items": {
                                    "type": "object",
                                    "properties": {
                                        "label": {
                                            "type": "string",
                                            "description": "Option label"
                                        },
                                        "description": {
                                            "type": "string",
                                            "description": "Option description"
                                        }
                                    },
                                    "required": ["label", "description"]
                                },
                                "description": "The available choices for this question. Must have 2-4 options."
                            }
                        },
                        "required": ["question", "header", "options", "multiSelect"]
                    },
                    "description": "Questions to ask the user (1-4 questions)"
                }
            },
            "required": ["questions"]
        })
    }

    async fn call(&self, args: Value) -> CallToolResult {
        let request: AskQuestionRequest = match serde_json::from_value(args) {
            Ok(r) => r,
            Err(e) => {
                return CallToolResult::error(vec![Content::text(format!(
                    "Invalid arguments: {e}"
                ))]);
            }
        };

        let questions: Vec<Question> = request
            .questions
            .into_iter()
            .map(|q| Question {
                question: q.question,
                header: q.header,
                options: q
                    .options
                    .into_iter()
                    .map(|o| QuestionOption {
                        label: o.label,
                        description: o.description,
                    })
                    .collect(),
                multi_select: q.multi_select,
            })
            .collect();

        let question_request = QuestionRequest {
            id: Uuid::new_v4().to_string(),
            tool_call_id: String::new(),
            questions,
        };

        let result = self.sender.send_question(question_request).await;

        match result {
            Ok(response) => {
                let output = serde_json::json!({
                    "answers": response.answers,
                });
                CallToolResult::success(vec![Content::text(output.to_string())])
            }
            Err(e) => {
                tracing::error!("Failed to get user response: {}", e);
                CallToolResult::error(vec![Content::text(
                    serde_json::json!({
                        "is_error": true,
                        "message": format!("Failed to get user response: {}", e)
                    })
                    .to_string(),
                )])
            }
        }
    }
}
