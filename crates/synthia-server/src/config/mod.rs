//! Server configuration module
//!
//! Provides configuration types for the Synthia server.

mod agent;
mod mcp;
mod provider;
mod server;

pub use agent::{AgentConfig, SkillConfig};
pub use mcp::{
    DEFAULT_MCP_ENABLED,
    DEFAULT_MCP_SERVER_TYPE,
    DEFAULT_MCP_TIMEOUT,
    McpConfig,
};
pub use provider::{ModelConfig, ProviderConfig};
pub use server::{
    AuthConfig,
    DEFAULT_HOST,
    DEFAULT_MAX_AGENTS,
    DEFAULT_PORT,
    DEFAULT_VERSION,
    RateLimitConfig,
    ServerConfig,
};
