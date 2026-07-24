//! The [`McpRegistry`] struct + `Default` + `Clone` impls +
//! the 18 main methods (2 ctors + 16 service methods).
//!
//! The `Registry<McpServerInfo>` trait impl lives in
//! [`super::registry_trait`]; the
//! `LifecycleRegistry<McpServerInfo>` trait impl lives in
//! [`super::lifecycle_trait`].

use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use synthia_tool_orchestrator::{DynamicResolver, adapter::ToolAdapter};

use super::types::{McpFilter, McpServerInfo};
use crate::{
    discovery::ToolDefinition,
    manager::McpManager,
    types::{McpError, McpServerConfig, ToolSummary},
};

pub struct McpRegistry {
    pub(super) configs: RwLock<HashMap<String, McpServerConfig>>,
    pub(super) schema_cache: RwLock<HashMap<String, serde_json::Value>>,
    pub(super) servers: RwLock<HashMap<String, McpServerInfo>>,
    pub(super) manager: Option<Arc<McpManager>>,
}

impl McpRegistry {
    pub fn new() -> Self {
        Self {
            configs: RwLock::new(HashMap::new()),
            schema_cache: RwLock::new(HashMap::new()),
            servers: RwLock::new(HashMap::new()),
            manager: None,
        }
    }

    pub fn with_manager(manager: Arc<McpManager>) -> Self {
        Self {
            configs: RwLock::new(HashMap::new()),
            schema_cache: RwLock::new(HashMap::new()),
            servers: RwLock::new(HashMap::new()),
            manager: Some(manager),
        }
    }

    pub async fn add_config(&self, config: McpServerConfig) {
        let name = config.name.clone();
        if let Some(ref manager) = self.manager {
            manager.register_config(config.clone()).await;
        }
        self.configs
            .write()
            .expect("RwLock poisoned")
            .insert(name, config);
    }

    pub async fn discover_tools(
        &self,
        server_name: &str,
    ) -> Result<Vec<ToolSummary>, McpError> {
        if let Some(ref manager) = self.manager {
            return manager.discover_tools_for_server(server_name).await;
        }
        Ok(vec![ToolSummary {
            name: format!("{}_tool", server_name),
            description: format!("Tool from {}", server_name),
        }])
    }

    pub async fn get_tool_metadata(
        &self,
        server_name: &str,
    ) -> Vec<ToolSummary> {
        if let Some(ref manager) = self.manager
            && let Some(discovery) = manager.get_discovery(server_name).await
        {
            return discovery.list_summaries().await;
        }
        self.configs
            .read()
            .expect("RwLock poisoned")
            .get(server_name)
            .map(|config| {
                vec![ToolSummary {
                    name: format!("{}_tool", config.name),
                    description: format!("MCP tool from {}", config.name),
                }]
            })
            .unwrap_or_default()
    }

    pub async fn discover_all_tools(
        &self,
    ) -> Result<HashMap<String, Vec<ToolSummary>>, McpError> {
        if let Some(ref manager) = self.manager {
            return manager.discover_tools().await;
        }
        let configs = self.configs.read().expect("RwLock poisoned").clone();
        let mut result = HashMap::new();
        for (name, _config) in configs.iter() {
            let tools = self.discover_tools(name).await?;
            result.insert(name.clone(), tools);
        }
        Ok(result)
    }

    pub async fn register_tools_to_registry(
        &self,
        tool_registry: &synthia_tool::registry::ToolRegistry,
    ) -> Result<(), McpError> {
        let all_tools = self.discover_all_tools().await?;
        let manager = self.manager.clone().ok_or_else(|| {
            McpError::ServerNotFound("No manager configured".to_string())
        })?;
        for (server_name, tools) in all_tools {
            for tool_summary in tools {
                let tool_definition = ToolDefinition {
                    name: tool_summary.name.clone(),
                    description: tool_summary.description.clone(),
                    input_schema: serde_json::json!({}),
                };
                let adapter =
                    Arc::new(crate::tool_adapter::McpToolAdapter::new(
                        server_name.clone(),
                        tool_definition,
                        manager.clone(),
                    ));
                tool_registry.register(synthia_tool::ToolEntry::new(adapter));
            }
        }
        Ok(())
    }

