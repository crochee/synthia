//! Synthia Server - HTTP API server for Synthia Agent
//!
//! Provides a REST API for interacting with the Synthia agent,
//! suitable for frontend applications, editor plugins, and TUI clients.

use anyhow::Result;
use axum::Router;
use tracing::info;

mod agent;
mod auth;
mod chat;
pub mod config;
mod error;
mod mcp;
mod model;
mod routes;
mod session;
mod skill;
mod state;
mod tool;
mod utils;
mod ws;

pub use config::{
    AuthConfig,
    DEFAULT_HOST,
    DEFAULT_MAX_AGENTS,
    DEFAULT_PORT,
    DEFAULT_VERSION,
    McpConfig,
    ModelConfig,
    ProviderConfig,
    RateLimitConfig,
    ServerConfig,
    SkillConfig,
};
pub use error::{
    ApiError,
    ApiResponse,
    EmptyResponse,
    PagedResponse,
    ServerError,
};
pub use mcp::{McpServerConfig, McpServerStatus, McpService};
pub use model::ModelInfo;
pub use session::{CompactionResult, FormattedMessage, SessionInfo};
pub use skill::{AddSkillRequest, SkillInfo, SkillLoadResult};
pub use state::{AppState, ServerEventHandler};
pub use tool::{
    ToolAnnotations,
    ToolExecuteRequest,
    ToolExecuteResponse,
    ToolInfo,
};

pub use self::agent::{build_agent, create_subagent_tool, register_tools};

pub async fn run_server(
    state: AppState,
    cancel_token: tokio_util::sync::CancellationToken,
) -> Result<()> {
    let addr = format!("{}:{}", state.config_host, state.config_port);
    let app: Router = routes::build_routes(state);

    info!("Starting Synthia server on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;

    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            cancel_token.cancelled().await;
        })
        .await?;

    Ok(())
}
