//! Application state types

use std::{path::PathBuf, sync::Arc};

use synthia_agent::{
    Agent,
    event_handler::AgentEventHandler,
    session::SessionManager,
    tools::ToolRegistry,
};

use crate::{config::ServerConfig, mcp::McpService};

#[derive(Clone)]
pub struct AppState {
    pub agent: Agent,
    pub session_manager: Arc<dyn SessionManager>,
    pub tool_registry: Arc<ToolRegistry>,
    pub current_dir: PathBuf,
    pub mcp_module: McpService,
    pub config: Arc<tokio::sync::RwLock<ServerConfig>>,
    pub config_path: PathBuf,
    pub config_host: String,
    pub config_port: u16,
}

#[derive(Clone, Default)]
pub struct ServerEventHandler;

#[async_trait::async_trait]
impl AgentEventHandler for ServerEventHandler {
    async fn on_event(
        &self,
        _agent_name: &str,
        _event: &synthia_agent::types::AgentEvent,
    ) {
    }
}
