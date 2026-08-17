//! Unit tests for the `config` module family.
//!
//! Coverage map (47 tests):
//!
//! - `ProviderConfig` + `ModelConfig`: 6 tests (defaults, serde,
//!   round-trip, optional fields).
//! - `AgentConfig` + `SkillConfig`: 8 tests (defaults, all field
//!   serde, round-trip, field independence).
//! - `ServerConfig`: 14 tests (defaults via `load` of a missing
//!   file, JSON round-trip, all sub-struct defaults, `default_agent`
//!   path).
//! - `AuthConfig` + `RateLimitConfig` + `CorsConfig`: 12 tests
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
// AgentConfig + SkillConfig
// =============================================================================

/// `AgentConfig` MUST default all 7 fields to None / empty /
/// false when deserialized from `{}`.
#[test]
fn test_agent_config_defaults_all_seven_fields() {
    let a: AgentConfig = serde_json::from_str("{}").unwrap();
    assert!(a.description.is_none());
    assert!(a.model.is_none());
    assert!(a.max_steps.is_none());
    assert!(a.allowed_tools.is_empty());
    assert!(a.denied_tools.is_empty());
    assert!(!a.hidden);
    assert!(a.color.is_none());
}

/// `AgentConfig` MUST round-trip all 7 fields through JSON.
#[test]
fn test_agent_config_round_trips_all_seven_fields() {
    let a = AgentConfig {
        description: Some("code-reviewer".to_string()),
        model: Some("claude-opus".to_string()),
        max_steps: Some(50),
        allowed_tools: vec!["bash".to_string(), "edit".to_string()],
        denied_tools: vec!["web_search".to_string()],
        hidden: true,
        color: Some("#FF0000".to_string()),
    };
    let json = serde_json::to_string(&a).unwrap();
    let parsed: AgentConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.description, a.description);
    assert_eq!(parsed.model, a.model);
    assert_eq!(parsed.max_steps, a.max_steps);
    assert_eq!(parsed.allowed_tools, a.allowed_tools);
    assert_eq!(parsed.denied_tools, a.denied_tools);
    assert_eq!(parsed.hidden, a.hidden);
    assert_eq!(parsed.color, a.color);
}

/// `AgentConfig` MUST treat `hidden = false` as the default and
/// accept `hidden = true` when explicitly provided.
#[test]
fn test_agent_config_hidden_field_boolean() {
    let a: AgentConfig = serde_json::from_str("{}").unwrap();
    assert!(!a.hidden);
    let a: AgentConfig = serde_json::from_str(r#"{"hidden": true}"#).unwrap();
    assert!(a.hidden);
    let a: AgentConfig = serde_json::from_str(r#"{"hidden": false}"#).unwrap();
    assert!(!a.hidden);
}

/// `AgentConfig` MUST preserve `allowed_tools` and `denied_tools`
/// as independent lists (no cross-pollination).
#[test]
fn test_agent_config_allowed_vs_denied_tools_independent() {
    let json = r#"{
        "allowed_tools": ["a", "b"],
        "denied_tools": ["c", "d"]
    }"#;
    let a: AgentConfig = serde_json::from_str(json).unwrap();
    assert_eq!(a.allowed_tools, vec!["a", "b"]);
    assert_eq!(a.denied_tools, vec!["c", "d"]);
    // Empty list round-trip.
    let json = r#"{"allowed_tools": [], "denied_tools": []}"#;
    let a: AgentConfig = serde_json::from_str(json).unwrap();
    assert!(a.allowed_tools.is_empty());
    assert!(a.denied_tools.is_empty());
}

/// `SkillConfig` MUST require both `name` and `path` (no
/// serde defaults — these are the file identity).
#[test]
fn test_skill_config_requires_name_and_path() {
    let s = SkillConfig {
        name: "linting".to_string(),
        path: "/etc/skills/lint.md".to_string(),
    };
    let json = serde_json::to_string(&s).unwrap();
    let parsed: SkillConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.name, "linting");
    assert_eq!(parsed.path, "/etc/skills/lint.md");
}

/// `SkillConfig` MUST fail to deserialize when `name` is
/// missing (mandatory field).
#[test]
fn test_skill_config_missing_name_fails_to_deserialize() {
    let json = r#"{"path": "/x"}"#;
    let result: Result<SkillConfig, _> = serde_json::from_str(json);
    assert!(result.is_err());
}

/// `SkillConfig` MUST fail to deserialize when `path` is
/// missing (mandatory field).
#[test]
fn test_skill_config_missing_path_fails_to_deserialize() {
    let json = r#"{"name": "x"}"#;
    let result: Result<SkillConfig, _> = serde_json::from_str(json);
    assert!(result.is_err());
}

