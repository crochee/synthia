//! Tool service for tool management logic

use std::sync::Arc;

use rmcp::model::CallToolResult;
use serde_json::Value;
use synthia_agent::tools::ToolRegistry;

use super::types::{ToolInfo, tool_info_from_tool};
use crate::error::ServerError;

pub struct ToolService {
    registry: Arc<ToolRegistry>,
}

impl ToolService {
    pub fn new(registry: Arc<ToolRegistry>) -> Self {
        Self { registry }
    }

    pub fn list(&self) -> Vec<ToolInfo> {
        let tool_names = self.registry.tool_names();
        let mut tools = Vec::with_capacity(tool_names.len());
        for name in tool_names {
            if let Some(tool) = self.registry.get_tool(&name) {
                tools.push(tool_info_from_tool(tool));
            }
        }
        tools
    }

    pub fn get(&self, name: &str) -> Option<ToolInfo> {
        self.registry.get_tool(name).map(tool_info_from_tool)
    }

    pub async fn execute(
        &self,
        name: &str,
        arguments: Value,
    ) -> Result<CallToolResult, ServerError> {
        self.registry
            .execute_with_tool(name, &arguments, |tool| {
                let arguments = arguments.clone();
                async move { tool.call(arguments).await }
            })
            .await
            .map_err(|e| ServerError::ToolError(e.to_string()))
    }
}
