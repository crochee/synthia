use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use synthia_tool::{
    traits::Tool,
    types::{ToolInput, ToolOutput},
};

use crate::manager::McpManager;

/// Default timeout for MCP tool calls (60 seconds).
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(60);

/// Wrapper that turns an MCP-discovered tool into a `synthia_tool::Tool`.
///
/// Holds tool metadata (name, description, input schema) and a reference to
/// the `McpManager` for execution delegation.
pub struct McpTool {
    /// The MCP tool name.
    name: String,
    /// The MCP tool description.
    description: String,
    /// JSON Schema for the tool's input parameters.
    input_schema: serde_json::Value,
    /// Name of the MCP server that provides this tool.
    server_name: String,
    /// Shared reference to the MCP manager for execution.
    manager: Arc<McpManager>,
    /// Timeout for tool execution.
    timeout: Duration,
}

/// Serialisable representation of McpTool metadata (used for checkpointing).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolMeta {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
    pub server_name: String,
}

impl McpTool {
    /// Create a new McpTool from discovered metadata.
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        input_schema: serde_json::Value,
        server_name: impl Into<String>,
        manager: Arc<McpManager>,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            input_schema,
            server_name: server_name.into(),
            manager,
            timeout: DEFAULT_TIMEOUT,
        }
    }

    /// Create a new McpTool with a custom timeout.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Returns the MCP server name this tool belongs to.
    pub fn server_name(&self) -> &str {
        &self.server_name
    }

    /// Returns the tool metadata as a serialisable struct.
    pub fn meta(&self) -> McpToolMeta {
        McpToolMeta {
            name: self.name.clone(),
            description: self.description.clone(),
            input_schema: self.input_schema.clone(),
            server_name: self.server_name.clone(),
        }
    }

    /// Normalise an MCP call result into `ToolOutput`.
    pub fn normalize_result(result: &serde_json::Value) -> ToolOutput {
        // MCP tool results follow the spec:
        // { "content": [{ "type": "text", "text": "..." }, { "type": "image", ... }] }
        // Or can be arbitrary JSON for simpler servers.

        if let Some(content_array) =
            result.get("content").and_then(|c| c.as_array())
        {
            let texts: Vec<String> = content_array
                .iter()
                .filter_map(|item| {
                    if item.get("type").and_then(|t| t.as_str()) == Some("text")
                    {
                        item.get("text")
                            .and_then(|t| t.as_str())
                            .map(String::from)
                    } else {
                        Some(serde_json::to_string(item).unwrap_or_default())
                    }
                })
                .collect();

            if texts.is_empty() {
                ToolOutput::text(
                    serde_json::to_string_pretty(result).unwrap_or_default(),
                )
            } else {
                ToolOutput::text(texts.join("\n"))
            }
        } else if let Some(text) = result.get("text").and_then(|t| t.as_str()) {
            ToolOutput::text(text)
        } else if let Some(is_error) =
            result.get("isError").and_then(|v| v.as_bool())
        {
            if is_error {
                ToolOutput::error(
                    serde_json::to_string_pretty(result).unwrap_or_default(),
                )
            } else {
                ToolOutput::text(
                    serde_json::to_string_pretty(result).unwrap_or_default(),
                )
            }
        } else {
            ToolOutput::text(
                serde_json::to_string_pretty(result).unwrap_or_default(),
            )
        }
    }
}

#[async_trait]
impl Tool for McpTool {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn parameters(&self) -> serde_json::Value {
        self.input_schema.clone()
    }

