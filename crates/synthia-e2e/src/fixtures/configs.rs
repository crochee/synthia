use serde_json::Value;

#[derive(Debug, Clone)]
pub struct TestConfig {
    pub name: String,
    pub config_type: ConfigType,
    pub content: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConfigType {
    Provider,
    Skill,
    Guardian,
    Mcp,
    Behavior,
}

impl TestConfig {
    pub fn provider_openai() -> Self {
        Self {
            name: "provider-openai".to_string(),
            config_type: ConfigType::Provider,
            content: serde_json::json!({
                "provider": "openai",
                "model": "gpt-4o",
                "api_key": "test-key-12345",
                "base_url": "https://api.openai.com/v1",
                "max_tokens": 4096,
                "temperature": 0.7,
                "supports_tools": true,
                "supports_streaming": true
            }),
        }
    }

    pub fn provider_anthropic() -> Self {
        Self {
            name: "provider-anthropic".to_string(),
            config_type: ConfigType::Provider,
            content: serde_json::json!({
                "provider": "anthropic",
                "model": "claude-sonnet-4-20250514",
                "api_key": "test-anthropic-key",
                "max_tokens": 4096,
                "temperature": 0.7,
                "supports_tools": true,
                "supports_streaming": true
            }),
        }
    }

    pub fn guardian_config() -> Self {
        Self {
            name: "guardian-default".to_string(),
            config_type: ConfigType::Guardian,
            content: serde_json::json!({
                "loop_detection": {
                    "enabled": true,
                    "soft_block_after": 5,
                    "hard_block_after": 10
                },
                "circuit_breaker": {
                    "enabled": true,
                    "error_threshold": 5,
                    "timeout_seconds": 60
                },
                "permission_policy": {
                    "default_level": "require_confirm",
                    "trusted_tools": ["read_file", "list_directory"]
                }
            }),
        }
    }

    pub fn behavior_config() -> Self {
        Self {
            name: "behavior-default".to_string(),
            config_type: ConfigType::Behavior,
            content: serde_json::json!({
                "max_iterations": 100,
                "iteration_timeout_seconds": 300,
                "context_window_tokens": 128000,
                "compaction_threshold_percent": 80,
                "compaction_strategy": "summarize"
            }),
        }
    }

    pub fn mcp_config() -> Self {
        Self {
            name: "mcp-filesystem".to_string(),
            config_type: ConfigType::Mcp,
            content: serde_json::json!({
                "mcp_servers": [
                    {
                        "name": "filesystem",
                        "command": "npx",
                        "args": ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"],
                        "env": {}
                    }
                ],
                "idle_timeout_seconds": 300,
                "lazy_connect": true
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_config_structure() {
        let config = TestConfig::provider_openai();
        assert_eq!(config.config_type, ConfigType::Provider);
        assert_eq!(config.content["model"], "gpt-4o");
    }

    #[test]
    fn test_guardian_config_has_loop_detection() {
        let config = TestConfig::guardian_config();
        assert_eq!(config.config_type, ConfigType::Guardian);
        assert!(
            config.content["loop_detection"]["enabled"]
                .as_bool()
                .unwrap()
        );
    }

    #[test]
    fn test_mcp_config_lazy_connect() {
        let config = TestConfig::mcp_config();
        assert!(config.content["lazy_connect"].as_bool().unwrap());
    }
}