    /// Discover tools from a single server and register them into both the
    /// static [`ToolRegistry`] and the dynamic orchestrator resolver.
    pub async fn register_tools_for_server(
        &self,
        server_name: &str,
        tool_registry: &synthia_tool::registry::ToolRegistry,
        dynamic_resolver: &DynamicResolver,
    ) -> Result<Vec<String>, McpError> {
        let tools = self.discover_tools(server_name).await?;
        let manager = self.manager.clone().ok_or_else(|| {
            McpError::ServerNotFound("No manager configured".to_string())
        })?;

        let mut registered = Vec::with_capacity(tools.len());
        for tool_summary in tools {
            let tool_definition = ToolDefinition {
                name: tool_summary.name.clone(),
                description: tool_summary.description.clone(),
                input_schema: serde_json::json!({}),
            };
            let adapter = Arc::new(crate::tool_adapter::McpToolAdapter::new(
                server_name.to_string(),
                tool_definition,
                manager.clone(),
            ));

            tool_registry
                .register(synthia_tool::ToolEntry::new(adapter.clone()));
            dynamic_resolver.register(
                tool_summary.name.clone(),
                Arc::new(ToolAdapter::new(adapter)),
            );
            registered.push(tool_summary.name);
        }
        Ok(registered)
    }

    pub async fn get_tool_schema(
        &self,
        server_id: &str,
        tool_name: &str,
    ) -> Result<serde_json::Value, McpError> {
        let cache_key = format!("{}:{}", server_id, tool_name);
        {
            let cache = self.schema_cache.read().expect("RwLock poisoned");
            if let Some(schema) = cache.get(&cache_key) {
                return Ok(schema.clone());
            }
        }

        let schema = serde_json::json!({});
        self.schema_cache
            .write()
            .expect("RwLock poisoned")
            .insert(cache_key, schema.clone());
        Ok(schema)
    }

    pub async fn cache_tool_schema(
        &self,
        server_id: &str,
        tool_name: &str,
        schema: serde_json::Value,
    ) {
        let cache_key = format!("{}:{}", server_id, tool_name);
        self.schema_cache
            .write()
            .expect("RwLock poisoned")
            .insert(cache_key, schema);
    }

    pub async fn clear_schema_cache(&self) {
        self.schema_cache.write().expect("RwLock poisoned").clear();
    }

    pub async fn get_config(&self, name: &str) -> Option<McpServerConfig> {
        self.configs
            .read()
            .expect("RwLock poisoned")
            .get(name)
            .cloned()
    }

    pub async fn remove_config(&self, name: &str) -> bool {
        self.configs
            .write()
            .expect("RwLock poisoned")
            .remove(name)
            .is_some()
    }

    pub async fn list_configs(&self) -> Vec<McpServerConfig> {
        self.configs
            .read()
            .expect("RwLock poisoned")
            .values()
            .cloned()
            .collect()
    }

    pub(super) fn filter_servers(
        &self,
        filter: Option<McpFilter>,
    ) -> Vec<McpServerInfo> {
        let servers = self.servers.read().expect("RwLock poisoned");
        let filter = filter.unwrap_or_default();

        let mut result: Vec<_> = servers
            .values()
            .filter(|server| {
                if filter.enabled_only && !server.enabled {
                    return false;
                }
                if let Some(ref transport_type) = filter.transport_type {
                    let server_transport = if server.command.contains("npx")
                        || server.command.contains("node")
                    {
                        "stdio"
                    } else if server.command.starts_with("http")
                        || server.command.starts_with("https")
                    {
                        "http"
                    } else {
                        "unknown"
                    };
                    if !server_transport
                        .to_lowercase()
                        .contains(&transport_type.to_lowercase())
                    {
                        return false;
                    }
                }
                true
            })
            .cloned()
            .collect();

        result.sort_by(|a, b| a.name.cmp(&b.name));
        result
    }

    pub fn contains(&self, name: &str) -> bool {
        self.servers
            .read()
            .expect("RwLock poisoned")
            .contains_key(name)
    }

    pub fn len(&self) -> usize {
        self.servers.read().expect("RwLock poisoned").len()
    }

    pub fn is_empty(&self) -> bool {
        self.servers.read().expect("RwLock poisoned").is_empty()
    }
}

impl Default for McpRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for McpRegistry {
    fn clone(&self) -> Self {
        let configs = self.configs.read().expect("RwLock poisoned").clone();
        let schema_cache =
            self.schema_cache.read().expect("RwLock poisoned").clone();
        let servers = self.servers.read().expect("RwLock poisoned").clone();
        Self {
            configs: RwLock::new(configs),
            schema_cache: RwLock::new(schema_cache),
            servers: RwLock::new(servers),
            manager: self.manager.clone(),
        }
    }
}
