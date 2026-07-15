//! Built-in tool provider for MCP-shaped tools.
//!
//! Wraps the static tool surface defined in `synthia-mcp` (`McpTool`,
//! `McpToolAdapter`). The real tool list is populated dynamically by
//! `McpManager::discover`, but the provider advertises the same names
//! and JSON Schemas so downstream dispatch can locate them by string
//! before any MCP server is connected.

use async_trait::async_trait;

use crate::tools::dynamic_provider::{SchemaRef, ToolDefinition, ToolProvider};

/// Provider for MCP-shaped tools: echo, list, and read.
#[derive(Clone)]
pub struct MCPToolsProvider;

impl MCPToolsProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for MCPToolsProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ToolProvider for MCPToolsProvider {
    fn name(&self) -> &'static str {
        "mcp_tools"
    }

    fn list_tools(&self) -> Vec<ToolDefinition> {
        vec![
            ToolDefinition {
                name: "mcp_echo".to_string(),
                description:
                    "Echoes the supplied message back through the MCP tool surface for connectivity checks."
                        .to_string(),
                parameters: SchemaRef::Inline(serde_json::json!({
                    "type": "object",
                    "properties": {
                        "message": {
                            "type": "string",
                            "description": "The message to echo back from the MCP server."
                        }
                    },
                    "required": ["message"]
                })),
                deprecated: None,
            },
            ToolDefinition {
                name: "mcp_list".to_string(),
                description: "Lists the tools advertised by connected MCP servers."
                    .to_string(),
                parameters: SchemaRef::Inline(serde_json::json!({
                    "type": "object",
                    "properties": {
                        "server": {
                            "type": "string",
                            "description": "Optional MCP server name to scope the listing to."
                        }
                    }
                })),
                deprecated: None,
            },
            ToolDefinition {
                name: "mcp_read".to_string(),
                description:
                    "Reads a resource from an MCP server by URI, mirroring the MCP resources/read method."
                        .to_string(),
                parameters: SchemaRef::Inline(serde_json::json!({
                    "type": "object",
                    "properties": {
                        "uri": {
                            "type": "string",
                            "description": "The MCP resource URI to read."
                        }
                    },
                    "required": ["uri"]
                })),
                deprecated: None,
            },
        ]
    }
}
