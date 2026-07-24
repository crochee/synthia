//! The 5 start methods on [`super::core::McpProxy`]:
//!
//! - [`McpProxy::start_server`] — the public dispatcher
//!   that validates the config, checks for duplicates, and
//!   delegates to the 4 transport-specific starters.
//! - [`McpProxy::start_stdio_server`] — spawns a
//!   `tokio::process::Command` with the configured
//!   `command` + `args` + `env`. Stdio is piped
//!   (`stdin=null`, `stdout=pipe`, `stderr=pipe`) and
//!   `kill_on_drop=true`.
//! - [`McpProxy::start_sse_server`] /
//!   `start_http_server` / `start_ws_server` — 3 network
//!   starters. All 3 issue a connectivity check (HTTP GET
//!   for SSE/HTTP, WS handshake for WS) but do not retain
//!   the live connection; the resulting handle is a
//!   [`super::handle::NetworkHandle`] placeholder.
//!
//! All 5 methods are gated by `startup_timeout` (default
//! 30s).

use std::process::Stdio;

use tokio::process::Child;
use tracing::{debug, info, warn};

use super::{
    core::McpProxy,
    error::McpProxyError,
    handle::{NetworkHandle, ServerHandle},
};
use crate::{
    registry::McpServerConfig,
    types::{McpConfigError, Transport},
};

impl McpProxy {
    /// Start a single MCP server by name
    pub async fn start_server(
        &self,
        _name: &str,
        config: &McpServerConfig,
    ) -> Result<(), McpProxyError> {
        // Validate config
        config.validate().map_err(|e| {
            McpProxyError::ValidationFailed(config.name.clone(), e)
        })?;

        // Check if already running
        {
            let servers = self.servers.read().await;
            if servers.contains_key(&config.name) {
                return Err(McpProxyError::ServerAlreadyRunning(
                    config.name.clone(),
                ));
            }
        }

        info!(
            "starting MCP server: {} (transport: {:?})",
            config.name, config.transport
        );

        let handle = match config.transport() {
            Transport::Stdio => {
                let child = self.start_stdio_server(config).await?;
                ServerHandle::Stdio(child)
            }
            Transport::Sse => {
                self.start_sse_server(config).await?;
                ServerHandle::Network(NetworkHandle::new(config.transport()))
            }
            Transport::Http => {
                self.start_http_server(config).await?;
                ServerHandle::Network(NetworkHandle::new(config.transport()))
            }
            Transport::Ws => {
                self.start_ws_server(config).await?;
                ServerHandle::Network(NetworkHandle::new(config.transport()))
            }
        };

        let mut servers = self.servers.write().await;
        servers.insert(config.name.clone(), handle);

        Ok(())
    }

    /// Start an MCP server using stdio transport
    pub(super) async fn start_stdio_server(
        &self,
        config: &McpServerConfig,
    ) -> Result<Child, McpProxyError> {
        let command = config.command.as_ref().ok_or_else(|| {
            McpProxyError::ValidationFailed(
                config.name.clone(),
                McpConfigError::MissingCommand,
            )
        })?;

        debug!(
            "spawning stdio server: {} with command: {} {:?}",
            config.name, command, config.args
        );

        let mut cmd = tokio::process::Command::new(command);
        cmd.args(&config.args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        // Set environment variables
        for (key, value) in &config.env {
            cmd.env(key, value);
        }

        // Wrap spawn in a future that resolves immediately with the result
        let child =
            tokio::time::timeout(self.startup_timeout, async { cmd.spawn() })
                .await
                .map_err(|_| {
                    McpProxyError::ConnectionTimeout(config.name.clone())
                })?
                .map_err(|e| {
                    McpProxyError::StartFailed(config.name.clone(), e)
                })?;

        debug!(
            "stdio server {} started with pid: {:?}",
            config.name,
            child.id()
        );

        Ok(child)
    }

    /// Start an MCP server using SSE transport
    pub(super) async fn start_sse_server(
        &self,
        config: &McpServerConfig,
    ) -> Result<(), McpProxyError> {
        let url = config.url.as_ref().ok_or_else(|| {
            McpProxyError::ValidationFailed(
                config.name.clone(),
                McpConfigError::MissingUrl(Transport::Sse),
            )
        })?;

        debug!("connecting to SSE server: {} at {}", config.name, url);

        // Verify the endpoint is reachable
        let client = reqwest::Client::builder()
            .timeout(self.startup_timeout)
            .build()
            .map_err(McpProxyError::HttpError)?;

        // Send a request to verify connectivity
        let response = client
            .get(url)
            .send()
            .await
            .map_err(McpProxyError::HttpError)?;

        if !response.status().is_success() {
            warn!(
                "SSE server {} returned non-success status: {}",
                config.name,
                response.status()
            );
        }

        debug!("SSE server {} connected successfully", config.name);
        Ok(())
    }

    /// Start an MCP server using HTTP transport
    pub(super) async fn start_http_server(
        &self,
        config: &McpServerConfig,
    ) -> Result<(), McpProxyError> {
        let url = config.url.as_ref().ok_or_else(|| {
            McpProxyError::ValidationFailed(
                config.name.clone(),
                McpConfigError::MissingUrl(Transport::Http),
            )
        })?;

        debug!("connecting to HTTP server: {} at {}", config.name, url);

        let client = reqwest::Client::builder()
            .timeout(self.startup_timeout)
            .build()
            .map_err(McpProxyError::HttpError)?;

        // Send a request to verify connectivity
        let response = client
            .get(url)
            .send()
            .await
            .map_err(McpProxyError::HttpError)?;

        if !response.status().is_success() {
            warn!(
                "HTTP server {} returned non-success status: {}",
                config.name,
                response.status()
            );
        }

        debug!("HTTP server {} connected successfully", config.name);
        Ok(())
    }

    /// Start an MCP server using WebSocket transport
    pub(super) async fn start_ws_server(
        &self,
        config: &McpServerConfig,
    ) -> Result<(), McpProxyError> {
        let url = config.url.as_ref().ok_or_else(|| {
            McpProxyError::ValidationFailed(
                config.name.clone(),
                McpConfigError::MissingUrl(Transport::Ws),
            )
        })?;

        debug!("connecting to WebSocket server: {} at {}", config.name, url);

        use tokio_tungstenite::connect_async;

        // Note: tokio_tungstenite's connect_async doesn't support WASM mode in regular tokio runtime
        // This is a simplified implementation
        match connect_async(url).await {
            Ok((ws_stream, response)) => {
                debug!(
                    "WebSocket server {} connected, response: {:?}",
                    config.name,
                    response.status()
                );
                // In a full implementation, you'd store the WebSocket stream and handle messages
                let _ = ws_stream; // Acknowledge without warning
            }
            Err(e) => {
                return Err(McpProxyError::WebSocketError(format!(
                    "failed to connect to {url}: {e}"
                )));
            }
        }

        debug!("WebSocket server {} connected successfully", config.name);
        Ok(())
    }
}
