//! [`OpenAIProfile`] + its constructors.

use serde::{Deserialize, Serialize};

use super::capabilities::ModelCapabilities;

/// Typed OpenAI-compatible provider profile.
///
/// Carries everything `OpenAICompatibleProvider::new(base_url,
/// model_config).with_api_key(&key)` needs, plus the API key
/// environment variable name (resolved at `build_provider` time so
/// profiles are cheap to clone and serialise).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OpenAIProfile {
    /// Profile name (used as the entry key when registered into a
    /// `WorkspaceConfig`-style map; libre consumer code doesn't
    /// need this for direct construction).
    pub name: String,
    /// Base URL (`https://api.openai.com/v1` by default; configurable
    /// for OpenRouter / Azure / local llama.cpp / vLLM / etc.).
    #[serde(default = "default_openai_base_url")]
    pub base_url: String,
    /// Environment variable name to read the API key from.
    /// `OPENAI_API_KEY` is the conventional default; an Azure
    /// deployment might point at `AZURE_OPENAI_API_KEY`.
    #[serde(default = "default_openai_key_env")]
    pub api_key_env: String,
    /// Default model id (e.g. `gpt-4o`, `gpt-4o-mini`,
    /// `meta-llama/llama-3.3-70b-instruct`).
    pub default_model: String,
    /// Capability overrides.
    #[serde(default)]
    pub capabilities: Option<ModelCapabilities>,
}

fn default_openai_base_url() -> String {
    "https://api.openai.com/v1".to_string()
}

fn default_openai_key_env() -> String {
    "OPENAI_API_KEY".to_string()
}

impl OpenAIProfile {
    /// Conventional `openai` entry pointing at api.openai.com with the
    /// `gpt-4o` default model.
    pub fn openai_default() -> Self {
        Self {
            name: "openai".to_string(),
            base_url: default_openai_base_url(),
            api_key_env: default_openai_key_env(),
            default_model: "gpt-4o".to_string(),
            capabilities: None,
        }
    }

    /// Conventional `openai-mini` entry with `gpt-4o-mini` default.
    pub fn openai_mini() -> Self {
        Self {
            name: "openai-mini".to_string(),
            base_url: default_openai_base_url(),
            api_key_env: default_openai_key_env(),
            default_model: "gpt-4o-mini".to_string(),
            capabilities: None,
        }
    }

    /// OpenRouter-flavoured profile: any base URL, any model id.
    pub fn openai_compat(
        name: impl Into<String>,
        base_url: impl Into<String>,
        default_model: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            base_url: base_url.into(),
            api_key_env: default_openai_key_env(),
            default_model: default_model.into(),
            capabilities: None,
        }
    }

    /// Override the resolved capability set (e.g. for a
    /// reasoning-disabled local model).
    pub fn with_capabilities(
        mut self,
        capabilities: ModelCapabilities,
    ) -> Self {
        self.capabilities = Some(capabilities);
        self
    }

    /// Effective capability set (the override or the frontier-chat default).
    pub fn effective_capabilities(&self) -> ModelCapabilities {
        self.capabilities
            .clone()
            .unwrap_or_else(ModelCapabilities::frontier_chat)
    }
}
