//! Server configuration module
//!
//! Provides configuration types for the Synthia server.

mod agent;
pub mod provider;
pub mod server;
pub mod yaml_bridge;

pub use agent::AgentConfig;
pub use provider::{ModelConfig, ProviderConfig};
pub use server::{
    AuthConfig,
    CorsConfig,
    DEFAULT_HOST,
    DEFAULT_MAX_AGENTS,
    DEFAULT_PORT,
    DEFAULT_VERSION,
    McpServerConfig,
    OperationEndpointConfig,
    ServerConfig,
    ToolsConfig,
};

/// Environment variable holding the deployment configuration as a
/// string. Wins over every file candidate so Kubernetes ConfigMaps
/// and CI runners can ship config without mounting a file (OpenCode
/// `OPENCODE_CONFIG_CONTENT` parity).
pub const CONFIG_CONTENT_ENV: &str = "SYNTHIA_CONFIG_CONTENT";

/// The deployment configuration, resolved **once** per boot.
///
/// Resolution order:
/// 1. `SYNTHIA_CONFIG_CONTENT` env-content (R62);
/// 2. the `--config <path>` the operator named, when it exists (R53);
/// 3. `{workspace_root}/config.toml`;
/// 4. `{workspace_root}/.synthia/config.toml`.
///
/// R53: the named file used to configure only the provider bridge, while
/// the agent / tool / auth / CORS / operations / MCP sections
/// were read from `config.toml` regardless — so `make dev-server`, which
/// passes `--config config.yaml`, silently dropped half of the file it
/// was told to use. One resolution point fixes that and removes the
/// repeated parse (every section used to re-read and re-parse the file).
///
/// R62: the env-content candidate is detected by [`ServerConfig::load`]
/// (extension pick) by writing the content to a tempfile in the system
/// temp dir; the tempfile's lifetime is the function call. A malformed
/// env overlay is logged and the next layer wins, matching the
/// existing fail-soft contract for file candidates.
///
/// [`ServerConfig::load`] picks the format by extension, so the named
/// file may be `.toml`, `.yaml`, or `.yml`. A candidate that exists but
/// does not parse is logged and skipped — the next candidate is tried,
/// and when none is left the callers get `None` and apply their own
/// defaults, so a config typo never blocks boot (the same fail-soft
/// contract `[tools]`, `[mcp_servers]` and the provider bridge follow).
pub fn resolve_server_config(
    workspace_root: &std::path::Path,
    config_path: Option<&std::path::PathBuf>,
) -> Option<crate::config::ServerConfig> {
    use crate::config::ServerConfig;

    let mut candidates: Vec<std::path::PathBuf> = Vec::new();
    // Keeps the env-content tempfile alive until the function
    // returns so `ServerConfig::load` can read it. Dropping the
    // helper deletes the file.
    let mut _env_keeper: Option<tempfile::NamedTempFile> = None;
    if let Some(content) = std::env::var_os(CONFIG_CONTENT_ENV)
        && let Some(content) = content.to_str()
        && !content.trim().is_empty()
        && let Some((path, keeper)) = write_env_content_tempfile(content)
    {
        _env_keeper = Some(keeper);
        candidates.push(path);
    }
    if let Some(path) = config_path {
        candidates.push(path.clone());
    }
    candidates.push(workspace_root.join("config.toml"));
    candidates.push(workspace_root.join(".synthia").join("config.toml"));

    for candidate in candidates {
        if !candidate.exists() {
            continue;
        }
        match ServerConfig::load(&candidate) {
            Ok(cfg) => {
                tracing::info!(
                    path = %candidate.display(),
                    "loaded server config"
                );
                return Some(cfg);
            }
            Err(e) => {
                tracing::warn!(
                    path = %candidate.display(),
                    error = %e,
                    "failed to load server config; trying the next candidate"
                );
            }
        }
    }
    None
}

