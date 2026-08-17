//! Server configuration module
//!
//! Provides configuration types for the Synthia server.

mod agent;
pub mod provider;
pub mod server;
pub mod yaml_bridge;

#[cfg(test)]
mod tests;

pub use agent::{AgentConfig, SkillConfig};
pub use provider::{ModelConfig, ProviderConfig};
pub use server::{
    AuthConfig,
    CorsConfig,
    DEFAULT_HOST,
    DEFAULT_MAX_AGENTS,
    DEFAULT_PORT,
    DEFAULT_VERSION,
    RateLimitConfig,
    ServerConfig,
};
