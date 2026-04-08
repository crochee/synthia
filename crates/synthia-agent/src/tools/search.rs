//! Tool search functionality for finding tools by name or keywords.

use std::sync::Weak;

use async_trait::async_trait;
use rmcp::model::{CallToolResult, Content};
use serde_json::{Value, json};

use super::{Tool, ToolRegistry};

pub struct ToolSearchTool {
    registry: Weak<ToolRegistry>,
}

impl ToolSearchTool {
    pub fn new(registry: Weak<ToolRegistry>) -> Self {
        Self { registry }
    }
}

#[async_trait]
impl Tool for ToolSearchTool {
    fn name(&self) -> &str {
        "search"
    }

    fn description(&self) -> &str {
        "Search for tools by exact name or keywords in their name and description"
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "The search query (tool name or keyword)"
                },
                "max_results": {
                    "type": "integer",
                    "minimum": 1,
                    "description": "Maximum number of results to return (default: 10)",
                    "default": 10
                }
            },
            "required": ["query"]
        })
    }

    async fn call(&self, args: Value) -> CallToolResult {
        let query = match args.get("query") {
            Some(Value::String(q)) => q.to_lowercase(),
            _ => {
                return CallToolResult::error(vec![Content::text(
                    "Missing required parameter: query",
                )]);
            }
        };

        let max_results = args
            .get("max_results")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(10)
            .max(1) as usize;

        let registry = match self.registry.upgrade() {
            Some(r) => r,
            None => {
                return CallToolResult::error(vec![Content::text(
                    "Tool registry no longer available",
                )]);
            }
        };

        let all_tools = registry.filtered_tools(&[], &[]).await;

        let matches: Vec<_> = all_tools
            .iter()
            .filter(|tool| {
                let name = tool.name().to_lowercase();
                let desc = tool.description().to_lowercase();
                name.contains(&query)
                    || desc.contains(&query)
                    || query.contains(&name)
            })
            .take(max_results)
            .map(|tool| {
                json!({
                    "name": tool.name(),
                    "description": tool.description(),
                    "parameters": tool.parameters()
                })
            })
            .collect();

        let content = if matches.is_empty() {
            serde_json::json!({
                "message": format!("No tools found matching '{}'", query)
            })
        } else {
            serde_json::json!({
                "tools": matches,
                "count": matches.len()
            })
        };

        CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&content).unwrap_or_default(),
        )])
    }

    fn is_read_only(&self, _args: &Value) -> bool {
        true
    }
}
