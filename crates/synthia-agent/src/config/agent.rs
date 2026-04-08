//! Agent configuration
//!
//! Configuration for agent behavior and capabilities.

use std::{fmt, path::PathBuf};

use serde::{Deserialize, Deserializer, Serialize};

use crate::{guardian::GuardianConfig, model_router::ModelConfig};

/// Agent identity and role
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum AgentName {
    /// 独立工作的 agent
    #[default]
    Solo,
    /// 团队领导者 - 可以创建团队、分配任务
    Lead,
    /// 团队成员或子智能体
    /// String 是 agent 的名称（如 "reviewer", "tester", "code-reviewer"）
    Custom(String),
}

impl AgentName {
    pub fn is_solo(&self) -> bool {
        matches!(self, AgentName::Solo)
    }

    pub fn is_lead(&self) -> bool {
        matches!(self, AgentName::Lead)
    }

    pub fn is_custom(&self) -> bool {
        matches!(self, AgentName::Custom(_))
    }

    pub fn as_str(&self) -> &str {
        match self {
            AgentName::Solo => "solo",
            AgentName::Lead => "lead",
            AgentName::Custom(name) => name,
        }
    }
}

impl fmt::Display for AgentName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl Serialize for AgentName {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for AgentName {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(match s.as_str() {
            "solo" => AgentName::Solo,
            "lead" => AgentName::Lead,
            _ => AgentName::Custom(s),
        })
    }
}

/// Agent configuration
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AgentConfig {
    pub name: AgentName,
    pub models: Vec<ModelConfig>,
    pub description: String,
    pub allowed_tools: Vec<String>,
    pub denied_tools: Vec<String>,
    pub hidden: bool,
    pub workspace_dir: PathBuf,
    pub is_subagent: bool,
    pub guardian: GuardianConfig,
    #[serde(default)]
    pub prompt: Option<String>,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            name: AgentName::default(),
            models: Default::default(),
            description: "main agent with general-purpose capabilities"
                .to_string(),
            allowed_tools: Default::default(),
            denied_tools: Default::default(),
            hidden: Default::default(),
            workspace_dir: std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from(".")),
            is_subagent: false,
            guardian: GuardianConfig::default(),
            prompt: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_config_default() {
        let config = AgentConfig::default();
        assert!(config.name.is_solo());
        assert_eq!(
            config.description,
            "main agent with general-purpose capabilities"
        );
        assert!(config.allowed_tools.is_empty());
        assert!(config.denied_tools.is_empty());
        assert!(!config.hidden);
        assert!(!config.is_subagent);
        assert!(config.models.is_empty());
        // GuardianConfig has different defaults, just verify it exists and is enabled
        assert!(config.guardian.enabled);
    }

    #[test]
    fn test_agent_config_serialization() {
        let config = AgentConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let parsed: AgentConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, config.name);
        assert_eq!(parsed.description, config.description);
        assert_eq!(parsed.hidden, config.hidden);
        assert_eq!(parsed.is_subagent, config.is_subagent);
    }

    #[test]
    fn test_agent_config_json_fields() {
        let json = r#"{
            "name": "test-agent",
            "models": [],
            "description": "test description",
            "allowed_tools": ["read", "write"],
            "denied_tools": ["exec"],
            "hidden": true,
            "workspace_dir": "/tmp",
            "is_subagent": true,
            "guardian": {
                "enabled": true,
                "risk_threshold": 80,
                "max_retries": 3,
                "mode": "Simple",
                "dangerous_tools": [],
                "dangerous_patterns": []
            },
            "prompt": "You are a helpful assistant."
        }"#;
        let config: AgentConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.name, AgentName::Custom("test-agent".to_string()));
        assert_eq!(config.description, "test description");
        assert_eq!(config.allowed_tools, vec!["read", "write"]);
        assert_eq!(config.denied_tools, vec!["exec"]);
        assert!(config.hidden);
        assert!(config.is_subagent);
        assert_eq!(
            config.prompt,
            Some("You are a helpful assistant.".to_string())
        );
    }

    #[test]
    fn test_agent_config_toml_serialization() {
        let config = AgentConfig {
            name: AgentName::Custom("toml-test".to_string()),
            models: vec![],
            description: "toml desc".to_string(),
            allowed_tools: vec!["tool1".to_string()],
            denied_tools: vec!["tool2".to_string()],
            hidden: true,
            workspace_dir: std::path::PathBuf::from("/custom/path"),
            is_subagent: true,
            guardian: GuardianConfig::default(),
            prompt: Some("Static system prompt".to_string()),
        };
        let tom = toml::to_string(&config).unwrap();
        let parsed: AgentConfig = toml::from_str(&tom).unwrap();
        assert_eq!(parsed.name, config.name);
        assert_eq!(parsed.hidden, config.hidden);
    }

    #[test]
    fn test_agent_config_workspace_dir_default() {
        let config = AgentConfig::default();
        // workspace_dir should be set to current_dir or fallback to "."
        assert!(!config.workspace_dir.as_os_str().is_empty());
    }

    #[test]
    fn test_agent_config_partial_deserialization() {
        // Test deserialization with minimal fields - all non-defaulted fields must be provided
        let json = r#"{
            "name": "partial-test",
            "models": [],
            "description": "desc only",
            "allowed_tools": [],
            "denied_tools": [],
            "hidden": false,
            "workspace_dir": "/tmp",
            "is_subagent": false,
            "guardian": {
                "enabled": true,
                "risk_threshold": 80,
                "max_retries": 3,
                "mode": "Simple",
                "dangerous_tools": [],
                "dangerous_patterns": []
            }
        }"#;
        let config: AgentConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.name, AgentName::Custom("partial-test".to_string()));
        assert_eq!(config.description, "desc only");
        assert!(config.allowed_tools.is_empty());
        assert!(!config.hidden);
        assert!(config.prompt.is_none());
    }
}
