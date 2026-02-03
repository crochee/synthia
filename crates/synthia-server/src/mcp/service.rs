//! MCP service for MCP server management logic

use std::{collections::HashMap, sync::Arc};

use anyhow::Result;
use dashmap::DashMap;
use tracing::{error, info, warn};

use super::{
    server::McpServer,
    types::{McpServerConfig, McpServerStatus},
};
use crate::error::ServerError;

#[derive(Clone)]
pub struct McpService {
    servers: Arc<DashMap<String, McpServer>>,
}

impl McpService {
    pub fn new() -> Self {
        Self {
            servers: Arc::new(DashMap::new()),
        }
    }

    pub async fn register_server(
        &self,
        config: McpServerConfig,
    ) -> Result<McpServerStatus> {
        let name = config.name.clone();

        if self.servers.contains_key(&name) {
            return Err(anyhow::anyhow!(
                "MCP server '{}' already registered",
                name
            ));
        }

        let server = McpServer::new(config);
        let status = server.status.clone();
        self.servers.insert(name, server);

        info!("Registered MCP server: {}", status.name);
        Ok(status)
    }

    pub async fn unregister_server(&self, name: &str) -> Result<()> {
        if let Some((_, mut server)) = self.servers.remove(name) {
            server.stop().await?;
            info!("Unregistered MCP server: {}", name);
        }
        Ok(())
    }

    pub async fn start_server(&self, name: &str) -> Result<()> {
        if let Some(mut server) = self.servers.get_mut(name) {
            server.start().await?;
        } else {
            return Err(anyhow::anyhow!("MCP server '{}' not found", name));
        }
        Ok(())
    }

    pub async fn stop_server(&self, name: &str) -> Result<()> {
        if let Some(mut server) = self.servers.get_mut(name) {
            server.stop().await?;
        }
        Ok(())
    }

    pub fn list(&self) -> Vec<McpServerStatus> {
        self.servers
            .iter()
            .map(|r| r.value().status.clone())
            .collect()
    }

    pub fn get(&self, name: &str) -> Option<McpServerStatus> {
        self.servers.get(name).map(|r| r.value().status.clone())
    }

    pub async fn list_server_tools(
        &self,
        name: &str,
    ) -> Result<Vec<rmcp::model::Tool>> {
        if let Some(server) = self.servers.get(name) {
            server.list_tools().await
        } else {
            Err(anyhow::anyhow!("MCP server '{}' not found", name))
        }
    }

    pub async fn health_check_all(&self) -> HashMap<String, bool> {
        let mut results = HashMap::new();
        let names: Vec<String> =
            self.servers.iter().map(|r| r.key().clone()).collect();

        for name in names {
            if let Some(server) = self.servers.get_mut(&name) {
                results.insert(name, server.health_check());
            }
        }
        results
    }

    pub async fn start_all(&self) -> Result<()> {
        let names: Vec<String> =
            self.servers.iter().map(|r| r.key().clone()).collect();

        for name in names {
            if let Err(e) = self.start_server(&name).await {
                warn!("Failed to start MCP server '{}': {}", name, e);
            }
        }

        Ok(())
    }

    pub async fn stop_all(&self) -> Result<()> {
        let names: Vec<String> =
            self.servers.iter().map(|r| r.key().clone()).collect();

        for name in names {
            if let Err(e) = self.stop_server(&name).await {
                error!("Failed to stop MCP server '{}': {}", name, e);
            }
        }

        Ok(())
    }

    pub async fn register(
        &self,
        config: McpServerConfig,
    ) -> Result<McpServerStatus, ServerError> {
        self.register_server(config)
            .await
            .map_err(|e| ServerError::McpError(e.to_string()))
    }

    pub async fn unregister(&self, name: &str) -> Result<(), ServerError> {
        self.unregister_server(name)
            .await
            .map_err(|e| ServerError::McpError(e.to_string()))
    }

    pub async fn start(&self, name: &str) -> Result<(), ServerError> {
        self.start_server(name)
            .await
            .map_err(|e| ServerError::McpError(e.to_string()))
    }

    pub async fn stop(&self, name: &str) -> Result<(), ServerError> {
        self.stop_server(name)
            .await
            .map_err(|e| ServerError::McpError(e.to_string()))
    }

    pub async fn list_tools(
        &self,
        name: &str,
    ) -> Result<Vec<rmcp::model::Tool>, ServerError> {
        self.list_server_tools(name)
            .await
            .map_err(|e| ServerError::McpError(e.to_string()))
    }
}

impl Default for McpService {
    fn default() -> Self {
        Self::new()
    }
}