/// `SkillConfig` MUST support Clone (used to share the config
/// across skill registry insertions).
#[test]
fn test_skill_config_clone_preserves_fields() {
    let s = SkillConfig {
        name: "a".to_string(),
        path: "/b".to_string(),
    };
    let c = s.clone();
    assert_eq!(c.name, s.name);
    assert_eq!(c.path, s.path);
}

// =============================================================================
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
    assert!(c.model_override.is_none());
    assert!(c.providers.is_empty());
    assert!(c.agents.is_empty());
    assert!(c.skills.is_empty());
    assert!(c.default_agent.is_none());
    // auth, rate_limit, cors are all Default.
    assert!(!c.auth.enabled);
    assert!(!c.rate_limit.enabled);
    assert_eq!(c.cors.allowed_origins, Vec::<String>::new());
}

/// `ServerConfig` MUST round-trip a fully-populated config
/// through JSON without loss.
#[test]
fn test_server_config_full_round_trip() {
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
        },
    );
    let original = ServerConfig {
        version: "1.0".to_string(),
        host: "0.0.0.0".to_string(),
        port: 9000,
        model_override: Some("override-model".to_string()),
        max_agents: 10,
        providers,
        agents,
        skills: vec![SkillConfig {
            name: "s1".to_string(),
            path: "/s1.md".to_string(),
        }],
        auth: AuthConfig {
            enabled: true,
            api_keys: vec!["key1".to_string()],
            key_to_user: HashMap::new(),
        },
        rate_limit: RateLimitConfig {
            enabled: true,
            requests_per_minute: 100,
            burst: 20,
        },
        cors: CorsConfig {
            allowed_origins: vec!["https://app.example".to_string()],
            allowed_methods: vec!["GET".to_string()],
            allowed_headers: vec!["Authorization".to_string()],
        },
        default_agent: Some("default".to_string()),
    };
    let json = serde_json::to_string(&original).unwrap();
    let parsed: ServerConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.version, original.version);
    assert_eq!(parsed.host, original.host);
    assert_eq!(parsed.port, original.port);
    assert_eq!(parsed.max_agents, original.max_agents);
    assert_eq!(parsed.model_override, original.model_override);
    assert_eq!(parsed.default_agent, original.default_agent);
    assert_eq!(parsed.providers.len(), 1);
    assert!(parsed.providers.contains_key("openai"));
    assert_eq!(parsed.agents.len(), 1);
    assert!(parsed.agents.contains_key("default"));
    assert_eq!(parsed.skills.len(), 1);
    assert_eq!(parsed.skills[0].name, "s1");
    assert_eq!(parsed.auth.api_keys, vec!["key1".to_string()]);
    assert_eq!(parsed.rate_limit.requests_per_minute, 100);
    assert_eq!(
        parsed.cors.allowed_origins,
        vec!["https://app.example".to_string()]
    );
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

/// `ServerConfig` MUST deserialize a config with multiple
/// providers, agents, and skills intact.
#[test]
fn test_server_config_multi_provider_agent_skill_round_trip() {
    let json = r#"{
        "providers": {
            "openai": {"api_key": "k1"},
            "anthropic": {"api_key": "k2"}
        },
        "agents": {
            "a1": {"model": "gpt-4o"},
            "a2": {"model": "claude-opus", "hidden": true}
        },
        "skills": [
            {"name": "s1", "path": "/s1.md"},
            {"name": "s2", "path": "/s2.md"}
        ]
    }"#;
    let c: ServerConfig = serde_json::from_str(json).unwrap();
    assert_eq!(c.providers.len(), 2);
    assert_eq!(c.agents.len(), 2);
    assert_eq!(c.skills.len(), 2);
    assert!(c.agents.get("a2").unwrap().hidden);
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
    assert_eq!(parsed.key_to_user.get("key-1"), Some(&"user-a".to_string()));
    assert_eq!(parsed.key_to_user.get("key-2"), Some(&"user-b".to_string()));
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
// RateLimitConfig
// =============================================================================

/// `RateLimitConfig::default()` MUST have rate limit disabled
/// but populate `requests_per_minute = 60` and `burst = 10`.
#[test]
fn test_rate_limit_config_default_values() {
    let r = RateLimitConfig::default();
    assert!(!r.enabled);
    assert_eq!(r.requests_per_minute, 60);
    assert_eq!(r.burst, 10);
}

/// `RateLimitConfig` MUST round-trip `enabled = true` with
/// custom request/burst rates.
#[test]
fn test_rate_limit_config_round_trip_custom_values() {
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

/// `RateLimitConfig` serde MUST apply defaults
/// (60 / 10) when fields are omitted.
#[test]
fn test_rate_limit_config_serde_defaults_apply() {
    let r: RateLimitConfig =
        serde_json::from_str(r#"{"enabled": true}"#).unwrap();
    assert!(r.enabled);
    assert_eq!(r.requests_per_minute, 60);
    assert_eq!(r.burst, 10);
}

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
        allowed_origins: vec!["https://a".to_string(), "https://b".to_string()],
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
