//! Server configuration types

use std::path::PathBuf;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::{AgentConfig, ProviderConfig};

pub const DEFAULT_HOST: &str = "127.0.0.1";
pub const DEFAULT_PORT: u16 = 8080;
pub const DEFAULT_VERSION: &str = "1.0";
pub const DEFAULT_MAX_AGENTS: usize = 5;

/// Where the server binds, after CLI / env / config / defaults.
///
/// R54: this existed only as two `clap` defaults (`127.0.0.1:8080`),
/// while the Dockerfile and both compose files set
/// `SYNTHIA_HOST=0.0.0.0` and `SYNTHIA_PORT=8080` — so a containerized
/// server bound loopback and its published port was unreachable, and
/// `host` / `port` in the deployment config were read by nobody.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindAddress {
    pub host: String,
    pub port: u16,
}

impl BindAddress {
    /// Resolve the address, most specific source first:
    ///
    /// 1. `cli` — the value `clap` produced, which already merged
    ///    `--host` / `--port` with `SYNTHIA_HOST` / `SYNTHIA_PORT`;
    /// 2. the deployment config's `host` / `port`;
    /// 3. [`DEFAULT_HOST`] / [`DEFAULT_PORT`].
    ///
    /// Each part resolves independently, so `--port 9000` with
    /// `host = "0.0.0.0"` in the config means exactly that.
    #[must_use]
    pub fn resolve(cli: BindOverrides, config: Option<&ServerConfig>) -> Self {
        let host = cli
            .host
            .or_else(|| config.map(|c| c.host.clone()))
            .unwrap_or_else(|| DEFAULT_HOST.to_string());
        let port = cli
            .port
            .or_else(|| config.map(|c| c.port))
            .unwrap_or(DEFAULT_PORT);
        Self { host, port }
    }
}

/// The command line's contribution to [`BindAddress::resolve`].
///
/// `Option` rather than a default-filled value on purpose: a default
/// would be indistinguishable from an explicit `--host 127.0.0.1`, and
/// the config file could then never win.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BindOverrides {
    pub host: Option<String>,
    pub port: Option<u16>,
}

impl std::fmt::Display for BindAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.host, self.port)
    }
}

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
    /// Cap on registered agents. `POST /api/v1/agents` refuses a
    /// registration once the registry holds this many, so a public
    /// deployment cannot grow its agent set without bound. The default
    /// member counts, so `max_agents = 5` leaves room for four
    /// registered ones.
    #[serde(default = "default_max_agents")]
    pub max_agents: usize,
    #[serde(default)]
    pub providers: std::collections::HashMap<String, ProviderConfig>,
    #[serde(default)]
    pub agents: std::collections::HashMap<String, AgentConfig>,
    #[serde(default)]
    pub auth: AuthConfig,
    /// Name of the default agent. When `None` the server falls
    /// back to the first agent registered in
    /// [`crate::state::AppState::agent_registry`].
    #[serde(default)]
    pub default_agent: Option<String>,
    #[serde(default)]
    pub cors: CorsConfig,
    /// R21: MCP servers to spawn at boot. Each server's tools
    /// join the local tool registry, so remote tools flow
    /// through the same guard pipeline, dispatch, and metrics
    /// as builtins. A server that fails to start is logged and
    /// skipped — it never blocks boot.
    #[serde(default)]
    pub mcp_servers: Vec<McpServerConfig>,
    /// R29: the optional `POST .../operation` discriminated-union
    /// endpoint. Off by default — the route is not registered at
    /// all when disabled, so the path falls through to the
    /// standard 404 envelope.
    #[serde(default)]
    pub operations: OperationEndpointConfig,
    /// R34: the deployment's model-facing tool surface. Applied to
    /// the live tool registry at boot; a name the registry does not
    /// have is logged and skipped, never a startup failure. See
    /// [`ToolsConfig`].
    #[serde(default)]
    pub tools: ToolsConfig,
}

