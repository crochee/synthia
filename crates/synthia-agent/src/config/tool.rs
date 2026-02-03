//! Tool configuration
//!
//! Configuration for tool execution behavior.

use serde::{Deserialize, Serialize};

fn default_notification_interval_secs() -> u64 {
    30
}

fn default_max_notifications() -> usize {
    3
}

fn default_max_concurrent_tools() -> usize {
    5
}

fn default_tool_timeout_secs() -> u64 {
    30
}

fn default_read_pool_size() -> usize {
    10
}

fn default_write_pool_size() -> usize {
    5
}

/// Configuration for tool execution behavior.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct ToolConfig {
    #[serde(default = "default_notification_interval_secs")]
    pub notification_interval_secs: u64,
    #[serde(default = "default_max_notifications")]
    pub max_notifications: usize,
    #[serde(default = "default_max_concurrent_tools")]
    pub max_concurrent_tools: usize,
    #[serde(default = "default_tool_timeout_secs")]
    pub default_tool_timeout_secs: u64,
    #[serde(default = "default_read_pool_size")]
    pub read_pool_size: usize,
    #[serde(default = "default_write_pool_size")]
    pub write_pool_size: usize,
}

impl Default for ToolConfig {
    fn default() -> Self {
        Self {
            notification_interval_secs: 30,
            max_notifications: 3,
            max_concurrent_tools: 5,
            default_tool_timeout_secs: 30,
            read_pool_size: 10,
            write_pool_size: 5,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_config_default() {
        let config = ToolConfig::default();
        assert_eq!(config.notification_interval_secs, 30);
        assert_eq!(config.max_notifications, 3);
        assert_eq!(config.max_concurrent_tools, 5);
        assert_eq!(config.default_tool_timeout_secs, 30);
        assert_eq!(config.read_pool_size, 10);
        assert_eq!(config.write_pool_size, 5);
    }

    #[test]
    fn test_tool_config_serialization() {
        let config = ToolConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let parsed: ToolConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, config);
    }

    #[test]
    fn test_tool_config_json_roundtrip() {
        let config = ToolConfig {
            notification_interval_secs: 60,
            max_notifications: 5,
            max_concurrent_tools: 10,
            default_tool_timeout_secs: 120,
            read_pool_size: 20,
            write_pool_size: 8,
        };

        let json = serde_json::to_string(&config).unwrap();
        let parsed: ToolConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.notification_interval_secs, 60);
        assert_eq!(parsed.max_notifications, 5);
        assert_eq!(parsed.max_concurrent_tools, 10);
        assert_eq!(parsed.default_tool_timeout_secs, 120);
        assert_eq!(parsed.read_pool_size, 20);
        assert_eq!(parsed.write_pool_size, 8);
    }

    #[test]
    fn test_tool_config_partial_deserialization() {
        let json = r#"{"notification_interval_secs": 45}"#;
        let config: ToolConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.notification_interval_secs, 45);
        // Verify defaults for other fields
        assert_eq!(config.max_notifications, 3);
        assert_eq!(config.max_concurrent_tools, 5);
        assert_eq!(config.default_tool_timeout_secs, 30);
    }

    #[test]
    fn test_tool_config_toml_serialization() {
        let config = ToolConfig {
            notification_interval_secs: 90,
            max_notifications: 10,
            max_concurrent_tools: 15,
            default_tool_timeout_secs: 60,
            read_pool_size: 30,
            write_pool_size: 12,
        };

        let tom = toml::to_string(&config).unwrap();
        let parsed: ToolConfig = toml::from_str(&tom).unwrap();

        assert_eq!(parsed.notification_interval_secs, 90);
        assert_eq!(parsed.max_notifications, 10);
        assert_eq!(parsed.max_concurrent_tools, 15);
        assert_eq!(parsed.default_tool_timeout_secs, 60);
        assert_eq!(parsed.read_pool_size, 30);
        assert_eq!(parsed.write_pool_size, 12);
    }

    #[test]
    fn test_tool_config_default_functions() {
        assert_eq!(default_notification_interval_secs(), 30);
        assert_eq!(default_max_notifications(), 3);
        assert_eq!(default_max_concurrent_tools(), 5);
        assert_eq!(default_tool_timeout_secs(), 30);
        assert_eq!(default_read_pool_size(), 10);
        assert_eq!(default_write_pool_size(), 5);
    }

    #[test]
    fn test_tool_config_clone() {
        let config = ToolConfig::default();
        let cloned = config;
        assert_eq!(cloned, config);
    }

    #[test]
    fn test_tool_config_debug() {
        let config = ToolConfig::default();
        let debug_str = format!("{config:?}");
        assert!(debug_str.contains("ToolConfig"));
        assert!(debug_str.contains("notification_interval_secs"));
    }

    #[test]
    fn test_tool_config_eq() {
        let config1 = ToolConfig::default();
        let config2 = ToolConfig::default();
        assert_eq!(config1, config2);

        let config3 = ToolConfig {
            notification_interval_secs: 99,
            ..ToolConfig::default()
        };
        assert_ne!(config1, config3);
    }

    #[test]
    fn test_tool_config_hash() {
        use std::collections::HashSet;
        let config = ToolConfig::default();
        let mut set = HashSet::new();
        set.insert(config);
        assert!(set.contains(&ToolConfig::default()));
    }
}
