//! Remote trigger tool
//!
//! Tool for triggering remote operations or webhook endpoints.

use async_trait::async_trait;
use rmcp::model::{CallToolResult, Content};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::tools::Tool;

/// HTTP methods supported by RemoteTrigger
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum HttpMethod {
    #[default]
    Get,
    Post,
    Put,
    Delete,
    Patch,
    Head,
}

impl std::fmt::Display for HttpMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HttpMethod::Get => write!(f, "GET"),
            HttpMethod::Post => write!(f, "POST"),
            HttpMethod::Put => write!(f, "PUT"),
            HttpMethod::Delete => write!(f, "DELETE"),
            HttpMethod::Patch => write!(f, "PATCH"),
            HttpMethod::Head => write!(f, "HEAD"),
        }
    }
}

/// RemoteTrigger input
#[derive(Debug, Deserialize)]
pub struct RemoteTriggerInput {
    pub url: String,
    #[serde(default)]
    pub method: HttpMethod,
    pub headers: Option<serde_json::Map<String, serde_json::Value>>,
    pub body: Option<String>,
}

/// RemoteTriggerTool - trigger remote operations or webhook endpoints
#[derive(Clone)]
pub struct RemoteTriggerTool {
    client: reqwest::Client,
}

impl RemoteTriggerTool {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }
}

impl Default for RemoteTriggerTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for RemoteTriggerTool {
    fn name(&self) -> &str {
        "RemoteTrigger"
    }

    fn description(&self) -> &str {
        "Trigger a remote operation or webhook endpoint"
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "The URL to trigger"
                },
                "method": {
                    "type": "string",
                    "enum": ["GET", "POST", "PUT", "DELETE", "PATCH", "HEAD"],
                    "description": "HTTP method to use",
                    "default": "GET"
                },
                "headers": {
                    "type": "object",
                    "description": "Optional headers to include"
                },
                "body": {
                    "type": "string",
                    "description": "Optional body content"
                }
            },
            "required": ["url"]
        })
    }

    fn is_dangerous(&self, _args: &Value) -> bool {
        true
    }

    async fn call(&self, args: Value) -> CallToolResult {
        let input: RemoteTriggerInput = match serde_json::from_value(args) {
            Ok(i) => i,
            Err(e) => {
                return CallToolResult::error(vec![Content::text(format!(
                    "Invalid input: {e}"
                ))]);
            }
        };

        if input.url.is_empty() {
            return CallToolResult::error(vec![Content::text(
                "URL cannot be empty".to_string(),
            )]);
        }

        let mut request = match input.method {
            HttpMethod::Get => self.client.get(&input.url),
            HttpMethod::Post => self.client.post(&input.url),
            HttpMethod::Put => self.client.put(&input.url),
            HttpMethod::Delete => self.client.delete(&input.url),
            HttpMethod::Patch => self.client.patch(&input.url),
            HttpMethod::Head => self.client.head(&input.url),
        };

        // Add headers if provided
        if let Some(headers) = input.headers {
            for (key, value) in headers {
                if let Some(value_str) = value.as_str() {
                    request = request.header(&key, value_str);
                }
            }
        }

        // Add body if provided
        if let Some(body) = input.body {
            request = request.body(body);
        }

        match request.send().await {
            Ok(response) => {
                let status = response.status();
                let text = response.text().await.unwrap_or_default();

                let mut result_lines = Vec::new();
                result_lines.push(format!("Status: {status}"));
                result_lines.push(format!("Body:\n{text}"));

                CallToolResult::success(vec![Content::text(
                    result_lines.join("\n"),
                )])
            }
            Err(e) => CallToolResult::error(vec![Content::text(format!(
                "Failed to trigger remote endpoint: {e}"
            ))]),
        }
    }
}