/// The `[tools]` section (R34): which registered tools the model is
/// told about, and in what form.
///
/// The section is applied to the live registry at boot — after the
/// builtins, the `skill` tool, and any configured MCP server's tools
/// have registered — so it can name remote tools too. Every name is
/// resolved against that registry; an unknown name is **skipped with
/// a warning** rather than failing startup, because an MCP server
/// that fails to connect must not take the server down. The effective
/// surface is logged at boot and exposed as
/// [`crate::state::AppState::tool_surface`].
///
/// Write order is `groups` → `deferred` → `hidden`, so when one tool
/// appears in several places the later entry wins. The knobs:
///
/// | key | effect |
/// |---|---|
/// | `groups` + `active_groups` | active-group member → advertised; inactive-group member → not advertised but still callable |
/// | `deferred` | advertised by name and description only until its first call in the transcript |
/// | `hidden` | not advertised **and** refused by the registry's dispatch (the privacy flag) |
/// | `max_visible` | caps the agent's model-facing list in registry order; not a registry write, so the HTTP catalog endpoint shows the uncapped list |
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ToolsConfig {
    /// Tools advertised by name only until the transcript shows a
    /// call ([`synthia::tool::ToolExposure::Deferred`]).
    #[serde(default)]
    pub deferred: Vec<String>,
    /// Tools hidden from every listing *and* refused on dispatch
    /// ([`synthia::tool::ToolRegistry::set_hidden`]).
    #[serde(default)]
    pub hidden: Vec<String>,
    /// Named groups: group name → member tool names. A member of an
    /// inactive group keeps its registration but is not advertised.
    #[serde(default)]
    pub groups: std::collections::BTreeMap<String, Vec<String>>,
    /// Groups whose members are advertised.
    #[serde(default)]
    pub active_groups: Vec<String>,
    /// Cap on how many tools one agent request advertises, applied in
    /// registry (name) order. `None` advertises the whole visible
    /// catalog.
    #[serde(default)]
    pub max_visible: Option<usize>,
}

/// The `POST .../operation` endpoint gate (R29).
///
/// One flag, one endpoint. Kept as a struct (rather than a bare
/// `bool` on `ServerConfig`) so future operation-level knobs
/// land here without another top-level field — the same shape
/// `cors` already uses.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct OperationEndpointConfig {
    /// Register `POST /api/v1/chat/sessions/{id}/operation`.
    /// Default `false`.
    #[serde(default)]
    pub enabled: bool,
}
/// One MCP server to launch over stdio (R21).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct McpServerConfig {
    /// Display name (used in logs and the tool's presentation
    /// hint).
    pub name: String,
    /// Executable to spawn.
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    /// Extra environment variables for the child process.
    #[serde(default)]
    pub env: std::collections::HashMap<String, String>,
    /// Set `false` to keep the entry in config without spawning.
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

