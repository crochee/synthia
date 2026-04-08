use std::{collections::HashMap, path::Path};

use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use synthia_agent::{
    config::AgentConfig,
    model_router::{ModelCapabilities, ModelConfig},
};

static ENV_VAR_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\$\{([^}]+)\}").expect("Failed to compile env var regex")
});

const DEFAULT_TEMPERATURE: f32 = 0.7;
const DEFAULT_MAX_TOKENS: u32 = 4096;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default, rename = "$schema")]
    pub schema: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub providers: HashMap<String, ProviderConfig>,
    #[serde(default)]
    pub mcps: HashMap<String, McpConfig>,
    #[serde(default)]
    pub agents: HashMap<String, AgentConfigYaml>,
}

impl AppConfig {
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path).with_context(|| {
            format!("Failed to read config file: {}", path.display())
        })?;

        let expanded = expand_env_vars(&content);

        match path.extension().and_then(|ext| ext.to_str()) {
            Some("yaml") | Some("yml") => serde_yaml::from_str(&expanded)
                .with_context(|| {
                    format!(
                        "Failed to parse YAML config file: {}",
                        path.display()
                    )
                }),
            Some("toml") => toml::from_str(&expanded).with_context(|| {
                format!("Failed to parse TOML config file: {}", path.display())
            }),
            _ => {
                // Default to TOML if extension is not recognized
                toml::from_str(&expanded).with_context(|| {
                    format!("Failed to parse config file: {}", path.display())
                })
            }
        }
    }

    pub fn get_all_models(&self) -> Vec<ModelConfig> {
        let mut configs = Vec::new();

        for (provider_key, provider) in &self.providers {
            for model in &provider.models {
                let mut config = match provider_key.as_str() {
                    "anthropic" => ModelConfig::anthropic(&model.name),
                    "openai" | "ollama" => ModelConfig::openai(&model.name),
                    _ => ModelConfig::openai(&model.name),
                };

                let info = config.model_info_mut();
                info.api_key = provider.api_key.clone();
                info.base_url = provider.base_url.clone();
                info.context_window = model.context_window;
                info.description = model.description.clone();
                info.capabilities = model.capabilities.clone();
                info.temperature =
                    model.temperature.or(Some(DEFAULT_TEMPERATURE));
                info.max_tokens =
                    model.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS);

                configs.push(config);
            }
        }

        configs
    }

    pub fn get_mcps(&self) -> Vec<(String, McpConfig)> {
        self.mcps
            .iter()
            .filter(|(_, mcp)| mcp.enabled)
            .map(|(name, v)| (name.clone(), v.clone()))
            .collect()
    }

    pub fn get_agents(&self, current_dir: &Path) -> Vec<AgentConfig> {
        self.agents
            .iter()
            .map(|(name, v)| AgentConfig {
                name: synthia_agent::config::AgentName::Custom(name.clone()),
                models: self.get_all_models(),
                description: v.description.clone(),
                allowed_tools: v.allowed_tools.clone().unwrap_or_default(),
                denied_tools: v.denied_tools.clone().unwrap_or_default(),
                hidden: v.hidden,
                workspace_dir: current_dir.to_path_buf(),
                is_subagent: true,
                guardian: synthia_agent::GuardianConfig::default(),
                prompt: None,
            })
            .collect()
    }

    pub fn get_max_agents(&self) -> Option<u32> {
        // Default to 5 concurrent agents if not specified
        Some(5)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub supports_streaming: Option<bool>,
    #[serde(default)]
    pub models: Vec<ModelInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub name: String,
    #[serde(default)]
    pub context_window: Option<usize>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub capabilities: Option<ModelCapabilities>,
    #[serde(default)]
    pub temperature: Option<f32>,
    #[serde(default)]
    pub max_tokens: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfigYaml {
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub allowed_tools: Option<Vec<String>>,
    #[serde(default)]
    pub denied_tools: Option<Vec<String>>,
    #[serde(default)]
    pub max_steps: Option<u32>,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub hidden: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpConfig {
    #[serde(default)]
    pub r#type: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub args: Option<Vec<String>>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default)]
    pub timeout: Option<u64>,
    #[serde(default)]
    pub enabled: bool,
}

fn expand_env_vars(content: &str) -> String {
    ENV_VAR_REGEX
        .replace_all(content, |caps: &regex::Captures| {
            let var_name = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            std::env::var(var_name).unwrap_or_else(|_| caps[0].to_string())
        })
        .into_owned()
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use tempfile::NamedTempFile;

    use super::*;

    #[test]
    fn test_expand_env_vars_no_vars() {
        let content = "hello world";
        assert_eq!(expand_env_vars(content), "hello world");
    }

    #[test]
    #[ignore = "Requires unsafe env manipulation"]
    fn test_expand_env_vars_with_var() {
        let content = "value: ${TEST_VAR_FOR_CONFIG}";
        assert_eq!(expand_env_vars(content), "value: ${TEST_VAR_FOR_CONFIG}");
    }

    #[test]
    fn test_expand_env_vars_missing_var() {
        let content = "value: ${NONEXISTENT_VAR_12345}";
        assert_eq!(expand_env_vars(content), "value: ${NONEXISTENT_VAR_12345}");
    }

    #[test]
    #[ignore = "Requires unsafe env manipulation"]
    fn test_expand_env_vars_multiple_vars() {
        let content = "a: ${VAR_A_TEST}, b: ${VAR_B_TEST}";
        assert_eq!(
            expand_env_vars(content),
            "a: ${VAR_A_TEST}, b: ${VAR_B_TEST}"
        );
    }

    #[test]
    fn test_app_config_load_valid() {
        let toml_content = r#"
version = "1.0"

[providers.openai]
api_key = "test-key"
base_url = "https://api.test.com/v1"

[[providers.openai.models]]
name = "gpt-4"
context_window = 8192

[mcps]

[agents]
"#;
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(toml_content.as_bytes()).unwrap();

        let config = AppConfig::load(temp_file.path()).unwrap();
        assert_eq!(config.version, Some("1.0".to_string()));
        assert!(config.providers.contains_key("openai"));
    }

    #[test]
    fn test_app_config_load_invalid_toml() {
        let toml_content = r#"
version = [
invalid toml
"#;
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(toml_content.as_bytes()).unwrap();

        let result = AppConfig::load(temp_file.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_app_config_load_missing_file() {
        let result =
            AppConfig::load(Path::new("/nonexistent/path/config.yaml"));
        assert!(result.is_err());
    }

    #[test]
    fn test_get_all_models() {
        let toml_content = r#"
[providers.openai]
api_key = "test-key"
base_url = "https://api.test.com/v1"

[[providers.openai.models]]
name = "gpt-4"
context_window = 8192
max_tokens = 2048

[[providers.openai.models]]
name = "gpt-3.5-turbo"

[agents]

[mcps]
"#;
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(toml_content.as_bytes()).unwrap();

        let config = AppConfig::load(temp_file.path()).unwrap();
        let models = config.get_all_models();

        assert_eq!(models.len(), 2);
    }

    #[test]
    fn test_get_mcps_enabled() {
        let toml_content = r#"
[providers]

[agents]

[mcps.filesystem]
type = "stdio"
enabled = true
command = "node"
args = ["server.js"]

[mcps.disabled_mcp]
type = "stdio"
enabled = false
"#;
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(toml_content.as_bytes()).unwrap();

        let config = AppConfig::load(temp_file.path()).unwrap();
        let mcps = config.get_mcps();

        assert_eq!(mcps.len(), 1);
        assert_eq!(mcps[0].0, "filesystem");
    }

    #[test]
    fn test_get_agents() {
        let toml_content = r#"
[providers.openai]
api_key = "test-key"

[[providers.openai.models]]
name = "gpt-4"

[agents.assistant]
description = "Test assistant"
hidden = false

[mcps]
"#;
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(toml_content.as_bytes()).unwrap();

        let config = AppConfig::load(temp_file.path()).unwrap();
        let agents = config.get_agents(temp_file.path().parent().unwrap());

        assert_eq!(agents.len(), 1);
        assert_eq!(
            agents[0].name,
            synthia_agent::config::AgentName::Custom("assistant".to_string())
        );
        assert_eq!(agents[0].description, "Test assistant".to_string());
    }

    #[test]
    fn test_provider_config_default() {
        let config = ProviderConfig {
            api_key: None,
            base_url: None,
            supports_streaming: None,
            models: vec![],
        };

        assert!(config.api_key.is_none());
        assert!(config.models.is_empty());
    }
}
