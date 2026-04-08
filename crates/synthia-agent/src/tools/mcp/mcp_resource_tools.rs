//! MCP resource tools module
//!
//! This module provides tools for accessing MCP server resources.

use std::sync::Arc;

use async_trait::async_trait;
use rmcp::model::{CallToolResult, Content, Resource, ResourceContents};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::McpToolCollector;
use crate::tools::Tool;

/// List MCP resources input
#[derive(Debug, Deserialize)]
pub struct ListMcpResourcesInput {
    pub server: Option<String>,
}

/// List MCP resources output
#[derive(Debug, Serialize)]
pub struct ListMcpResourcesOutput {
    pub resources: Vec<ResourceInfo>,
}

/// Resource info for JSON serialization
#[derive(Debug, Serialize)]
pub struct ResourceInfo {
    pub uri: String,
    pub name: String,
    pub description: Option<String>,
    pub mime_type: Option<String>,
}

impl From<Resource> for ResourceInfo {
    fn from(resource: Resource) -> Self {
        let uri = resource.raw.uri.clone();
        let name = resource.raw.name.clone();
        Self {
            uri,
            name,
            description: resource.raw.description,
            mime_type: resource.raw.mime_type,
        }
    }
}

/// ReadMcpResource input
#[derive(Debug, Deserialize)]
pub struct ReadMcpResourceInput {
    pub server: Option<String>,
    pub uri: String,
}

/// ReadMcpResource tool - reads a specific MCP resource
#[derive(Clone)]
pub struct ReadMcpResourceTool {
    collector: Arc<McpToolCollector>,
}

impl ReadMcpResourceTool {
    pub fn new(collector: Arc<McpToolCollector>) -> Self {
        Self { collector }
    }
}

/// ListMcpResourcesTool - lists MCP server resources
#[derive(Clone)]
pub struct ListMcpResourcesTool {
    collector: Arc<McpToolCollector>,
}

impl ListMcpResourcesTool {
    pub fn new(collector: Arc<McpToolCollector>) -> Self {
        Self { collector }
    }
}

#[async_trait]
impl Tool for ListMcpResourcesTool {
    fn name(&self) -> &str {
        "ListMcpResources"
    }

    fn description(&self) -> &str {
        "List all available resources from connected MCP servers"
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "server": {
                    "type": "string",
                    "description": "Optional server name to list resources from. If not provided, lists resources from all servers."
                }
            }
        })
    }

    async fn call(&self, args: Value) -> CallToolResult {
        let input: ListMcpResourcesInput = match serde_json::from_value(args) {
            Ok(i) => i,
            Err(e) => {
                return CallToolResult::error(vec![Content::text(format!(
                    "Invalid input: {e}"
                ))]);
            }
        };

        let collector = Arc::clone(&self.collector);

        let all_resources = if let Some(server_name) = input.server {
            // List from specific server
            match collector.list_server_resources(&server_name).await {
                Ok(res) => res
                    .into_iter()
                    .map(|r| {
                        let mut info = ResourceInfo::from(r);
                        info.name = format!("{}: {}", server_name, info.name);
                        info
                    })
                    .collect(),
                Err(e) => {
                    return CallToolResult::error(vec![Content::text(
                        format!(
                            "Failed to list resources from server '{server_name}': {e}"
                        ),
                    )]);
                }
            }
        } else {
            // List from all servers
            let servers = collector.list_all_servers().await;
            let mut resources = Vec::new();

            for server_name in servers {
                match collector.list_server_resources(&server_name).await {
                    Ok(res) => {
                        resources.extend(res.into_iter().map(|r| {
                            let mut info = ResourceInfo::from(r);
                            info.name = format!("{server_name}: {}", info.name);
                            info
                        }));
                    }
                    Err(e) => {
                        return CallToolResult::error(vec![Content::text(
                            format!(
                                "Failed to list resources from server '{server_name}': {e}"
                            ),
                        )]);
                    }
                }
            }
            resources
        };

        let output = ListMcpResourcesOutput {
            resources: all_resources,
        };

        CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&output).unwrap_or_else(|_| {
                "{\"error\": \"serialization failed\"}".to_string()
            }),
        )])
    }
}

#[async_trait]
impl Tool for ReadMcpResourceTool {
    fn name(&self) -> &str {
        "ReadMcpResource"
    }

    fn description(&self) -> &str {
        "Read a specific resource from an MCP server"
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "server": {
                    "type": "string",
                    "description": "Optional server name. If not provided, searches all servers."
                },
                "uri": {
                    "type": "string",
                    "description": "The URI of the resource to read"
                }
            },
            "required": ["uri"]
        })
    }

    async fn call(&self, args: Value) -> CallToolResult {
        let input: ReadMcpResourceInput = match serde_json::from_value(args) {
            Ok(i) => i,
            Err(e) => {
                return CallToolResult::error(vec![Content::text(format!(
                    "Invalid input: {e}"
                ))]);
            }
        };

        let collector = Arc::clone(&self.collector);

        if let Some(ref server_name) = input.server {
            // Read from specific server
            match collector
                .read_server_resource(server_name, &input.uri)
                .await
            {
                Ok(contents) => {
                    let mut result_lines = Vec::new();
                    for content in contents {
                        match content {
                            ResourceContents::TextResourceContents {
                                uri,
                                text,
                                mime_type,
                                ..
                            } => {
                                result_lines.push(format!("URI: {uri}"));
                                if let Some(mt) = mime_type {
                                    result_lines
                                        .push(format!("MIME Type: {mt}"));
                                }
                                result_lines.push(text);
                            }
                            ResourceContents::BlobResourceContents {
                                uri,
                                blob,
                                mime_type,
                                ..
                            } => {
                                result_lines.push(format!("URI: {uri}"));
                                result_lines
                                    .push(format!("MIME Type: {mime_type:?}"));
                                result_lines.push(format!(
                                    "(Binary blob, {} bytes)",
                                    blob.len()
                                ));
                            }
                        }
                    }
                    CallToolResult::success(vec![Content::text(
                        result_lines.join("\n"),
                    )])
                }
                Err(e) => CallToolResult::error(vec![Content::text(format!(
                    "Failed to read resource: {e}"
                ))]),
            }
        } else {
            // Search all servers
            let servers = collector.list_all_servers().await;

            for server_name in servers {
                if let Ok(contents) = collector
                    .read_server_resource(&server_name, &input.uri)
                    .await
                {
                    let mut result_lines = Vec::new();
                    result_lines
                        .push(format!("Found on server '{server_name}':"));
                    for content in contents {
                        match content {
                            ResourceContents::TextResourceContents {
                                uri,
                                text,
                                mime_type,
                                ..
                            } => {
                                result_lines.push(format!("URI: {uri}"));
                                if let Some(mt) = mime_type {
                                    result_lines
                                        .push(format!("MIME Type: {mt}"));
                                }
                                result_lines.push(text);
                            }
                            ResourceContents::BlobResourceContents {
                                uri,
                                blob,
                                mime_type,
                                ..
                            } => {
                                result_lines.push(format!("URI: {uri}"));
                                result_lines
                                    .push(format!("MIME Type: {mime_type:?}"));
                                result_lines.push(format!(
                                    "(Binary blob, {} bytes)",
                                    blob.len()
                                ));
                            }
                        }
                    }
                    return CallToolResult::success(vec![Content::text(
                        result_lines.join("\n"),
                    )]);
                }
            }

            CallToolResult::error(vec![Content::text(format!(
                "Resource '{}' not found in any server",
                input.uri
            ))])
        }
    }
}
