//! MCP server process management

use std::process::Stdio;

use anyhow::Result;
use rmcp::service::ServerSink;
use tokio::process::Command;
use tracing::{error, info};

use super::types::{McpServerConfig, McpServerStatus};

pub struct McpServerHandle {
    pub child: tokio::process::Child,
    pub server: Option<ServerSink>,
}

impl McpServerHandle {
    pub async fn stop(&mut self) -> Result<()> {
        self.child.kill().await?;
        self.server = None;
        Ok(())
    }
}

pub struct McpServer {
    pub config: McpServerConfig,
    pub status: McpServerStatus,
    handle: Option<McpServerHandle>,
    running: bool,
}

impl McpServer {
    pub fn new(config: McpServerConfig) -> Self {
        let status = McpServerStatus {
            name: config.name.clone(),
            status: "stopped".to_string(),
            description: config.description.clone(),
            tools: Vec::new(),
        };

        Self {
            config,
            status,
            handle: None,
            running: false,
        }
    }

    pub async fn start(&mut self) -> Result<()> {
        if !self.config.enabled {
            self.status.status = "disabled".to_string();
            return Ok(());
        }

        if self.is_running() {
            return Ok(());
        }

        let mut cmd = Command::new(&self.config.command);
        cmd.args(&self.config.args)
            .envs(&self.config.env)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        match cmd.spawn() {
            Ok(child) => {
                self.handle = Some(McpServerHandle {
                    child,
                    server: None,
                });
                self.running = true;
                self.status.status = "running".to_string();
                info!("Started MCP server '{}'", self.config.name);
            }
            Err(e) => {
                self.status.status = format!("error: {}", e);
                error!(
                    "Failed to start MCP server '{}': {}",
                    self.config.name, e
                );
                return Err(anyhow::anyhow!(
                    "Failed to start MCP server: {}",
                    e
                ));
            }
        }

        Ok(())
    }

    pub async fn stop(&mut self) -> Result<()> {
        if let Some(ref mut handle) = self.handle {
            handle.stop().await?;
        }
        self.handle = None;
        self.running = false;
        self.status.status = "stopped".to_string();
        info!("Stopped MCP server '{}'", self.config.name);
        Ok(())
    }

    pub fn is_running(&self) -> bool {
        self.running
    }

    pub fn health_check(&self) -> bool {
        self.running
    }

    pub async fn list_tools(&self) -> Result<Vec<rmcp::model::Tool>> {
        if let Some(ref handle) = self.handle
            && let Some(ref server) = handle.server
        {
            let tools = server.list_all_tools().await?;
            return Ok(tools);
        }
        Ok(Vec::new())
    }
}
