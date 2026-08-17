//! Server configuration types

use std::path::PathBuf;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::{AgentConfig, ProviderConfig, SkillConfig};

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
    pub agents: std::collections::HashMap<String, AgentConfig>,
    #[serde(default)]
    pub skills: Vec<SkillConfig>,
    #[serde(default)]
    pub auth: AuthConfig,
    /// Name of the default agent. When `None` the server falls
    /// back to the first agent registered in
    /// [`crate::state::AppState::agent_registry`].
    #[serde(default)]
    pub default_agent: Option<String>,
    #[serde(default)]
    pub rate_limit: RateLimitConfig,
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
    #[serde(default = "default_allowed_origins")]
    pub allowed_origins: Vec<String>,
    #[serde(default = "default_allowed_methods")]
    pub allowed_methods: Vec<String>,
    #[serde(default = "default_allowed_headers")]
    pub allowed_headers: Vec<String>,
}

fn default_allowed_origins() -> Vec<String> {
    // Empty list → CORS layer falls back to `Any` (permissive by default).
    // Operators can override via `cors.allowed_origins` in config to lock
    // down to a specific set of origins.
    Vec::new()
}

fn default_allowed_methods() -> Vec<String> {
    // Empty list → CORS layer falls back to `Any` (permissive by default).
    Vec::new()
}

fn default_allowed_headers() -> Vec<String> {
    // Empty list → CORS layer falls back to `Any` (permissive by default).
    Vec::new()
}

impl Default for CorsConfig {
    fn default() -> Self {
        Self {
            allowed_origins: default_allowed_origins(),
            allowed_methods: default_allowed_methods(),
            allowed_headers: default_allowed_headers(),
        }
    }
}

impl ServerConfig {
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
            agents: std::collections::HashMap::new(),
            skills: Vec::new(),
            auth: AuthConfig::default(),
            rate_limit: RateLimitConfig::default(),
            cors: CorsConfig::default(),
            default_agent: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- default_* helper functions ---------------------------------

    /// The 4 `default_*` helpers MUST return the documented
    /// constant values (so JSON-deserialized configs without
    /// those fields get the right defaults).
    #[test]
    fn default_helpers_return_pinned_values() {
        assert_eq!(default_version(), "1.0");
        assert_eq!(default_host(), "127.0.0.1");
        assert_eq!(default_port(), 8080);
        assert_eq!(default_max_agents(), 5);
        assert_eq!(default_rate_limit_requests(), 60);
        assert_eq!(default_rate_limit_burst(), 10);
    }

    /// `default_allowed_*` CORS helpers MUST return empty vecs
    /// (operators MUST explicitly opt into CORS restrictions).
    #[test]
    fn default_allowed_cors_helpers_return_empty_vecs() {
        assert!(default_allowed_origins().is_empty());
        assert!(default_allowed_methods().is_empty());
        assert!(default_allowed_headers().is_empty());
    }

    // -- ServerConfig::default --------------------------------------

    /// `ServerConfig::default()` MUST populate all 12 fields
    /// with documented defaults.
    #[test]
    fn server_config_default_all_twelve_fields() {
        let c = ServerConfig::default();
        assert_eq!(c.version, "1.0");
        assert_eq!(c.host, "127.0.0.1");
        assert_eq!(c.port, 8080);
        assert!(c.model_override.is_none());
        assert_eq!(c.max_agents, 5);
        assert!(c.providers.is_empty());
        assert!(c.agents.is_empty());
        assert!(c.skills.is_empty());
        assert!(!c.auth.enabled);
        assert!(!c.rate_limit.enabled);
        assert_eq!(c.rate_limit.requests_per_minute, 60);
        assert_eq!(c.rate_limit.burst, 10);
        assert!(c.cors.allowed_origins.is_empty());
        assert!(c.cors.allowed_methods.is_empty());
        assert!(c.cors.allowed_headers.is_empty());
        assert!(c.default_agent.is_none());
    }

