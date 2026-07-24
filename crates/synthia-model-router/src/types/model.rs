use serde::{Deserialize, Serialize};

use super::core::ProviderType;

fn default_temperature() -> Option<f32> {
    Some(0.7)
}

fn default_max_tokens() -> u32 {
    4096
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCapabilities {
    pub vision: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub name: String,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub context_window: Option<usize>,
    pub description: Option<String>,
    pub capabilities: Option<ModelCapabilities>,
    #[serde(default = "default_temperature")]
    pub temperature: Option<f32>,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
}

impl ModelInfo {
    pub fn with_name(name: &str) -> Self {
        Self {
            name: name.to_string(),
            api_key: None,
            base_url: None,
            context_window: None,
            description: None,
            capabilities: None,
            temperature: Some(0.7),
            max_tokens: 4096,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ModelConfig {
    Anthropic(ModelInfo),
    OpenAI(ModelInfo),
    OpenAICompatible {
        info: ModelInfo,
        base_url: String,
    },
    Custom {
        provider_type: String,
        info: ModelInfo,
    },
}

impl ModelConfig {
    pub fn model_info(&self) -> &ModelInfo {
        match self {
            ModelConfig::Anthropic(info) => info,
            ModelConfig::OpenAI(info) => info,
            ModelConfig::OpenAICompatible { info, .. } => info,
            ModelConfig::Custom { info, .. } => info,
        }
    }

    pub fn model_info_mut(&mut self) -> &mut ModelInfo {
        match self {
            ModelConfig::Anthropic(info) => info,
            ModelConfig::OpenAI(info) => info,
            ModelConfig::OpenAICompatible { info, .. } => info,
            ModelConfig::Custom { info, .. } => info,
        }
    }

    pub fn provider_type(&self) -> ProviderType {
        match self {
            ModelConfig::Anthropic(_) => ProviderType::Anthropic,
            ModelConfig::OpenAI(_) => ProviderType::OpenAI,
            ModelConfig::OpenAICompatible { .. } => {
                ProviderType::OpenAICompatible
            }
            ModelConfig::Custom { .. } => ProviderType::Custom,
        }
    }

    pub fn anthropic(name: &str) -> Self {
        ModelConfig::Anthropic(ModelInfo::with_name(name))
    }

    pub fn openai(name: &str) -> Self {
        ModelConfig::OpenAI(ModelInfo::with_name(name))
    }

    pub fn openai_compatible(name: &str, base_url: &str) -> Self {
        ModelConfig::OpenAICompatible {
            info: ModelInfo::with_name(name),
            base_url: base_url.to_string(),
        }
    }
}
