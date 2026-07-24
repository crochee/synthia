use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// The type of response the mock LLM should return
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ResponseType {
    TextOnly,
    ToolCalls,
    Mixed,
    Error,
}

/// Represents a tool call in a mock LLM response
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MockToolCall {
    pub id: String,
    pub name: String,
    pub input: serde_json::Value,
}

/// Represents an error that can be returned by the mock LLM
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MockError {
    pub status: u16,
    pub message: String,
}

impl MockError {
    pub fn rate_limit(retry_after_secs: Option<u64>) -> Self {
        let msg = match retry_after_secs {
            Some(s) => format!("Rate limited. Retry after {s}s"),
            None => "Rate limited".to_string(),
        };
        Self {
            status: 429,
            message: msg,
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self {
            status: 500,
            message: message.into(),
        }
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: 400,
            message: message.into(),
        }
    }

    pub(super) fn to_json(&self) -> Result<String> {
        let body = serde_json::json!({
            "error": {
                "message": self.message,
                "type": match self.status {
                    429 => "rate_limit_error",
                    400 => "invalid_request_error",
                    _ => "api_error",
                },
                "code": self.status,
            }
        });
        serde_json::to_string(&body).context("failed to serialize error")
    }
}

/// A scripted response that the mock LLM will return
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScriptedResponse {
    pub response_type: ResponseType,
    pub content: String,
    #[serde(default)]
    pub tool_calls: Vec<MockToolCall>,
    pub error: Option<MockError>,
}

impl ScriptedResponse {
    /// Create a text-only response
    pub fn text(content: impl Into<String>) -> Self {
        Self {
            response_type: ResponseType::TextOnly,
            content: content.into(),
            tool_calls: vec![],
            error: None,
        }
    }

    /// Create a response with tool calls (and optional accompanying text)
    pub fn with_tools(
        content: impl Into<String>,
        tool_calls: Vec<MockToolCall>,
    ) -> Self {
        let content_str = content.into();
        Self {
            response_type: if content_str.is_empty() {
                ResponseType::ToolCalls
            } else {
                ResponseType::Mixed
            },
            content: content_str,
            tool_calls,
            error: None,
        }
    }

    /// Create an error response
    pub fn error(err: MockError) -> Self {
        Self {
            response_type: ResponseType::Error,
            content: String::new(),
            tool_calls: vec![],
            error: Some(err),
        }
    }

    /// Serialize to JSON matching the LLM API response format
    pub fn to_json(&self) -> Result<String> {
        if let Some(ref err) = self.error {
            let body = serde_json::json!({
                "error": {
                    "message": err.message,
                    "type": match err.status {
                        429 => "rate_limit_error",
                        400 => "invalid_request_error",
                        _ => "api_error",
                    },
                    "code": err.status,
                }
            });
            return serde_json::to_string(&body)
                .context("failed to serialize error response");
        }

        // Build content blocks array
        let mut content_blocks: Vec<serde_json::Value> = vec![];

        // Add text block if there is content
        if !self.content.is_empty() {
            content_blocks.push(serde_json::json!({
                "type": "text",
                "text": self.content,
            }));
        }

        // Add tool use blocks
        for tc in &self.tool_calls {
            content_blocks.push(serde_json::json!({
                "type": "tool_use",
                "id": tc.id,
                "name": tc.name,
                "input": tc.input,
            }));
        }

        let body = serde_json::json!({
            "id": "msg_mock_001",
            "type": "message",
            "role": "assistant",
            "content": content_blocks,
            "model": "mock-model",
            "usage": {
                "input_tokens": 100,
                "output_tokens": 50,
            },
            "stop_reason": match self.response_type {
                ResponseType::Error => serde_json::Value::Null,
                _ => serde_json::Value::String("end_turn".to_string()),
            },
        });

        serde_json::to_string(&body)
            .context("failed to serialize scripted response")
    }
}
