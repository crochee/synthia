//! [`AnthropicProfile`] + its constructors.

use serde::{Deserialize, Serialize};

use super::{
    DEFAULT_ANTHROPIC_CONTEXT_WINDOW,
    DEFAULT_ANTHROPIC_MAX_OUTPUT_TOKENS,
    capabilities::ModelCapabilities,
};

/// Typed Anthropic provider profile.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AnthropicProfile {
    pub name: String,
    /// Optional base URL override (Anthropic Bedrock / Vertex / proxy).
    #[serde(default)]
    pub base_url: Option<String>,
    /// Environment variable name to read the API key from.
    #[serde(default = "default_anthropic_key_env")]
    pub api_key_env: String,
    /// Default model id (e.g. `claude-sonnet-4-20250514`).
    pub default_model: String,
    #[serde(default)]
    pub capabilities: Option<ModelCapabilities>,
}

fn default_anthropic_key_env() -> String {
    "ANTHROPIC_API_KEY".to_string()
}

impl AnthropicProfile {
    /// Conventional Anthropic entry pointing at Claude Sonnet 4.
    pub fn claude_sonnet_4() -> Self {
        Self {
            name: "anthropic".to_string(),
            base_url: None,
            api_key_env: default_anthropic_key_env(),
            default_model: "claude-sonnet-4-20250514".to_string(),
            capabilities: None,
        }
    }

    /// Conventional Anthropic entry pointing at Claude Haiku 4.5.
    pub fn claude_haiku_4_5() -> Self {
        Self {
            name: "anthropic-haiku".to_string(),
            base_url: None,
            api_key_env: default_anthropic_key_env(),
            default_model: "claude-haiku-4-5-20251001".to_string(),
            capabilities: None,
        }
    }

    /// Anthropic-on-Vertex / Bedrock proxy variant: any base URL.
    pub fn anthropic_proxy(
        name: impl Into<String>,
        base_url: impl Into<String>,
        default_model: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            base_url: Some(base_url.into()),
            api_key_env: default_anthropic_key_env(),
            default_model: default_model.into(),
            capabilities: None,
        }
    }

    pub fn with_capabilities(
        mut self,
        capabilities: ModelCapabilities,
    ) -> Self {
        self.capabilities = Some(capabilities);
        self
    }

    pub fn effective_capabilities(&self) -> ModelCapabilities {
        self.capabilities.clone().unwrap_or(ModelCapabilities {
            supports_tools: true,
            supports_streaming: true,
            supports_reasoning: true,
            max_output_tokens: DEFAULT_ANTHROPIC_MAX_OUTPUT_TOKENS,
            context_window: DEFAULT_ANTHROPIC_CONTEXT_WINDOW,
        })
    }
}
