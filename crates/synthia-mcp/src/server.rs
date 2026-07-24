use std::{future::Future, time::Duration};

use rmcp::{
    Error as McpLibError,
    model::{ClientInfo, Implementation, ProtocolVersion},
    service::{Peer, RoleClient, RunningService, Service, ServiceExt},
    transport::TokioChildProcess,
};

use crate::types::{McpError, McpServerConfig};

#[derive(Debug, Clone)]
pub struct IdleTimeoutConfig {
    pub timeout: Duration,
    pub check_interval: Duration,
}

impl Default for IdleTimeoutConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(1800),
            check_interval: Duration::from_secs(60),
        }
    }
}

#[derive(Default)]
pub struct McpServer {
    active: std::sync::Mutex<bool>,
}

impl McpServer {
    pub fn new() -> Self {
        Self {
            active: std::sync::Mutex::new(false),
        }
    }

    pub async fn record_activity(&self, _server_name: &str) {}

    pub async fn start_idle_monitor(&self) {
        if let Ok(mut active) = self.active.lock() {
            *active = true;
        }
    }

    pub async fn stop_idle_monitor(&self) {
        if let Ok(mut active) = self.active.lock() {
            *active = false;
        }
    }
}

#[derive(Default, Clone)]
pub struct McpClientService {
    peer: Option<Peer<RoleClient>>,
}

impl Service<RoleClient> for McpClientService {
    #[allow(clippy::manual_async_fn)]
    fn handle_request(
        &self,
        _request: rmcp::model::ServerRequest,
        _context: rmcp::service::RequestContext<RoleClient>,
    ) -> impl Future<Output = Result<rmcp::model::ClientResult, McpLibError>>
    + Send
    + '_ {
        async {
            Ok(rmcp::model::ClientResult::EmptyResult(
                rmcp::model::EmptyResult {},
            ))
        }
    }

    #[allow(clippy::manual_async_fn)]
    fn handle_notification(
        &self,
        _notification: rmcp::model::ServerNotification,
    ) -> impl Future<Output = Result<(), McpLibError>> + Send + '_ {
        async { Ok(()) }
    }

    fn get_peer(&self) -> Option<Peer<RoleClient>> {
        self.peer.clone()
    }

    fn set_peer(&mut self, peer: Peer<RoleClient>) {
        self.peer = Some(peer);
    }

    fn get_info(&self) -> ClientInfo {
        ClientInfo {
            protocol_version: ProtocolVersion::default(),
            capabilities: rmcp::model::ClientCapabilities::default(),
            client_info: Implementation {
                name: "synthia-mcp".into(),
                version: "0.1.0".into(),
            },
        }
    }
}

pub async fn start_mcp_server(
    config: &McpServerConfig,
) -> Result<RunningService<RoleClient, McpClientService>, McpError> {
    use tokio::process::Command;

    let mut cmd = Command::new(&config.command);
    cmd.args(&config.args);

    let transport = TokioChildProcess::new(&mut cmd).map_err(|e| {
        McpError::ServerNotFound(format!(
            "Failed to create rmcp transport for '{}': {}",
            config.name, e
        ))
    })?;

    let service = McpClientService::default();

    let running_service =
        service
            .serve(transport)
            .await
            .map_err(|e: std::io::Error| {
                McpError::ServerNotFound(format!(
                    "Failed to start rmcp client service for '{}': {}",
                    config.name, e
                ))
            })?;

    tracing::info!(server = %config.name, "MCP server started via rmcp");
    Ok(running_service)
}

pub fn get_client_sink(
    service: &RunningService<RoleClient, McpClientService>,
) -> &Peer<RoleClient> {
    service.peer()
}