impl McpServerConfig {
    /// Convert to the transport spawn config.
    #[must_use]
    pub fn to_stdio_config(&self) -> synthia::mcp::StdioConfig {
        let mut config = synthia::mcp::StdioConfig::new(&self.command)
            .args(self.args.clone())
            .label(self.name.clone());
        for (key, value) in &self.env {
            config = config.env(key.clone(), value.clone());
        }
        config
    }
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
            max_agents: default_max_agents(),
            providers: std::collections::HashMap::new(),
            agents: std::collections::HashMap::new(),
            auth: AuthConfig::default(),
            cors: CorsConfig::default(),
            default_agent: None,
            mcp_servers: Vec::new(),
            operations: OperationEndpointConfig::default(),
            tools: ToolsConfig::default(),
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
    }

    // -- BindAddress (R54) -------------------------------------------

    /// The CLI wins over the config, and the config wins over the
    /// defaults — the precedence a deployment expects.
    #[test]
    fn bind_address_resolution_precedence() {
        let config = ServerConfig {
            host: "0.0.0.0".to_string(),
            port: 9000,
            ..ServerConfig::default()
        };

        // CLI/env (clap merges them into the same field) beats config.
        assert_eq!(
            BindAddress::resolve(
                BindOverrides {
                    host: Some("10.0.0.1".to_string()),
                    port: Some(7000),
                },
                Some(&config),
            ),
            BindAddress {
                host: "10.0.0.1".to_string(),
                port: 7000
            }
        );

        // Config beats the defaults.
        assert_eq!(
            BindAddress::resolve(BindOverrides::default(), Some(&config)),
            BindAddress {
                host: "0.0.0.0".to_string(),
                port: 9000
            }
        );

        // Nothing anywhere → the documented defaults.
        assert_eq!(
            BindAddress::resolve(BindOverrides::default(), None),
            BindAddress {
                host: DEFAULT_HOST.to_string(),
                port: DEFAULT_PORT
            }
        );
    }

    /// Each part resolves on its own: `--port` does not disturb the
    /// config's `host`, and vice versa. (A tuple-shaped resolver that
    /// took "the CLI's value" would lose one of the two.)
    #[test]
    fn bind_address_resolves_host_and_port_independently() {
        let config = ServerConfig {
            host: "0.0.0.0".to_string(),
            port: 9000,
            ..ServerConfig::default()
        };
        let address = BindAddress::resolve(
            BindOverrides {
                host: None,
                port: Some(7000),
            },
            Some(&config),
        );
        assert_eq!(address.host, "0.0.0.0", "host comes from the config");
        assert_eq!(address.port, 7000, "port comes from the CLI");
        assert_eq!(address.to_string(), "0.0.0.0:7000");
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

    /// `ServerConfig::default()` MUST populate every field with its
    /// documented default.
    #[test]
    fn server_config_default_fills_every_field() {
        let c = ServerConfig::default();
        assert_eq!(c.version, "1.0");
        assert_eq!(c.host, "127.0.0.1");
        assert_eq!(c.port, 8080);
        assert_eq!(c.max_agents, 5);
        assert!(c.providers.is_empty());
        assert!(c.agents.is_empty());
        assert!(!c.auth.enabled);
        assert!(c.cors.allowed_origins.is_empty());
        assert!(c.cors.allowed_methods.is_empty());
        assert!(c.cors.allowed_headers.is_empty());
        assert!(c.default_agent.is_none());
        assert!(c.tools.deferred.is_empty());
        assert!(c.tools.hidden.is_empty());
        assert!(c.tools.groups.is_empty());
        assert!(c.tools.active_groups.is_empty());
        assert!(c.tools.max_visible.is_none());
    }

    /// The `[tools]` section deserializes with its documented shape,
    /// and an omitted section is an empty surface (the no-op applied
    /// at boot).
    #[test]
    fn tools_section_deserializes_and_defaults_empty() {
        let c: ServerConfig = serde_json::from_str(
            r#"{"tools": {
                "deferred": ["query_db"],
                "hidden": ["admin"],
                "groups": {"files": ["read", "write"]},
                "active_groups": ["files"],
                "max_visible": 7
            }}"#,
        )
        .unwrap();
        assert_eq!(c.tools.deferred, vec!["query_db"]);
        assert_eq!(c.tools.hidden, vec!["admin"]);
        assert_eq!(c.tools.groups["files"], vec!["read", "write"]);
        assert_eq!(c.tools.active_groups, vec!["files"]);
        assert_eq!(c.tools.max_visible, Some(7));

        let empty: ServerConfig = serde_json::from_str("{}").unwrap();
        assert!(empty.tools.deferred.is_empty());
        assert!(empty.tools.hidden.is_empty());
        assert!(empty.tools.groups.is_empty());
        assert!(empty.tools.active_groups.is_empty());
        assert!(empty.tools.max_visible.is_none());
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
        assert_eq!(a.max_agents, b.max_agents);
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
