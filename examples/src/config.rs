use std::env;

use anyhow::{Context, Result};

#[derive(Debug, Clone)]
pub struct ProviderConfig {
    pub base_url: String,
    pub api_key: Option<String>,
    pub model_name: String,
}

impl ProviderConfig {
    pub fn from_env_openai() -> Result<Self> {
        let base_url =
            env::var("OPENAI_BASE_URL").context("OPENAI_BASE_URL")?;
        let api_key = env::var("OPENAI_API_KEY").context("OPENAI_API_KEY")?;
        let model_name = env::var("OPENAI_MODEL").context("OPENAI_MODEL")?;

        Ok(Self {
            base_url,
            api_key: Some(api_key),
            model_name,
        })
    }

    pub fn from_env_local() -> Result<Self> {
        let base_url = env::var("LOCAL_MODEL_URL")
            .unwrap_or_else(|_| "http://localhost:11434/v1".to_string());
        let model_name =
            env::var("LOCAL_MODEL_NAME").context("LOCAL_MODEL_NAME")?;

        Ok(Self {
            base_url,
            api_key: None,
            model_name,
        })
    }

    pub fn from_env_auto() -> Result<Self> {
        if env::var("OPENAI_BASE_URL").is_ok() {
            Self::from_env_openai()
        } else if env::var("LOCAL_MODEL_NAME").is_ok() {
            Self::from_env_local()
        } else {
            Err(anyhow::Error::msg(
                "OPENAI_BASE_URL or LOCAL_MODEL_NAME is required",
            ))
        }
    }

    pub fn provider_type(&self) -> &'static str {
        if self.api_key.is_some() {
            "OpenAI Compatible"
        } else {
            "Local Model"
        }
    }

    pub fn create_provider(&self) -> crate::OpenAICompatibleProvider {
        let mut provider = crate::OpenAICompatibleProvider::default()
            .with_base_url(&self.base_url);

        if let Some(api_key) = &self.api_key {
            provider = provider.with_api_key(api_key);
        }

        provider
    }

    pub fn default_message(&self) -> &'static str {
        if self.api_key.is_some() {
            "你好！"
        } else {
            "Hello!"
        }
    }

    pub fn default_tool_message(&self) -> &'static str {
        if self.api_key.is_some() {
            "现在几点了？"
        } else {
            "What time is it now?"
        }
    }
}
