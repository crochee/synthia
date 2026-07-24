use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use synthia_tool::{
    traits::Tool,
    types::{ToolInput, ToolOutput},
};

use crate::{
    discovery::ToolDefinition,
    manager::McpManager,
    mcp_tool::McpTool,
    types::{McpError, ToolSummary},
};

pub struct McpToolAdapter {
    pub server_id: String,
    pub tool_definition: ToolDefinition,
    pub manager: Arc<McpManager>,
}

impl McpToolAdapter {
    pub fn new(
        server_id: impl Into<String>,
        tool_definition: ToolDefinition,
        manager: Arc<McpManager>,
    ) -> Self {
        Self {
            server_id: server_id.into(),
            tool_definition,
            manager,
        }
    }

    pub async fn call(&self, input: ToolInput) -> ToolOutput {
        if self.manager.is_hybrid_mode_enabled() {
            if self
                .manager
                .is_hybrid_connection_connected(&self.server_id)
                .await
            {
                let result = self
                    .manager
                    .call_tool_hybrid(
                        &self.server_id,
                        &self.tool_definition.name,
                        input.input,
                    )
                    .await;
                return match result {
                    Ok(r) => McpTool::normalize_result(&r),
                    Err(e) => ToolOutput::error(format!(
                        "MCP tool call failed: {}",
                        e
                    )),
                };
            }
            if let Err(e) = self
                .manager
                .connect_hybrid_connection(&self.server_id)
                .await
            {
                return ToolOutput::error(format!("Failed to connect: {}", e));
            }
            let result = self
                .manager
                .call_tool_hybrid(
                    &self.server_id,
                    &self.tool_definition.name,
                    input.input,
                )
                .await;
            return match result {
                Ok(r) => McpTool::normalize_result(&r),
                Err(e) => {
                    ToolOutput::error(format!("MCP tool call failed: {}", e))
                }
            };
        }

        if !self.manager.is_connected(&self.server_id).await {
            return ToolOutput::error(format!(
                "MCP server '{}' is not connected",
                self.server_id
            ));
        }

        let arguments = input.input;

        let result = self
            .manager
            .call_tool(&self.server_id, &self.tool_definition.name, arguments)
            .await;

        match result {
            Ok(r) => McpTool::normalize_result(&r),
            Err(e) => ToolOutput::error(format!("MCP tool call failed: {}", e)),
        }
    }

    pub fn into_mcp_tool(self: Arc<Self>) -> McpTool {
        McpTool::new(
            self.tool_definition.name.clone(),
            self.tool_definition.description.clone(),
            self.tool_definition.input_schema.clone(),
            self.server_id.clone(),
            self.manager.clone(),
        )
    }

    pub fn name(&self) -> &str {
        &self.tool_definition.name
    }

    pub fn description(&self) -> &str {
        &self.tool_definition.description
    }

    pub fn parameters(&self) -> Value {
        self.tool_definition.input_schema.clone()
    }
}

#[async_trait]
impl Tool for McpToolAdapter {
    fn name(&self) -> &str {
        &self.tool_definition.name
    }

    fn description(&self) -> &str {
        &self.tool_definition.description
    }

    fn parameters(&self) -> Value {
        self.tool_definition.input_schema.clone()
    }

    async fn call(&self, input: ToolInput) -> ToolOutput {
        self.call(input).await
    }
}

impl TryFrom<(String, ToolDefinition, Arc<McpManager>)> for McpToolAdapter {
    type Error = McpError;

    fn try_from(
        (server_id, tool_definition, manager): (
            String,
            ToolDefinition,
            Arc<McpManager>,
        ),
    ) -> std::result::Result<Self, Self::Error> {
        Ok(Self::new(server_id, tool_definition, manager))
    }
}

impl TryFrom<ToolSummary> for McpToolAdapter {
    type Error = McpError;

    fn try_from(
        _summary: ToolSummary,
    ) -> std::result::Result<Self, Self::Error> {
        Err(McpError::ServerNotFound(
            "Cannot create McpToolAdapter from ToolSummary alone, needs manager".to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_adapter() -> (Arc<McpToolAdapter>, Arc<McpManager>) {
        let manager = Arc::new(McpManager::new());
        let tool_definition = ToolDefinition {
            name: "test-tool".to_string(),
            description: "A test MCP tool".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string" }
                }
            }),
        };
        let adapter = Arc::new(McpToolAdapter::new(
            "test-server",
            tool_definition,
            manager.clone(),
        ));
        (adapter, manager)
    }

    #[tokio::test]
    async fn test_adapter_call_when_not_connected() {
        let (adapter, _manager) = create_test_adapter();

        let input = ToolInput {
            name: "test-tool".to_string(),
            input: serde_json::json!({"query": "test"}),
            context: synthia_tool::types::ToolExecutionContext::new(
                "s1".to_string(),
                std::path::PathBuf::from("/tmp"),
            ),
        };

        let output = adapter.call(input).await;
        assert!(output.is_error.unwrap_or(false));
        assert!(output.content[0].text().unwrap().contains("not connected"));
    }

    #[test]
    fn test_adapter_metadata() {
        let (adapter, _manager) = create_test_adapter();

        assert_eq!(adapter.name(), "test-tool");
        assert_eq!(adapter.description(), "A test MCP tool");
        assert!(adapter.parameters().is_object());
    }

    #[tokio::test]
    async fn test_adapter_into_mcp_tool() {
        let (adapter, _manager) = create_test_adapter();
        let mcp_tool = adapter.clone().into_mcp_tool();

        assert_eq!(mcp_tool.name(), "test-tool");
        assert_eq!(mcp_tool.server_name(), "test-server");
    }

    #[tokio::test]
    async fn test_adapter_tool_trait_call() {
        let (adapter, _manager) = create_test_adapter();

        let input = ToolInput {
            name: "test-tool".to_string(),
            input: serde_json::json!({}),
            context: synthia_tool::types::ToolExecutionContext::new(
                "s1".to_string(),
                std::path::PathBuf::from("/tmp"),
            ),
        };

        let result = adapter.call(input).await;
        assert!(result.is_error.unwrap_or(false));
    }
}
