//! Unit tests for [`super::ConfigWatcher`],
//! [`super::MultiConfigWatcher`], and the
//! [`super::types`] data layer.
//!
//! Covers:
//! - `SynthiaConfig::default()` + 4 `validate` paths
//!   (default-ok, empty provider, zero token_budget,
//!   out-of-range compression_threshold).
//! - `SynthiaConfig::load_from_file` (full / partial /
//!   invalid TOML).
//! - `HotReloadableFields::diff` (changed / no-changed).
//! - `SharedConfig` concurrent read/write.
//! - Path resolvers (workspace precedence +
//!   `resolve_all_config_paths` fan-out).
//! - `ConfigType` Display impl.

#![allow(clippy::field_reassign_with_default)]

use std::{path::Path, sync::Arc};

use tokio::sync::RwLock;

use super::{
    resolve_all_config_paths,
    resolve_config_path,
    types::{ConfigType, HotReloadableFields, SharedConfig, SynthiaConfig},
};

#[allow(clippy::field_reassign_with_default)]
fn write_config(path: &Path, content: &str) {
    std::fs::write(path, content).unwrap();
}

#[test]
fn test_synthia_config_default() {
    let cfg = SynthiaConfig::default();
    assert_eq!(cfg.default_provider, "openai");
    assert_eq!(cfg.default_model, "gpt-4o");
    assert_eq!(cfg.token_budget, 128_000);
    assert!((cfg.compression_threshold - 0.85).abs() < f64::EPSILON);
    assert_eq!(cfg.max_iterations, 90);
    assert_eq!(cfg.max_concurrent_tools, 5);
    assert_eq!(cfg.loop_detection_threshold, 5);
    assert!((cfg.bm25_threshold - 0.3).abs() < f64::EPSILON);
}

#[test]
fn test_synthia_config_validate_ok() {
    let cfg = SynthiaConfig::default();
    assert!(cfg.validate().is_ok());
}

#[test]
fn test_synthia_config_validate_empty_provider() {
    let mut cfg = SynthiaConfig::default();
    cfg.default_provider = String::new();
    assert!(cfg.validate().is_err());
}

#[test]
fn test_synthia_config_validate_zero_token_budget() {
    let mut cfg = SynthiaConfig::default();
    cfg.token_budget = 0;
    assert!(cfg.validate().is_err());
}

#[test]
fn test_synthia_config_validate_compression_threshold_out_of_range() {
    let mut cfg = SynthiaConfig::default();
    cfg.compression_threshold = 1.5;
    assert!(cfg.validate().is_err());
}

#[test]
fn test_load_from_file_full() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    write_config(
        &path,
        r#"
[provider]
default_provider = "anthropic"
default_model = "claude-sonnet-4-20250514"

[context]
token_budget = 64000
compression_threshold = 0.9

[agent]
max_iterations = 50
max_concurrent_tools = 3

[guardian]
loop_detection_threshold = 3

[skill]
bm25_threshold = 0.5
"#,
    );

    let cfg = SynthiaConfig::load_from_file(&path).unwrap();
    assert_eq!(cfg.default_provider, "anthropic");
    assert_eq!(cfg.default_model, "claude-sonnet-4-20250514");
    assert_eq!(cfg.token_budget, 64000);
    assert!((cfg.compression_threshold - 0.9).abs() < f64::EPSILON);
    assert_eq!(cfg.max_iterations, 50);
    assert_eq!(cfg.max_concurrent_tools, 3);
    assert_eq!(cfg.loop_detection_threshold, 3);
    assert!((cfg.bm25_threshold - 0.5).abs() < f64::EPSILON);
}

#[test]
fn test_load_from_file_partial() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    write_config(
        &path,
        r#"
[provider]
default_model = "gpt-4o-mini"
"#,
    );

    let cfg = SynthiaConfig::load_from_file(&path).unwrap();
    assert_eq!(cfg.default_model, "gpt-4o-mini");
    assert_eq!(cfg.default_provider, "openai");
    assert_eq!(cfg.token_budget, 128_000);
}

#[test]
fn test_load_from_file_invalid_toml() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    write_config(&path, "this is not [valid toml");
    assert!(SynthiaConfig::load_from_file(&path).is_err());
}

#[test]
fn test_hot_reloadable_fields_diff() {
    let old = SynthiaConfig::default();
    let mut new = SynthiaConfig::default();
    new.default_model = "gpt-4o-mini".to_string();
    new.token_budget = 64000;

    let diff = HotReloadableFields::diff(&old, &new);
    assert!(diff.default_model_changed);
    assert!(diff.token_budget_changed);
    assert!(!diff.max_iterations_changed);

    let names = diff.changed_field_names();
    assert_eq!(names.len(), 2);
    assert!(names.contains(&"provider.default_model"));
    assert!(names.contains(&"context.token_budget"));
}

#[test]
fn test_hot_reloadable_fields_no_changes() {
    let cfg = SynthiaConfig::default();
    let diff = HotReloadableFields::diff(&cfg, &cfg);
    assert!(diff.is_empty());
}

#[tokio::test]
async fn test_shared_config_concurrent_access() {
    let cfg = SynthiaConfig::default();
    let shared: SharedConfig = Arc::new(RwLock::new(cfg));

    let r1 = shared.read().await;
    let r2 = shared.read().await;
    assert_eq!(r1.default_model, r2.default_model);
    drop(r1);
    drop(r2);

    let mut w = shared.write().await;
    w.default_model = "new-model".to_string();
    drop(w);

    let r = shared.read().await;
    assert_eq!(r.default_model, "new-model");
}

#[test]
fn test_resolve_config_path_workspace() {
    let dir = tempfile::tempdir().unwrap();
    let agents = dir.path().join(".agents");
    std::fs::create_dir_all(&agents).unwrap();
    let config = agents.join("config.toml");
    std::fs::write(&config, "[provider]\ndefault_model = \"test\"").unwrap();

    let resolved = resolve_config_path(dir.path());
    assert_eq!(resolved, config);
}

#[test]
fn test_resolve_all_config_paths() {
    let dir = tempfile::tempdir().unwrap();
    let agents = dir.path().join(".agents");
    std::fs::create_dir_all(&agents).unwrap();

    let paths = resolve_all_config_paths(dir.path());

    assert_eq!(paths.len(), 5);
    assert_eq!(
        paths.get(&ConfigType::Main),
        Some(&agents.join("config.toml"))
    );
    assert_eq!(
        paths.get(&ConfigType::Provider),
        Some(&agents.join("providers.toml"))
    );
    assert_eq!(paths.get(&ConfigType::Skill), Some(&agents.join("skills")));
    assert_eq!(
        paths.get(&ConfigType::Permission),
        Some(&agents.join("permissions.toml"))
    );
    assert_eq!(paths.get(&ConfigType::Mcp), Some(&agents.join("mcp.toml")));
}

#[test]
fn test_config_type_display() {
    assert_eq!(ConfigType::Main.to_string(), "main");
    assert_eq!(ConfigType::Provider.to_string(), "provider");
    assert_eq!(ConfigType::Skill.to_string(), "skill");
    assert_eq!(ConfigType::Permission.to_string(), "permission");
    assert_eq!(ConfigType::Mcp.to_string(), "mcp");
}
