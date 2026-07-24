use std::{collections::HashMap, path::Path};

use serde::{Deserialize, Serialize};
use synthia_core::Error;
use synthia_telemetry::SensitiveData;

use crate::{
    anthropic::AnthropicProvider,
    openai::OpenAICompatibleProvider,
    traits::ModelProvider,
    types::ModelConfig,
};

#[derive(Clone, Debug)]
pub struct ProviderConfig {
    pub name: String,
    pub api_key: synthia_telemetry::Sensitive<String>,
    pub api_endpoint: String,
    pub organization: Option<synthia_telemetry::Sensitive<String>>,
    pub headers: HashMap<String, synthia_telemetry::Sensitive<String>>,
}

impl SensitiveData for ProviderConfig {
    fn sensitive_fields() -> Vec<&'static str> {
        vec!["api_key", "organization", "headers"]
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProviderEntry {
    pub r#type: String,
    #[serde(default)]
    pub base_url: Option<String>,
    pub api_key_env: String,
    #[serde(default)]
    pub default_model: Option<String>,
    #[serde(default)]
    pub context_window: Option<usize>,
    #[serde(default)]
    pub max_output_tokens: Option<usize>,
    #[serde(default)]
    pub supports_tools: Option<bool>,
    #[serde(default)]
    pub supports_streaming: Option<bool>,
    #[serde(default)]
    pub supports_reasoning: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorkspaceConfig {
    #[serde(default = "default_provider")]
    pub default_provider: String,
    #[serde(default = "default_model")]
    pub default_model: String,
    pub providers: HashMap<String, ProviderEntry>,
}

fn default_provider() -> String {
    "openai".to_string()
}

fn default_model() -> String {
    "gpt-4o".to_string()
}

impl Default for WorkspaceConfig {
    fn default() -> Self {
        Self {
            default_provider: default_provider(),
            default_model: default_model(),
            providers: HashMap::new(),
        }
    }
}

impl WorkspaceConfig {
    pub fn load_from_dir(workspace_root: &Path) -> Result<Self, Error> {
        let config_path = workspace_root.join(".agents").join("config.toml");
        if config_path.exists() {
            let content = std::fs::read_to_string(&config_path)
                .map_err(|e| Error::Config(e.to_string()))?;
            let config: WorkspaceConfig = toml::from_str(&content)
                .map_err(|e| Error::Parse(e.to_string()))?;
            Ok(config)
        } else {
            Ok(Self::from_env())
        }
    }

    pub fn from_env() -> Self {
        let mut providers = HashMap::new();

        if std::env::var("OPENAI_API_KEY").is_ok()
            || std::env::var("OPENAI_BASE_URL").is_ok()
        {
            providers.insert(
                "openai".to_string(),
                ProviderEntry {
                    r#type: "openai".to_string(),
                    base_url: std::env::var("OPENAI_BASE_URL").ok(),
                    api_key_env: "OPENAI_API_KEY".to_string(),
                    default_model: Some("gpt-4o".to_string()),
                    context_window: Some(128_000),
                    max_output_tokens: Some(4096),
                    supports_tools: Some(true),
                    supports_streaming: Some(true),
                    supports_reasoning: Some(true),
                },
            );
        }

        if std::env::var("ANTHROPIC_API_KEY").is_ok() {
            providers.insert(
                "anthropic".to_string(),
                ProviderEntry {
                    r#type: "anthropic".to_string(),
                    base_url: std::env::var("ANTHROPIC_BASE_URL").ok(),
                    api_key_env: "ANTHROPIC_API_KEY".to_string(),
                    default_model: Some("claude-sonnet-4-20250514".to_string()),
                    context_window: Some(200_000),
                    max_output_tokens: Some(8192),
                    supports_tools: Some(true),
                    supports_streaming: Some(true),
                    supports_reasoning: Some(true),
                },
            );
        }

        let default_provider = if providers.contains_key("openai") {
            "openai".to_string()
        } else if providers.contains_key("anthropic") {
            "anthropic".to_string()
        } else {
            "openai".to_string()
        };

        let default_model = providers
            .get(&default_provider)
            .and_then(|p| p.default_model.clone())
            .unwrap_or_else(|| "gpt-4o".to_string());

        Self {
            default_provider,
            default_model,
            providers,
        }
    }

    pub fn resolve_api_key(
        &self,
        entry: &ProviderEntry,
    ) -> Result<String, Error> {
        std::env::var(&entry.api_key_env).map_err(|_| {
            Error::Config(format!(
                "Missing API key: environment variable {} not set for provider {}",
                entry.api_key_env, entry.r#type
            ))
        })
    }

    pub fn create_provider(
        &self,
        name: &str,
    ) -> Result<Box<dyn ModelProvider>, Error> {
        let entry = self
            .providers
            .get(name)
            .ok_or_else(|| Error::NotFound(name.to_string()))?;

        let api_key = self.resolve_api_key(entry)?;

        let model_config = ModelConfig {
            name: entry
                .default_model
                .clone()
                .unwrap_or_else(|| "unknown".to_string()),
            provider: entry.r#type.clone(),
            context_window: entry.context_window.unwrap_or(128_000),
            max_output_tokens: entry.max_output_tokens.unwrap_or(4096),
            supports_tools: entry.supports_tools.unwrap_or(true),
            supports_streaming: entry.supports_streaming.unwrap_or(true),
            supports_reasoning: entry.supports_reasoning.unwrap_or(false),
        };

        match entry.r#type.as_str() {
            "openai" => {
                let base_url = entry
                    .base_url
                    .clone()
                    .unwrap_or_else(|| "https://api.openai.com/v1".to_string());
                let provider =
                    OpenAICompatibleProvider::new(base_url, model_config)
                        .with_api_key(&api_key);
                Ok(Box::new(provider))
            }
            "anthropic" => {
                let mut provider =
                    AnthropicProvider::new(model_config).with_api_key(&api_key);
                if let Some(base_url) = &entry.base_url {
                    provider = provider.with_base_url(base_url);
                }
                Ok(Box::new(provider))
            }
            other => Err(Error::Validation(format!(
                "Unsupported provider type: {}",
                other
            ))),
        }
    }

    pub fn create_default_provider(
        &self,
    ) -> Result<Box<dyn ModelProvider>, Error> {
        self.create_provider(&self.default_provider)
    }

    pub fn available_providers(&self) -> Vec<String> {
        self.providers.keys().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = WorkspaceConfig::default();
        assert_eq!(config.default_provider, "openai");
        assert_eq!(config.default_model, "gpt-4o");
    }

    #[test]
    fn test_config_from_toml() {
        let toml_str = r#"
default_provider = "anthropic"
default_model = "claude-sonnet-4-20250514"

[providers.openai]
type = "openai"
base_url = "https://api.openai.com/v1"
api_key_env = "OPENAI_API_KEY"
default_model = "gpt-4o"

[providers.anthropic]
type = "anthropic"
api_key_env = "ANTHROPIC_API_KEY"
default_model = "claude-sonnet-4-20250514"
"#;
        let config: WorkspaceConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.default_provider, "anthropic");
        assert_eq!(config.providers.len(), 2);
        assert!(config.providers.contains_key("openai"));
        assert!(config.providers.contains_key("anthropic"));
    }