    async fn call(&self, input: ToolInput) -> ToolOutput {
        // Check connection status first
        if !self.manager.is_connected(&self.server_name).await {
            return ToolOutput::error("MCP server not connected");
        }

        // Execute with timeout
        let manager = Arc::clone(&self.manager);
        let server_name = self.server_name.clone();
        let tool_name = self.name.clone();
        let arguments = input.input.clone();

        let result = tokio::time::timeout(self.timeout, async move {
            manager.call_tool(&server_name, &tool_name, arguments).await
        })
        .await;

        match result {
            Ok(Ok(value)) => Self::normalize_result(&value),
            Ok(Err(e)) => {
                ToolOutput::error(format!("MCP tool call failed: {}", e))
            }
            Err(_) => ToolOutput::error(format!(
                "MCP tool call timed out after {:.1}s",
                self.timeout.as_secs_f64()
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mcp_tool_creation() {
        let _schema = serde_json::json!({
            "type": "object",
            "properties": {
                "query": { "type": "string" }
            }
        });
        // Can't easily test with real McpManager, so test the normalize_result helper
        let result = serde_json::json!({
            "content": [
                { "type": "text", "text": "Hello, world!" }
            ]
        });
        let output = McpTool::normalize_result(&result);
        assert!(!output.is_error.unwrap_or(false));
        assert!(output.content[0].text().unwrap().contains("Hello, world!"));
    }

    #[test]
    fn test_normalize_result_with_error_flag() {
        let result = serde_json::json!({
            "isError": true,
            "message": "Something went wrong"
        });
        let output = McpTool::normalize_result(&result);
        assert!(output.is_error.unwrap_or(false));
    }

    #[test]
    fn test_normalize_result_with_text_field() {
        let result = serde_json::json!({
            "text": "Direct text result"
        });
        let output = McpTool::normalize_result(&result);
        assert!(
            output.content[0]
                .text()
                .unwrap()
                .contains("Direct text result")
        );
    }

    #[test]
    fn test_normalize_result_with_multiple_content() {
        let result = serde_json::json!({
            "content": [
                { "type": "text", "text": "First line" },
                { "type": "text", "text": "Second line" }
            ]
        });
        let output = McpTool::normalize_result(&result);
        let text = output.content[0].text().unwrap();
        assert!(text.contains("First line"));
        assert!(text.contains("Second line"));
    }

    #[test]
    fn test_normalize_result_fallback() {
        let result = serde_json::json!({
            "arbitrary": "data",
            "numbers": [1, 2, 3]
        });
        let output = McpTool::normalize_result(&result);
        assert!(!output.is_error.unwrap_or(false));
        let text = output.content[0].text().unwrap();
        assert!(text.contains("arbitrary"));
    }

    #[test]
    fn test_normalize_result_empty_content_array() {
        let result = serde_json::json!({
            "content": []
        });
        let output = McpTool::normalize_result(&result);
        // Should fall back to pretty-printing the result
        assert!(!output.is_error.unwrap_or(false));
    }

    #[test]
    fn test_mcp_tool_meta() {
        let manager = Arc::new(McpManager::new());
        let tool = McpTool::new(
            "search",
            "Search the web",
            serde_json::json!({"type": "object"}),
            "web-server",
            manager,
        );

        let meta = tool.meta();
        assert_eq!(meta.name, "search");
        assert_eq!(meta.description, "Search the web");
        assert_eq!(meta.server_name, "web-server");
    }

    #[test]
    fn test_mcp_tool_server_name() {
        let manager = Arc::new(McpManager::new());
        let tool = McpTool::new(
            "search",
            "Search the web",
            serde_json::json!({}),
            "web-server",
            manager,
        );
        assert_eq!(tool.server_name(), "web-server");
    }

    #[test]
    fn test_mcp_tool_with_timeout() {
        let manager = Arc::new(McpManager::new());
        let tool = McpTool::new(
            "search",
            "Search",
            serde_json::json!({}),
            "web-server",
            manager,
        )
        .with_timeout(Duration::from_secs(30));

        assert_eq!(tool.timeout, Duration::from_secs(30));
    }

    #[tokio::test]
    async fn test_mcp_tool_call_disconnected() {
        let manager = Arc::new(McpManager::new());
        let tool = McpTool::new(
            "search",
            "Search",
            serde_json::json!({}),
            "nonexistent-server",
            manager,
        );

        let input = ToolInput {
            name: "search".to_string(),
            input: serde_json::json!({"query": "test"}),
            context: synthia_tool::types::ToolExecutionContext::new(
                "s1".to_string(),
                std::path::PathBuf::from("/tmp"),
            ),
        };

        let output = tool.call(input).await;
        assert!(output.is_error.unwrap_or(false));
        assert!(output.content[0].text().unwrap().contains("not connected"));
    }
}