    /// Two calls to `ServerConfig::default()` MUST produce equal
    /// values (deterministic, no shared state).
    #[test]
    fn server_config_default_is_deterministic() {
        let a = ServerConfig::default();
        let b = ServerConfig::default();
        // Pin all fields except collections (which are
        // independent anyway).
        assert_eq!(a.version, b.version);
        assert_eq!(a.host, b.host);
        assert_eq!(a.port, b.port);
        assert_eq!(a.model_override, b.model_override);
        assert_eq!(a.max_agents, b.max_agents);
        assert_eq!(a.skills.len(), b.skills.len());
        assert_eq!(a.default_agent, b.default_agent);
    }

    /// `ServerConfig` MUST derive `Debug + Clone` (used by
    /// server startup and config-reload paths).
    #[test]
    fn server_config_supports_debug_and_clone() {
        let c = ServerConfig::default();
        let _ = format!("{c:?}");
        let cloned = c.clone();
        assert_eq!(cloned.version, c.version);
        assert_eq!(cloned.port, c.port);
    }

    // -- ServerConfig::load edge cases ------------------------------

    /// `ServerConfig::load` for a non-existent path MUST return
    /// `Ok(ServerConfig::default())` (the documented
    /// first-run-convenience behavior).
    #[test]
    fn load_missing_file_returns_default() {
        let path = PathBuf::from("/none/a/expected/path/config.json");
        let c = ServerConfig::load(&path).expect("missing file must not error");
        assert_eq!(c.version, DEFAULT_VERSION);
        assert_eq!(c.host, DEFAULT_HOST);
        assert_eq!(c.port, DEFAULT_PORT);
        assert_eq!(c.max_agents, DEFAULT_MAX_AGENTS);
    }