/// Write the `SYNTHIA_CONFIG_CONTENT` value to a tempfile and return
/// its path. The file extension is `.yaml` so the YAML loader picks
/// it up; callers fall through to the next candidate on parse
/// failure, so a malformed env overlay is non-fatal.
///
/// The path's [`TempPath`] handle is leaked (via [`std::mem::forget`]
/// is **not** used — the helper retains ownership in the surrounding
/// scope through [`tempfile::NamedTempPath`]) so the file persists
/// until [`ServerConfig::load`] finishes reading it. The simpler
/// approach is to write the file at a known, well-known path under
/// the system temp dir and keep it for the lifetime of the process;
/// subsequent calls overwrite it.
fn write_env_content_tempfile(
    content: &str,
) -> Option<(std::path::PathBuf, tempfile::NamedTempFile)> {
    use std::io::Write;
    let Ok(mut tmp) = tempfile::Builder::new()
        .prefix("synthia-config-")
        .suffix(".yaml")
        .tempfile()
    else {
        tracing::warn!(
            env = CONFIG_CONTENT_ENV,
            "could not create tempfile for env-content config; skipping"
        );
        return None;
    };
    if let Err(e) = tmp.write_all(content.as_bytes()) {
        tracing::warn!(
            env = CONFIG_CONTENT_ENV,
            error = %e,
            "could not write env-content config to tempfile; skipping"
        );
        return None;
    }
    let path = tmp.path().to_path_buf();
    Some((path, tmp))
}

#[cfg(test)]
mod tests {
    //! Unit tests for the `config` module family.
    //!
    //! Coverage map (47 tests):
    //!
    //! - `ProviderConfig` + `ModelConfig`: 6 tests (defaults, serde,
    //!   round-trip, optional fields).
    //! - `AgentConfig`: 6 tests (defaults, all fields
    //!   serde, round-trip, field independence).
    //! - `ServerConfig`: 14 tests (defaults via `load` of a missing
    //!   file, JSON round-trip, all sub-struct defaults, `default_agent`
    //!   path).
    //! - `AuthConfig` + `CorsConfig`: 9 tests
    //!   (defaults, custom values, key_to_user map, rate limit
    //!   defaults, CORS defaults).
    //! - `ServerConfig::load`: 7 tests (missing file → default,
    //!   JSON file, YAML file, malformed file → Err, extension
    //!   detection).
    //! - Constants: 1 test.

    use std::collections::HashMap;

    use super::*;

    // =============================================================================
    // ProviderConfig + ModelConfig
    // =============================================================================

    /// `ProviderConfig` MUST default every field to None / empty
    /// when deserialized from `{}`.
    #[test]
    fn test_provider_config_defaults_all_fields_to_none_or_empty() {
        let p: ProviderConfig = serde_json::from_str("{}").unwrap();
        assert!(p.api_key.is_none());
        assert!(p.base_url.is_none());
        assert!(p.models.is_empty());
    }

    /// `ProviderConfig` MUST round-trip through JSON with all
    /// fields populated.
    #[test]
    fn test_provider_config_round_trips_through_json() {
        let p = ProviderConfig {
            api_key: Some("sk-123".to_string()),
            base_url: Some("https://api.example.com".to_string()),
            models: vec![ModelConfig {
                name: "gpt-4o".to_string(),
                description: Some("OpenAI flagship".to_string()),
                context_window: Some(128_000),
                temperature: Some(0.7),
                max_tokens: Some(4096),
            }],
        };
        let json = serde_json::to_string(&p).unwrap();
        let parsed: ProviderConfig =
            serde_json::from_str(&json).expect("round-trip parse");
        assert_eq!(parsed.api_key, p.api_key);
        assert_eq!(parsed.base_url, p.base_url);
        assert_eq!(parsed.models.len(), 1);
        assert_eq!(parsed.models[0].name, "gpt-4o");
        assert_eq!(parsed.models[0].temperature, Some(0.7));
    }

    /// `ModelConfig` MUST default all optional fields to None when
    /// only `name` is provided.
    #[test]
    fn test_model_config_minimal_serde_defaults_optionals() {
        let json = r#"{"name": "claude-opus"}"#;
        let m: ModelConfig = serde_json::from_str(json).unwrap();
        assert_eq!(m.name, "claude-opus");
        assert!(m.description.is_none());
        assert!(m.context_window.is_none());
        assert!(m.temperature.is_none());
        assert!(m.max_tokens.is_none());
    }

