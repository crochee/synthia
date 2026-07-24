//! Server configuration types

use std::path::PathBuf;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use synthia_agent::config::AgentImplementation;

use super::{AgentConfig, McpConfig, ProviderConfig, SkillConfig};

pub const DEFAULT_HOST: &str = "127.0.0.1";
pub const DEFAULT_PORT: u16 = 8080;
pub const DEFAULT_VERSION: &str = "1.0";
pub const DEFAULT_MAX_AGENTS: usize = 5;

fn default_version() -> String {
    DEFAULT_VERSION.to_string()
}

fn default_host() -> String {
    DEFAULT_HOST.to_string()
}

fn default_port() -> u16 {
    DEFAULT_PORT
}

fn default_max_agents() -> usize {
    DEFAULT_MAX_AGENTS
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServerConfig {
    #[serde(default = "default_version")]
    pub version: String,
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default)]
    pub model_override: Option<String>,
    #[serde(default = "default_max_agents")]
    pub max_agents: usize,
    #[serde(default)]
    pub providers: std::collections::HashMap<String, ProviderConfig>,
    #[serde(default)]
    pub mcps: std::collections::HashMap<String, McpConfig>,
    #[serde(default)]
    pub agents: std::collections::HashMap<String, AgentConfig>,
    #[serde(default)]
    pub skills: Vec<SkillConfig>,
    #[serde(default)]
    pub auth: AuthConfig,
    #[serde(default)]
    pub rate_limit: RateLimitConfig,
    #[serde(default)]
    pub agent_implementation: AgentImplementation,
    #[serde(default)]
    pub cors: CorsConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct AuthConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub api_keys: Vec<String>,
    /// Optional per-API-key user_id mapping.
    ///
    /// Resolution order (see also
    /// `synthia_server::middleware::auth::resolve_user_id_from_key`):
    /// 1. If the request's API key is in `key_to_user`, use that
    ///    `user_id` verbatim.
    /// 2. Otherwise, if the key is in `api_keys` but unmapped, derive
    ///    `user_id = hex(sha256(key))[..16]` (deterministic, key-bound).
    /// 3. Otherwise, reject the request.
    ///
    /// An explicit map wins over derivation so that operators can pin
    /// stable namespaces regardless of key rotation.
    #[serde(default)]
    pub key_to_user: std::collections::HashMap<String, String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RateLimitConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_rate_limit_requests")]
    pub requests_per_minute: u32,
    #[serde(default = "default_rate_limit_burst")]
    pub burst: u32,
}

fn default_rate_limit_requests() -> u32 {
    60
}

fn default_rate_limit_burst() -> u32 {
    10
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            requests_per_minute: default_rate_limit_requests(),
            burst: default_rate_limit_burst(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CorsConfig {
    #[serde(default = "default_cors_enabled")]
    pub enabled: bool,
    #[serde(default = "default_allowed_origins")]
    pub allowed_origins: Vec<String>,
    #[serde(default = "default_allowed_methods")]
    pub allowed_methods: Vec<String>,
    #[serde(default = "default_allowed_headers")]
    pub allowed_headers: Vec<String>,
}

fn default_cors_enabled() -> bool {
    true
}

fn default_allowed_origins() -> Vec<String> {
    vec!["http://localhost:5173".to_string()]
}

fn default_allowed_methods() -> Vec<String> {
    vec![
        "GET".to_string(),
        "POST".to_string(),
        "PUT".to_string(),
        "DELETE".to_string(),
        "OPTIONS".to_string(),
    ]
}

fn default_allowed_headers() -> Vec<String> {
    vec!["Content-Type".to_string(), "Authorization".to_string()]
}

impl Default for CorsConfig {
    fn default() -> Self {
        Self {
            enabled: default_cors_enabled(),
            allowed_origins: default_allowed_origins(),
            allowed_methods: default_allowed_methods(),
            allowed_headers: default_allowed_headers(),
        }
    }
}

impl ServerConfig {
    pub fn get_agents(&self) -> Vec<AgentConfig> {
        self.agents
            .values()
            .map(|config| AgentConfig {
                description: config.description.clone(),
                model: config.model.clone(),
                max_steps: config.max_steps,
                allowed_tools: config.allowed_tools.clone(),
                denied_tools: config.denied_tools.clone(),
                hidden: config.hidden,
                color: config.color.clone(),
            })
            .collect()
    }

    pub fn load(path: &PathBuf) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }

        let content = std::fs::read_to_string(path)?;
        if path.extension().and_then(|e| e.to_str()) == Some("yaml") {
            serde_yaml::from_str(&content).map_err(Into::into)
        } else {
            serde_json::from_str(&content).map_err(Into::into)
        }
    }

    pub fn save(&self, path: &PathBuf) -> Result<()> {
        let content =
            if path.extension().and_then(|e| e.to_str()) == Some("yaml") {
                serde_yaml::to_string(self)?
            } else {
                serde_json::to_string_pretty(self)?
            };

        std::fs::write(path, content)?;
        Ok(())
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            version: default_version(),
            host: default_host(),
            port: default_port(),
            model_override: None,
            max_agents: default_max_agents(),
            providers: std::collections::HashMap::new(),
            mcps: std::collections::HashMap::new(),
            agents: std::collections::HashMap::new(),
            skills: Vec::new(),
            auth: AuthConfig::default(),
            rate_limit: RateLimitConfig::default(),
            agent_implementation: AgentImplementation::default(),
            cors: CorsConfig::default(),
        }
    }
}
