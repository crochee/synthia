use std::path::PathBuf;

use crate::config::{ContextConfig, ProtectionZoneConfig};

#[test]
fn context_config_default() {
    let config = ContextConfig::default();
    assert_eq!(config.max_context_tokens, 128_000);
    assert_eq!(config.pre_sampling_threshold, 0.7);
    assert_eq!(config.mid_turn_threshold, 0.85);
    assert_eq!(config.max_iterations, 50);
    assert!(config.hot_memory_path.is_none());
}

#[test]
fn context_config_with_hot_memory_path() {
    let config = ContextConfig::default();
    assert!(config.hot_memory_path.is_none());

    let config = ContextConfig {
        hot_memory_path: Some(PathBuf::from("/tmp/hot_memory")),
        ..Default::default()
    };
    assert_eq!(
        config.hot_memory_path.unwrap(),
        PathBuf::from("/tmp/hot_memory")
    );
}

#[test]
fn context_config_serialize_deserialize() {
    let config = ContextConfig::default();
    let json = serde_json::to_string(&config).unwrap();
    let parsed: ContextConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.max_context_tokens, config.max_context_tokens);
    assert_eq!(parsed.pre_sampling_threshold, config.pre_sampling_threshold);
}

#[test]
fn protection_zone_config_default() {
    let config = ProtectionZoneConfig::default();
    assert_eq!(config.min_rounds, 3);
    assert_eq!(config.token_ratio, 0.35);
}

#[test]
fn protection_zone_config_custom() {
    let config = ProtectionZoneConfig {
        min_rounds: 5,
        token_ratio: 0.5,
    };
    assert_eq!(config.min_rounds, 5);
    assert_eq!(config.token_ratio, 0.5);
}

#[test]
fn context_config_protection_zone() {
    let config = ContextConfig::default();
    assert_eq!(config.protection_zone.min_rounds, 3);
    assert_eq!(config.protection_zone.token_ratio, 0.35);
}