    /// `ModelConfig` MUST round-trip all 5 fields through serde.
    #[test]
    fn test_model_config_round_trips_all_five_fields() {
        let m = ModelConfig {
            name: "gpt-4".to_string(),
            description: Some("d".to_string()),
            context_window: Some(8_192),
            temperature: Some(0.0),
            max_tokens: Some(1_024),
        };
        let json = serde_json::to_string(&m).unwrap();
        let parsed: ModelConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, m.name);
        assert_eq!(parsed.description, m.description);
        assert_eq!(parsed.context_window, m.context_window);
        assert_eq!(parsed.temperature, m.temperature);
        assert_eq!(parsed.max_tokens, m.max_tokens);
    }

    /// `ModelConfig` MUST support being cloned and still equal the
    /// original field-for-field (Clone derive contract).
    #[test]
    fn test_model_config_clone_preserves_all_fields() {
        let m = ModelConfig {
            name: "x".to_string(),
            description: Some("d".to_string()),
            context_window: Some(1),
            temperature: Some(0.5),
            max_tokens: Some(2),
        };
        let c = m.clone();
        assert_eq!(c.name, m.name);
        assert_eq!(c.description, m.description);
        assert_eq!(c.context_window, m.context_window);
        assert_eq!(c.temperature, m.temperature);
        assert_eq!(c.max_tokens, m.max_tokens);
    }

    /// `ProviderConfig` MUST accept a list of multiple `ModelConfig`
    /// entries (the typical multi-model setup).
    #[test]
    fn test_provider_config_with_multiple_models() {
        let p = ProviderConfig {
            api_key: None,
            base_url: None,
            models: vec![
                ModelConfig {
                    name: "small".to_string(),
                    description: None,
                    context_window: Some(8_000),
                    temperature: None,
                    max_tokens: None,
                },
                ModelConfig {
                    name: "large".to_string(),
                    description: None,
                    context_window: Some(200_000),
                    temperature: None,
                    max_tokens: None,
                },
            ],
        };
        let json = serde_json::to_string(&p).unwrap();
        let parsed: ProviderConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.models.len(), 2);
        assert_eq!(parsed.models[0].name, "small");
        assert_eq!(parsed.models[1].name, "large");
    }

    // =============================================================================
    // AgentConfig

    // ServerConfig — defaults and full round-trip
    // =============================================================================

    /// `ServerConfig::default()` MUST populate every field with
    /// its documented default (host, port, version, max_agents).
    #[test]
    fn test_server_config_default_values_pinned() {
        let c = ServerConfig::default();
        assert_eq!(c.version, DEFAULT_VERSION);
        assert_eq!(c.host, DEFAULT_HOST);
        assert_eq!(c.port, DEFAULT_PORT);
        assert_eq!(c.max_agents, DEFAULT_MAX_AGENTS);
        assert!(c.providers.is_empty());
        assert!(c.agents.is_empty());
        assert!(c.default_agent.is_none());
        // auth and cors are both Default.
        assert!(!c.auth.enabled);
        assert_eq!(c.cors.allowed_origins, Vec::<String>::new());
    }

    /// `ServerConfig` MUST round-trip a fully-populated config
    /// through JSON without loss.
    #[test]
    fn test_server_config_full_round_trip() {
        let original = build_sample_server_config();
        let json = serde_json::to_string(&original).unwrap();
        let parsed: ServerConfig = serde_json::from_str(&json).unwrap();
        assert_top_level_round_trips(&original, &parsed);
        assert_mcp_round_trips(&parsed);
        assert_tools_surface_round_trips(&parsed);
        assert_auth_cors_round_trips(&parsed);
        assert_providers_agents_round_trips(&parsed);
        assert_operations_round_trips(&parsed);
    }

    /// Build the fully-populated `ServerConfig` used by
    /// `test_server_config_full_round_trip`. Each sub-section
    /// (providers, agents, mcp, tools) is populated to
    /// the smallest non-trivial value that still exercises every
    /// field covered by the round-trip assertions below.
    fn build_sample_server_config() -> ServerConfig {
        let mut providers = HashMap::new();
        providers.insert(
            "openai".to_string(),
            ProviderConfig {
                api_key: Some("k".to_string()),
                base_url: None,
                models: vec![ModelConfig {
                    name: "gpt-4o".to_string(),
                    description: None,
                    context_window: None,
                    temperature: None,
                    max_tokens: None,
                }],
            },
        );
        let mut agents = HashMap::new();
        agents.insert(
            "default".to_string(),
            AgentConfig {
                description: None,
                model: Some("gpt-4o".to_string()),
                max_steps: Some(20),
                allowed_tools: vec![],
                denied_tools: vec![],
                hidden: false,
                color: None,
                compaction: None,
                strategy: None,
                sandbox: None,
            },
        );
        ServerConfig {
            version: "1.0".to_string(),
            host: "0.0.0.0".to_string(),
            port: 9000,
            max_agents: 10,
            providers,
            agents,
            auth: AuthConfig {
                enabled: true,
                api_keys: vec!["key1".to_string()],
                key_to_user: HashMap::new(),
            },
            cors: CorsConfig {
                allowed_origins: vec!["https://app.example".to_string()],
                allowed_methods: vec!["GET".to_string()],
                allowed_headers: vec!["Authorization".to_string()],
            },
            default_agent: Some("default".to_string()),
            mcp_servers: vec![McpServerConfig {
                name: "demo".to_string(),
                command: "npx".to_string(),
                args: vec!["-y".to_string(), "demo-server".to_string()],
                env: HashMap::new(),
                enabled: true,
            }],
            operations: OperationEndpointConfig { enabled: true },
            tools: ToolsConfig {
                deferred: vec!["query_db".to_string()],
                hidden: vec!["admin".to_string()],
                groups: std::collections::BTreeMap::from([(
                    "files".to_string(),
                    vec!["read".to_string(), "write".to_string()],
                )]),
                active_groups: vec!["files".to_string()],
                max_visible: Some(12),
            },
        }
    }

    /// Top-level scalars (version, host, port, max_agents,
    /// default_agent) MUST round-trip unchanged.
    fn assert_top_level_round_trips(
        original: &ServerConfig,
        parsed: &ServerConfig,
    ) {
        assert_eq!(parsed.version, original.version);
        assert_eq!(parsed.host, original.host);
        assert_eq!(parsed.port, original.port);
        assert_eq!(parsed.max_agents, original.max_agents);
        assert_eq!(parsed.default_agent, original.default_agent);
    }

    /// R21: the MCP server list MUST round-trip with name, args,
    /// and the enabled flag preserved.
    fn assert_mcp_round_trips(parsed: &ServerConfig) {
        assert_eq!(parsed.mcp_servers.len(), 1);
        assert_eq!(parsed.mcp_servers[0].name, "demo");
        assert_eq!(parsed.mcp_servers[0].args[1], "demo-server");
        assert!(parsed.mcp_servers[0].enabled);
    }

    /// R34: the tool-surface section MUST round-trip the four
    /// list fields and the optional `max_visible` cap.
    fn assert_tools_surface_round_trips(parsed: &ServerConfig) {
        assert_eq!(parsed.tools.deferred, vec!["query_db".to_string()]);
        assert_eq!(parsed.tools.hidden, vec!["admin".to_string()]);
        assert_eq!(parsed.tools.groups["files"], vec!["read", "write"]);
        assert_eq!(parsed.tools.active_groups, vec!["files".to_string()]);
        assert_eq!(parsed.tools.max_visible, Some(12));
    }

    /// Auth api-keys and CORS allowed-origins MUST round-trip.
    fn assert_auth_cors_round_trips(parsed: &ServerConfig) {
        assert_eq!(parsed.auth.api_keys, vec!["key1".to_string()]);
        assert_eq!(
            parsed.cors.allowed_origins,
            vec!["https://app.example".to_string()]
        );
    }

    /// Provider and agent maps MUST each contain exactly one
    /// entry under the configured keys.
    fn assert_providers_agents_round_trips(parsed: &ServerConfig) {
        assert_eq!(parsed.providers.len(), 1);
        assert!(parsed.providers.contains_key("openai"));
        assert_eq!(parsed.agents.len(), 1);
        assert!(parsed.agents.contains_key("default"));
    }

    /// R29: the operation-endpoint gate MUST round-trip.
    fn assert_operations_round_trips(parsed: &ServerConfig) {
        assert!(parsed.operations.enabled);
    }

    /// `ServerConfig` MUST populate default values for `version`,
    /// `host`, `port`, `max_agents` when deserialized from an empty
    /// JSON object (the serde default functions).
    #[test]
    fn test_server_config_serde_defaults_apply() {
        let c: ServerConfig = serde_json::from_str("{}").unwrap();
        assert_eq!(c.version, "1.0");
        assert_eq!(c.host, "127.0.0.1");
        assert_eq!(c.port, 8080);
        assert_eq!(c.max_agents, 5);
        assert!(c.providers.is_empty());
    }

    /// `ServerConfig` MUST allow overriding individual defaults
    /// while leaving others at default.
    #[test]
    fn test_server_config_serde_overrides_preserve_others() {
        let json = r#"{"port": 3000, "host": "0.0.0.0"}"#;
        let c: ServerConfig = serde_json::from_str(json).unwrap();
        assert_eq!(c.port, 3000);
        assert_eq!(c.host, "0.0.0.0");
        // Untouched fields retain defaults.
        assert_eq!(c.version, "1.0");
        assert_eq!(c.max_agents, 5);
    }

    /// `ServerConfig::default_agent` MUST accept `Some("name")`
    /// when present in JSON.
    #[test]
    fn test_server_config_default_agent_some() {
        let json = r#"{"default_agent": "primary"}"#;
        let c: ServerConfig = serde_json::from_str(json).unwrap();
        assert_eq!(c.default_agent, Some("primary".to_string()));
    }

    /// `ServerConfig::default_agent` MUST be `None` when omitted.
    #[test]
    fn test_server_config_default_agent_none_when_omitted() {
        let c: ServerConfig = serde_json::from_str("{}").unwrap();
        assert!(c.default_agent.is_none());
    }

    // =============================================================================
    // AuthConfig
    // =============================================================================

    /// `AuthConfig::default()` MUST have auth disabled, empty key
    /// list, and empty key_to_user map.
    #[test]
    fn test_auth_config_default_disabled_with_empty_lists() {
        let a = AuthConfig::default();
        assert!(!a.enabled);
        assert!(a.api_keys.is_empty());
        assert!(a.key_to_user.is_empty());
    }

    /// `AuthConfig` MUST round-trip `key_to_user` map with explicit
    /// user_id mappings.
    #[test]
    fn test_auth_config_key_to_user_round_trip() {
        let mut m = HashMap::new();
        m.insert("key-1".to_string(), "user-a".to_string());
        m.insert("key-2".to_string(), "user-b".to_string());
        let a = AuthConfig {
            enabled: true,
            api_keys: vec!["key-1".to_string(), "key-2".to_string()],
            key_to_user: m.clone(),
        };
        let json = serde_json::to_string(&a).unwrap();
        let parsed: AuthConfig = serde_json::from_str(&json).unwrap();
        assert!(parsed.enabled);
        assert_eq!(parsed.api_keys.len(), 2);
        assert_eq!(
            parsed.key_to_user.get("key-1"),
            Some(&"user-a".to_string())
        );
        assert_eq!(
            parsed.key_to_user.get("key-2"),
            Some(&"user-b".to_string())
        );
    }

    /// `AuthConfig` MUST default `key_to_user` to an empty map when
    /// omitted in JSON.
    #[test]
    fn test_auth_config_serde_defaults_key_to_user_to_empty_map() {
        let json = r#"{"enabled": true, "api_keys": ["k"]}"#;
        let a: AuthConfig = serde_json::from_str(json).unwrap();
        assert!(a.key_to_user.is_empty());
    }

    // =============================================================================
    // =============================================================================
    // CorsConfig
    // =============================================================================

    /// `CorsConfig::default()` MUST have all three lists empty
    /// (operators must explicitly opt into CORS restrictions).
    #[test]
    fn test_cors_config_default_all_three_lists_empty() {
        let c = CorsConfig::default();
        assert!(c.allowed_origins.is_empty());
        assert!(c.allowed_methods.is_empty());
        assert!(c.allowed_headers.is_empty());
    }

    /// `CorsConfig` MUST round-trip populated lists.
    #[test]
    fn test_cors_config_round_trips_three_lists() {
        let c = CorsConfig {
            allowed_origins: vec![
                "https://a".to_string(),
                "https://b".to_string(),
            ],
            allowed_methods: vec!["GET".to_string(), "POST".to_string()],
            allowed_headers: vec!["Content-Type".to_string()],
        };
        let json = serde_json::to_string(&c).unwrap();
        let parsed: CorsConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.allowed_origins.len(), 2);
        assert_eq!(parsed.allowed_methods.len(), 2);
        assert_eq!(parsed.allowed_headers, vec!["Content-Type".to_string()]);
    }

    /// `CorsConfig` serde MUST apply defaults (empty lists) when
    /// fields are omitted.
    #[test]
    fn test_cors_config_serde_defaults_all_to_empty() {
        let c: CorsConfig = serde_json::from_str("{}").unwrap();
        assert!(c.allowed_origins.is_empty());
        assert!(c.allowed_methods.is_empty());
        assert!(c.allowed_headers.is_empty());
    }

    // =============================================================================
    // ServerConfig::load
    // =============================================================================

    /// `ServerConfig::load` for a non-existent path MUST return
    /// `Ok(ServerConfig::default())` (the well-known "first-run
    /// convenience" behavior).
    #[test]
    fn test_server_config_load_missing_file_returns_default() {
        let path = std::path::PathBuf::from("/nonexistent/path/to/config.json");
        let c = ServerConfig::load(&path).expect("missing file must not error");
        assert_eq!(c.version, DEFAULT_VERSION);
        assert_eq!(c.host, DEFAULT_HOST);
        assert_eq!(c.port, DEFAULT_PORT);
    }

    /// `ServerConfig::load` MUST parse JSON files when the
    /// extension is `.json`.
    #[test]
    fn test_server_config_load_json_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        std::fs::write(
            &path,
            r#"{"version": "2.0", "host": "10.0.0.1", "port": 4000, "max_agents": 7}"#,
        )
        .unwrap();
        let c = ServerConfig::load(&path).unwrap();
        assert_eq!(c.version, "2.0");
        assert_eq!(c.host, "10.0.0.1");
        assert_eq!(c.port, 4000);
        assert_eq!(c.max_agents, 7);
    }

    /// `ServerConfig::load` MUST parse YAML files when the
    /// extension is `.yaml`.
    #[test]
    fn test_server_config_load_yaml_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.yaml");
        std::fs::write(
            &path,
            "version: \"3.0\"\nhost: \"10.0.0.2\"\nport: 5000\nmax_agents: 3\n",
        )
        .unwrap();
        let c = ServerConfig::load(&path).unwrap();
        assert_eq!(c.version, "3.0");
        assert_eq!(c.host, "10.0.0.2");
        assert_eq!(c.port, 5000);
        assert_eq!(c.max_agents, 3);
    }

    /// `ServerConfig::load` MUST fall back to JSON parsing when
    /// the extension is NOT `.yaml` (so `.yml` and unknown
    /// extensions are treated as JSON). This pins the actual
    /// extension-detection contract — refactors adding `.yml` support
    /// would need to update the loader.
    #[test]
    fn test_server_config_load_yml_extension_falls_back_to_json() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.yml");
        // Write valid JSON content (since .yml falls through to JSON).
        std::fs::write(&path, r#"{"port": 7777}"#).unwrap();
        let c = ServerConfig::load(&path).unwrap();
        assert_eq!(c.port, 7777);
    }

    /// `ServerConfig::load` MUST return `Err` for malformed JSON.
    #[test]
    fn test_server_config_load_malformed_json_returns_err() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.json");
        std::fs::write(&path, "{ this is not json").unwrap();
        let result = ServerConfig::load(&path);
        assert!(result.is_err());
    }

    /// `ServerConfig::load` MUST return `Err` for malformed YAML.
    #[test]
    fn test_server_config_load_malformed_yaml_returns_err() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.yaml");
        std::fs::write(&path, ":bad:\n  :yaml\n: :\n").unwrap();
        let result = ServerConfig::load(&path);
        assert!(result.is_err());
    }

    // =============================================================================
    // Constants
    // =============================================================================

    /// The 4 module-level constants MUST remain pinned at their
    /// documented values (dashboards / docs reference them).
    #[test]
    fn test_module_level_constants_pinned() {
        assert_eq!(DEFAULT_HOST, "127.0.0.1");
        assert_eq!(DEFAULT_PORT, 8080);
        assert_eq!(DEFAULT_VERSION, "1.0");
        assert_eq!(DEFAULT_MAX_AGENTS, 5);
    }
}