    /// `ServerConfig::load` for a `.yaml` extension MUST parse
    /// as YAML and apply field overrides.
    #[test]
    fn load_yaml_file_with_field_override() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.yaml");
        std::fs::write(&path, "port: 9000\nmax_agents: 10\n").unwrap();
        let c = ServerConfig::load(&path).unwrap();
        assert_eq!(c.port, 9000);
        assert_eq!(c.max_agents, 10);
    }

    /// `ServerConfig::load` for a `.json` extension MUST parse
    /// as JSON.
    #[test]
    fn load_json_file_with_field_override() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        std::fs::write(&path, r#"{"port": 9001, "host": "10.0.0.1"}"#).unwrap();
        let c = ServerConfig::load(&path).unwrap();
        assert_eq!(c.port, 9001);
        assert_eq!(c.host, "10.0.0.1");
    }

    /// `ServerConfig::load` MUST return `Err` for malformed
    /// JSON.
    #[test]
    fn load_malformed_json_returns_err() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.json");
        std::fs::write(&path, "{ not json").unwrap();
        let result = ServerConfig::load(&path);
        assert!(result.is_err());
    }

    /// `ServerConfig::load` MUST return `Err` for malformed YAML.
    #[test]
    fn load_malformed_yaml_returns_err() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.yaml");
        std::fs::write(&path, ":bad:\n  :\n  : :").unwrap();
        let result = ServerConfig::load(&path);
        assert!(result.is_err());
    }

    /// `ServerConfig::load` MUST treat files without a
    /// `.yaml` extension as JSON.
    #[test]
    fn load_unknown_extension_treated_as_json() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.txt");
        // Write valid JSON — any non-yaml extension falls
        // through to JSON parsing.
        std::fs::write(&path, r#"{"port": 7777}"#).unwrap();
        let c = ServerConfig::load(&path).unwrap();
        assert_eq!(c.port, 7777);
    }

    // -- AuthConfig -------------------------------------------------

    /// `AuthConfig::default()` MUST have auth disabled, empty
    /// api_keys list, empty key_to_user map.
    #[test]
    fn auth_config_default_is_disabled() {
        let a = AuthConfig::default();
        assert!(!a.enabled);
        assert!(a.api_keys.is_empty());
        assert!(a.key_to_user.is_empty());
    }

    /// `AuthConfig` MUST round-trip through JSON with all 3
    /// fields.
    #[test]
    fn auth_config_round_trips_through_json() {
        let mut m = std::collections::HashMap::new();
        m.insert("key-1".to_string(), "user-a".to_string());
        let a = AuthConfig {
            enabled: true,
            api_keys: vec!["key-1".to_string(), "key-2".to_string()],
            key_to_user: m.clone(),
        };
        let json = serde_json::to_string(&a).unwrap();
        let parsed: AuthConfig = serde_json::from_str(&json).unwrap();
        assert!(parsed.enabled);
        assert_eq!(parsed.api_keys, vec!["key-1", "key-2"]);
        assert_eq!(
            parsed.key_to_user.get("key-1"),
            Some(&"user-a".to_string())
        );
    }

    /// `AuthConfig` MUST derive `Default` (used by
    /// `ServerConfig::default`).
    #[test]
    fn auth_config_supports_default_directly() {
        let _ = AuthConfig::default();
    }

    // -- RateLimitConfig --------------------------------------------

    /// `RateLimitConfig::default()` MUST have rate limit
    /// disabled but populate `requests_per_minute = 60` and
    /// `burst = 10`.
    #[test]
    fn rate_limit_config_default_values() {
        let r = RateLimitConfig::default();
        assert!(!r.enabled);
        assert_eq!(r.requests_per_minute, 60);
        assert_eq!(r.burst, 10);
    }

    /// `RateLimitConfig` MUST round-trip through JSON with all 3
    /// fields populated.
    #[test]
    fn rate_limit_config_round_trips_through_json() {
        let r = RateLimitConfig {
            enabled: true,
            requests_per_minute: 120,
            burst: 30,
        };
        let json = serde_json::to_string(&r).unwrap();
        let parsed: RateLimitConfig = serde_json::from_str(&json).unwrap();
        assert!(parsed.enabled);
        assert_eq!(parsed.requests_per_minute, 120);
        assert_eq!(parsed.burst, 30);
    }

    /// `RateLimitConfig` serde MUST apply defaults (60/10) when
    /// fields are omitted (so the `enabled` flag alone works).
    #[test]
    fn rate_limit_config_serde_defaults_apply() {
        let r: RateLimitConfig =
            serde_json::from_str(r#"{"enabled": true}"#).unwrap();
        assert!(r.enabled);
        assert_eq!(r.requests_per_minute, 60);
        assert_eq!(r.burst, 10);
    }

    // -- CorsConfig -------------------------------------------------

    /// `CorsConfig::default()` MUST have all 3 lists empty
    /// (operators must explicitly opt into CORS restrictions).
    #[test]
    fn cors_config_default_all_lists_empty() {
        let c = CorsConfig::default();
        assert!(c.allowed_origins.is_empty());
        assert!(c.allowed_methods.is_empty());
        assert!(c.allowed_headers.is_empty());
    }

    /// `CorsConfig` MUST round-trip through JSON with all 3
    /// lists populated.
    #[test]
    fn cors_config_round_trips_through_json() {
        let c = CorsConfig {
            allowed_origins: vec!["https://app.example".to_string()],
            allowed_methods: vec!["GET".to_string(), "POST".to_string()],
            allowed_headers: vec!["Authorization".to_string()],
        };
        let json = serde_json::to_string(&c).unwrap();
        let parsed: CorsConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(
            parsed.allowed_origins,
            vec!["https://app.example".to_string()]
        );
        assert_eq!(
            parsed.allowed_methods,
            vec!["GET".to_string(), "POST".to_string()]
        );
        assert_eq!(parsed.allowed_headers, vec!["Authorization".to_string()]);
    }

    /// `CorsConfig` serde MUST apply defaults (empty lists) when
    /// fields are omitted.
    #[test]
    fn cors_config_serde_defaults_apply() {
        let c: CorsConfig = serde_json::from_str("{}").unwrap();
        assert!(c.allowed_origins.is_empty());
        assert!(c.allowed_methods.is_empty());
        assert!(c.allowed_headers.is_empty());
    }
}
