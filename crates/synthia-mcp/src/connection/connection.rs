use std::sync::atomic::{AtomicU64, Ordering};

use chrono::{DateTime, Utc};
use rmcp::{
    service::{Peer, RoleClient, RunningService, ServiceExt},
    transport::TokioChildProcess,
};

use super::state::ConnectionState;
use crate::{
    discovery::ToolDefinition,
    server::McpClientService,
    types::{McpError, McpServerConfig},
};

pub struct McpConnection {
    pub server_id: String,
    pub config: McpServerConfig,
    pub tools: Vec<ToolDefinition>,
    pub connected_at: Option<DateTime<Utc>>,
    pub last_used_at: AtomicU64,
    pub last_ping_sent: AtomicU64,
    pub state: ConnectionState,
    pub is_dead: AtomicU64, // 1 = dead (heartbeat timeout), 0 = alive
    pub peer: Option<Peer<RoleClient>>,
    pub running_service: Option<RunningService<RoleClient, McpClientService>>,
}

impl McpConnection {
    pub fn new(
        server_id: String,
        config: McpServerConfig,
        tools: Vec<ToolDefinition>,
    ) -> Self {
        Self {
            server_id,
            config,
            tools,
            connected_at: None,
            last_used_at: AtomicU64::new(0),
            last_ping_sent: AtomicU64::new(0),
            state: ConnectionState::Discovered,
            is_dead: AtomicU64::new(0),
            peer: None,
            running_service: None,
        }
    }

    pub fn state(&self) -> ConnectionState {
        self.state
    }

    pub fn is_connected(&self) -> bool {
        self.state == ConnectionState::Connected
    }

    pub async fn connect(&mut self) -> Result<(), McpError> {
        if self.state == ConnectionState::Connected {
            return Ok(());
        }

        self.state = ConnectionState::Connecting;

        let result = self.establish_connection().await;

        match result {
            Ok(_) => {
                self.state = ConnectionState::Connected;
                self.connected_at = Some(Utc::now());
                self.update_last_used();
                tracing::info!(
                    server = %self.server_id,
                    tool_count = self.tools.len(),
                    "MCP connection established"
                );
                Ok(())
            }
            Err(e) => {
                self.state = ConnectionState::Error;
                tracing::error!(
                    server = %self.server_id,
                    error = %e,
                    "Failed to establish MCP connection"
                );
                Err(e)
            }
        }
    }

    async fn establish_connection(&mut self) -> Result<(), McpError> {
        use std::process::Stdio;

        let mut cmd = tokio::process::Command::new(&self.config.command);
        cmd.args(&self.config.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped());
        for (k, v) in &self.config.env {
            cmd.env(k, v);
        }

        let transport = TokioChildProcess::new(&mut cmd).map_err(|e| {
            McpError::ServerNotFound(format!(
                "Failed to create rmcp transport for '{}': {}",
                self.server_id, e
            ))
        })?;

        let service = McpClientService::default();
        let running_service = service.serve(transport).await.map_err(|e| {
            McpError::ServerNotFound(format!(
                "Failed to start rmcp service for '{}': {}",
                self.server_id, e
            ))
        })?;

        self.peer = Some(running_service.peer().clone());
        self.running_service = Some(running_service);
        Ok(())
    }

    pub async fn call_tool(
        &mut self,
        name: &str,
        input: serde_json::Value,
    ) -> Result<serde_json::Value, McpError> {
        if self.state != ConnectionState::Connected {
            self.connect().await?;
        }

        self.update_last_used();

        let peer = self.peer.as_mut().ok_or_else(|| {
            McpError::ServerNotFound(format!(
                "No peer for server '{}'",
                self.server_id
            ))
        })?;

        let response = crate::client::call_tool(peer, name, input).await?;

        // Extract content from CallToolResult - return the content as JSON
        let content = serde_json::to_value(&response.content).map_err(|e| {
            McpError::ServerNotFound(format!(
                "Failed to serialize content: {}",
                e
            ))
        })?;

        Ok(content)
    }

    pub async fn disconnect(&mut self) {
        self.state = ConnectionState::Idle;
        self.running_service = None;
        self.peer = None;
        self.connected_at = None;

        tracing::info!(
            server = %self.server_id,
            "MCP connection disconnected"
        );
    }

    pub async fn disconnect_graceful(&mut self) {
        if let Some(running_service) = self.running_service.take() {
            let _ = running_service.cancel().await;
        }
        self.disconnect().await;
    }

    pub fn update_last_used(&self) {
        let timestamp = Utc::now().timestamp().try_into().unwrap_or(0);
        self.last_used_at.store(timestamp, Ordering::SeqCst);
    }

    pub fn last_used_duration(&self) -> std::time::Duration {
        let last = self.last_used_at.load(Ordering::SeqCst);
        let now = Utc::now().timestamp() as u64;
        if last > now {
            std::time::Duration::from_secs(0)
        } else {
            std::time::Duration::from_secs(now - last)
        }
    }

    pub async fn refresh_tools(
        &mut self,
    ) -> Result<Vec<ToolDefinition>, McpError> {
        let peer = self.peer.as_mut().ok_or_else(|| {
            McpError::ServerNotFound(format!(
                "No peer for server '{}'",
                self.server_id
            ))
        })?;

        let response = crate::client::list_tools(peer).await?;

        let definitions: Vec<ToolDefinition> = response
            .tools
            .iter()
            .map(|t| ToolDefinition {
                name: t.name.to_string(),
                description: t.description.to_string(),
                input_schema: serde_json::Value::Object(
                    (*t.input_schema).clone(),
                ),
            })
            .collect();

        self.tools = definitions.clone();

        tracing::debug!(
            server = %self.server_id,
            count = definitions.len(),
            "Refreshed tool definitions"
        );

        Ok(definitions)
    }

    /// Check if connection is considered dead (heartbeat timeout)
    pub fn is_dead(&self) -> bool {
        self.is_dead.load(Ordering::SeqCst) == 1
    }

    /// Mark connection as dead (called when heartbeat times out)
    pub fn mark_dead(&self) {
        self.is_dead.store(1, Ordering::SeqCst);
    }

    pub fn get_tools(&self) -> &[ToolDefinition] {
        &self.tools
    }

    pub fn tools_mut(&mut self) -> &mut Vec<ToolDefinition> {
        &mut self.tools
    }
}

impl std::fmt::Debug for McpConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpConnection")
            .field("server_id", &self.server_id)
            .field("config", &self.config.name)
            .field("tool_count", &self.tools.len())
            .field("state", &self.state)
            .field("connected_at", &self.connected_at)
            .finish()
    }
}
