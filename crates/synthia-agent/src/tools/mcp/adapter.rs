//! MCP tool adapter implementation

use async_trait::async_trait;
use rmcp::{
    model::{CallToolRequestParams, CallToolResult, Tool as McpTool},
    service::ServerSink,
};
use serde_json::Value;

use crate::tools::Tool;

#[derive(Clone)]
pub struct McpToolAdapter {
    tool: McpTool,
    server: ServerSink,
}

impl McpToolAdapter {
    pub fn new(tool: McpTool, server: ServerSink) -> Self {
        Self { tool, server }
    }
}

#[async_trait]
impl Tool for McpToolAdapter {
    fn name(&self) -> &str {
        &self.tool.name
    }

    fn description(&self) -> &str {
        self.tool.description.as_deref().unwrap_or("")
    }

    fn parameters(&self) -> Value {
        serde_json::to_value(&self.tool.input_schema)
            .unwrap_or(Value::Object(Default::default()))
    }

    async fn call(&self, args: Value) -> CallToolResult {
        let arguments = match args {
            Value::Object(map) => Some(map),
            _ => {
                return CallToolResult::error(vec![
                    rmcp::model::Content::text(
                        "Invalid arguments format".to_string(),
                    ),
                ]);
            }
        };

        match self
            .server
            .call_tool(CallToolRequestParams {
                meta: None,
                name: self.tool.name.clone(),
                arguments,
                task: None,
            })
            .await
        {
            Ok(result) => result,
            Err(e) => CallToolResult::error(vec![rmcp::model::Content::text(
                format!("MCP tool error: {e}"),
            )]),
        }
    }
}
