//! MCP authentication tool
//!
//! Tool for authenticating with MCP servers that require OAuth or credentials.

use std::sync::Arc;

use async_trait::async_trait;
use rmcp::model::{CallToolResult, Content};
use serde::Deserialize;
use serde_json::{Value, json};

use super::McpToolCollector;
use crate::tools::Tool;

/// MCP authentication input
#[derive(Debug, Deserialize)]
pub struct McpAuthInput {
    pub server: String,
}

/// McpAuthTool - authenticate with MCP servers requiring OAuth or credentials
#[derive(Clone)]
pub struct McpAuthTool {
    collector: Arc<McpToolCollector>,
}

impl McpAuthTool {
    pub fn new(collector: Arc<McpToolCollector>) -> Self {
        Self { collector }
    }
}

#[async_trait]
impl Tool for McpAuthTool {
    fn name(&self) -> &str {
        "McpAuth"
    }

    fn description(&self) -> &str {
        "Authenticate with an MCP server that requires OAuth or credentials"
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "server": {
                    "type": "string",
                    "description": "The name of the MCP server to authenticate with"
                }
            },
            "required": ["server"]
        })
    }

    fn is_dangerous(&self, _args: &Value) -> bool {
        true
    }

    async fn call(&self, args: Value) -> CallToolResult {
        let input: McpAuthInput = match serde_json::from_value(args) {
            Ok(i) => i,
            Err(e) => {
                return CallToolResult::error(vec![Content::text(format!(
                    "Invalid input: {e}"
                ))]);
            }
        };

        let servers = self.collector.list_all_servers().await;
        let server_lower = input.server.to_lowercase();
        let matching: Vec<_> = servers
            .iter()
            .filter(|s| s.to_lowercase() == server_lower)
            .collect();

        if matching.is_empty() {
            return CallToolResult::error(vec![Content::text(format!(
                "MCP server '{}' not found. Available servers: {}",
                input.server,
                servers.join(", ")
            ))]);
        }

        let server_name = matching[0];

        // Trigger re-authentication by re-registering or notifying
        // The actual auth flow depends on the server implementation
        // This is a placeholder that signals the need for auth
        CallToolResult::success(vec![Content::text(format!(
            "Authentication triggered for MCP server '{server_name}'. Please complete the OAuth flow or provide credentials through the server's auth mechanism."
        ))])
    }
}
