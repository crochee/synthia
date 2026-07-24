use std::collections::HashMap;

use rmcp::service::{Peer, RoleClient};
use serde::{Deserialize, Serialize};
use serde_json::Map;
use tokio::sync::RwLock;

use crate::types::{McpError, ToolSummary};

/// Represents the full tool definition including schema and parameters (Level 1).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

/// Discovery layer with Level 0 (summary-only) and Level 1 (full schema) caching.
pub struct ToolDiscovery {
    /// Level 0 cache: server_name -> ToolSummary list (cached per connection lifetime)
    summary_cache: RwLock<HashMap<String, Vec<ToolSummary>>>,
    /// Level 1 cache: tool_name -> ToolDefinition (cached for session lifetime)
    schema_cache: RwLock<HashMap<String, ToolDefinition>>,
}

impl ToolDiscovery {
    pub fn new() -> Self {
        Self {
            summary_cache: RwLock::new(HashMap::new()),
            schema_cache: RwLock::new(HashMap::new()),
        }
    }

    /// Level 0: Discover tools returning ToolSummary list (name + description only).
    /// Results are cached for the connection lifetime.
    pub async fn discover_tools(
        &self,
        server_name: &str,
        peer: &mut Peer<RoleClient>,
    ) -> Result<Vec<ToolSummary>, McpError> {
        // Check cache first
        {
            let cache = self.summary_cache.read().await;
            if let Some(summaries) = cache.get(server_name) {
                return Ok(summaries.clone());
            }
        }

        // Fetch from MCP server
        let summaries = Self::fetch_tool_summaries(peer, server_name).await?;

        // Cache the results
        self.summary_cache
            .write()
            .await
            .insert(server_name.to_string(), summaries.clone());

        Ok(summaries)
    }

    /// Level 1: Load full ToolDefinition (with schema) for a specific tool.
    /// Loads on first call and caches for session lifetime.
    pub async fn get_tool_definition(
        &self,
        tool_name: &str,
        peer: &mut Peer<RoleClient>,
        server_name: &str,
    ) -> Result<ToolDefinition, McpError> {
        // Check schema cache first
        {
            let cache = self.schema_cache.read().await;
            if let Some(definition) = cache.get(tool_name) {
                return Ok(definition.clone());
            }
        }

        // Load full definition from MCP server
        let definition =
            Self::fetch_tool_definition(tool_name, peer, server_name).await?;

        // Cache for session lifetime
        self.schema_cache
            .write()
            .await
            .insert(tool_name.to_string(), definition.clone());

        Ok(definition)
    }

    /// List all cached tool summaries.
    pub async fn list_summaries(&self) -> Vec<ToolSummary> {
        self.summary_cache
            .read()
            .await
            .values()
            .flat_map(|summaries| summaries.iter().cloned())
            .collect()
    }

    /// Check if a tool's schema is already cached.
    pub async fn has_cached_definition(&self, tool_name: &str) -> bool {
        self.schema_cache.read().await.contains_key(tool_name)
    }

    /// Clear all caches. Called on server disconnect.
    pub async fn clear_cache(&self) {
        self.summary_cache.write().await.clear();
        self.schema_cache.write().await.clear();
    }

    /// Clear only the summary cache for a specific server.
    pub async fn clear_summary_cache(&self, server_name: &str) {
        self.summary_cache.write().await.remove(server_name);
    }

    /// Clear only the schema cache for a specific tool.
    pub async fn clear_schema_cache(&self, tool_name: &str) {
        self.schema_cache.write().await.remove(tool_name);
    }

    /// Fetch tool summaries from MCP server (Level 0: name + description only).
    async fn fetch_tool_summaries(
        peer: &mut Peer<RoleClient>,
        server_name: &str,
    ) -> Result<Vec<ToolSummary>, McpError> {
        let result = peer.list_tools(None).await.map_err(|e| {
            McpError::ServerNotFound(format!("list_tools error: {}", e))
        })?;

        let summaries: Vec<ToolSummary> = result
            .tools
            .iter()
            .map(|t| ToolSummary {
                name: t.name.to_string(),
                description: t.description.to_string(),
            })
            .collect();

        tracing::debug!(
            server = %server_name,
            count = summaries.len(),
            "Fetched tool summaries from MCP server"
        );

        Ok(summaries)
    }

    /// Fetch full tool definition from MCP server (Level 1: with schema).
    async fn fetch_tool_definition(
        tool_name: &str,
        peer: &mut Peer<RoleClient>,
        _server_name: &str,
    ) -> Result<ToolDefinition, McpError> {
        let result = peer.list_tools(None).await.map_err(|e| {
            McpError::ServerNotFound(format!("list_tools error: {}", e))
        })?;

        for t in &result.tools {
            if t.name == tool_name {
                let schema_map: Map<String, serde_json::Value> = t
                    .input_schema
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();
                return Ok(ToolDefinition {
                    name: t.name.to_string(),
                    description: t.description.to_string(),
                    input_schema: serde_json::Value::Object(schema_map),
                });
            }
        }

        Err(McpError::ServerNotFound(format!(
            "Tool '{}' not found",
            tool_name
        )))
    }
}

impl Default for ToolDiscovery {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_discovery_new() {
        let discovery = ToolDiscovery::new();
        assert!(discovery.summary_cache.try_read().is_ok());
        assert!(discovery.schema_cache.try_read().is_ok());
    }

    #[tokio::test]
    async fn test_clear_cache() {
        let discovery = ToolDiscovery::new();

        discovery.summary_cache.write().await.insert(
            "test-server".to_string(),
            vec![ToolSummary {
                name: "tool1".to_string(),
                description: "A test tool".to_string(),
            }],
        );
        discovery.schema_cache.write().await.insert(
            "tool1".to_string(),
            ToolDefinition {
                name: "tool1".to_string(),
                description: "A test tool".to_string(),
                input_schema: serde_json::json!({}),
            },
        );

        discovery.clear_cache().await;

        assert!(discovery.summary_cache.read().await.is_empty());
        assert!(discovery.schema_cache.read().await.is_empty());
    }

    #[tokio::test]
    async fn test_has_cached_definition() {
        let discovery = ToolDiscovery::new();

        assert!(!discovery.has_cached_definition("tool1").await);

        discovery.schema_cache.write().await.insert(
            "tool1".to_string(),
            ToolDefinition {
                name: "tool1".to_string(),
                description: "A test tool".to_string(),
                input_schema: serde_json::json!({}),
            },
        );

        assert!(discovery.has_cached_definition("tool1").await);
        assert!(!discovery.has_cached_definition("tool2").await);
    }

    #[tokio::test]
    async fn test_clear_individual_caches() {
        let discovery = ToolDiscovery::new();

        discovery.summary_cache.write().await.insert(
            "test-server".to_string(),
            vec![ToolSummary {
                name: "tool1".to_string(),
                description: "A test tool".to_string(),
            }],
        );
        discovery.schema_cache.write().await.insert(
            "tool1".to_string(),
            ToolDefinition {
                name: "tool1".to_string(),
                description: "A test tool".to_string(),
                input_schema: serde_json::json!({}),
            },
        );

        discovery.clear_summary_cache("test-server").await;
        assert!(discovery.summary_cache.read().await.is_empty());
        assert!(discovery.schema_cache.read().await.contains_key("tool1"));

        discovery.clear_schema_cache("tool1").await;
        assert!(discovery.schema_cache.read().await.is_empty());
    }
}