    #[test]
    fn test_resolve_api_key_missing() {
        let config = WorkspaceConfig::default();
        let entry = ProviderEntry {
            r#type: "openai".to_string(),
            base_url: None,
            api_key_env: "NONEXISTENT_KEY_FOR_TEST_12345".to_string(),
            default_model: None,
            context_window: None,
            max_output_tokens: None,
            supports_tools: None,
            supports_streaming: None,
            supports_reasoning: None,
        };
        let result = config.resolve_api_key(&entry);
        assert!(result.is_err());
    }

    #[test]
    fn test_provider_not_found() {
        let config = WorkspaceConfig::default();
        let result = config.create_provider("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_unsupported_provider_type() {
        let mut config = WorkspaceConfig::default();
        config.providers.insert(
            "test".to_string(),
            ProviderEntry {
                r#type: "gemini".to_string(),
                base_url: None,
                api_key_env: "GEMINI_API_KEY".to_string(),
                default_model: None,
                context_window: None,
                max_output_tokens: None,
                supports_tools: None,
                supports_streaming: None,
                supports_reasoning: None,
            },
        );
        #[allow(unsafe_code)]
        unsafe {
            std::env::set_var("GEMINI_API_KEY", "test-key")
        };
        let result = config.create_provider("test");
        #[allow(unsafe_code)]
        unsafe {
            std::env::remove_var("GEMINI_API_KEY")
        };
        assert!(result.is_err());
    }

    #[test]
    fn test_available_providers() {
        let mut config = WorkspaceConfig::default();
        config.providers.insert(
            "openai".to_string(),
            ProviderEntry {
                r#type: "openai".to_string(),
                base_url: None,
                api_key_env: "OPENAI_API_KEY".to_string(),
                default_model: None,
                context_window: None,
                max_output_tokens: None,
                supports_tools: None,
                supports_streaming: None,
                supports_reasoning: None,
            },
        );
        let providers = config.available_providers();
        assert!(providers.contains(&"openai".to_string()));
    }
}
