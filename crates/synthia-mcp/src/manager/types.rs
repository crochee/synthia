//! Type definitions for the MCP manager.
//!
//! [`ServerConnection`] tracks per-server runtime state (status, tool
//! discovery, optional running service, optional hybrid connection),
//! and [`McpManager`] holds the cross-server state (connection table,
//! config registry, idle-timeout tracking, credential store, hybrid
//! mode flag, and discovered-tools cache).
//!
//! Field visibility is `pub(super)` so the impl blocks scattered across
//! the [`construct`], [`lifecycle`], [`config`], [`idle`],
//! [`tool_call`], and [`hybrid`] submodules can manipulate state
//! directly while keeping the external surface narrow.

use std::{collections::HashMap, sync::Arc, time::Duration};

use rmcp::service::{RoleClient, RunningService};
use tokio::sync::RwLock;

use crate::{
    connection::McpConnection,
    discovery::{ToolDefinition, ToolDiscovery},
    oauth::CredentialStore,
    server::IdleTimeoutConfig,
    types::{ConnectionStatus, McpServerConfig},
};

/// Tracks the state of a single MCP server connection.
pub struct ServerConnection {
    pub status: ConnectionStatus,
    pub discovery: Arc<ToolDiscovery>,
    pub running_service:
        Option<RunningService<RoleClient, crate::server::McpClientService>>,
    pub hybrid_connection: Option<McpConnection>,
}

/// Lifecycle-managed MCP server: lazy start, idle timeout recycling, cleanup on exit.
pub struct McpManager {
    /// Active server connections keyed by server name.
    pub connections: RwLock<HashMap<String, ServerConnection>>,
    /// Server configs that have been registered but not yet started.
    pub(super) configs: RwLock<HashMap<String, McpServerConfig>>,
    /// Last activity timestamps for idle timeout tracking.
    pub(super) last_activity: RwLock<HashMap<String, std::time::Instant>>,
    /// Idle timeout configuration for auto-recycling.
    pub(super) idle_config: IdleTimeoutConfig,
    /// Credential store for OAuth token persistence across connections.
    pub(super) credential_store: Arc<CredentialStore>,
    /// Whether hybrid mode is enabled (discover without connecting).
    pub hybrid_mode_enabled: bool,
    /// Idle timeout for hybrid mode connections (default 5 minutes).
    pub idle_timeout: Duration,
    /// Cleanup interval for idle connections (default 1 minute).
    pub cleanup_interval: Duration,
    /// Discovered tools cache for hybrid mode (server_id -> tools).
    pub(super) discovered_tools: RwLock<HashMap<String, Vec<ToolDefinition>>>,
}
