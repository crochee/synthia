//! [`ProviderProfile`] — the closed enum + the surface
//! (build / resolve / model_config).

use serde::{Deserialize, Serialize};
use synthia_core::{Error, Sensitive};

use super::{
    DEFAULT_OPENAI_CONTEXT_WINDOW,
    DEFAULT_OPENAI_MAX_OUTPUT_TOKENS,
    anthropic::AnthropicProfile,
    openai::OpenAIProfile,
    stub::StubProfile,
};
#[cfg(feature = "anthropic")]
use crate::anthropic::AnthropicProvider;
#[cfg(feature = "openai")]
use crate::openai::OpenAICompatibleProvider;
use crate::{
    traits::ModelProvider,
    traits_stub::ModelProviderStub,
    types::ModelConfig,
};

/// The closed enum a lib consumer selects from.
///
/// New variants are added when a new backend lands (Bedrock / Vertex
/// / Cohere / etc.) — existing variants are stable.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProviderProfile {
    /// OpenAI-compatible backend (official OpenAI / OpenRouter /
    /// Azure / local llama.cpp).
    OpenAI(OpenAIProfile),
    /// Anthropic backend (official / Bedrock / Vertex proxy).
    Anthropic(AnthropicProfile),
    /// Offline stub for tests and tutorials (no network, no API key).
    Stub(StubProfile),
}

impl ProviderProfile {
    /// Wire-string identifier (`"openai"` / `"anthropic"` / `"stub"`).
    ///
    /// Stable wire tag used by session telemetry, log breadcrumbs,
    /// and `SessionEvent::Compaction` projection consumers. Do **not**
    /// change these strings without a migration plan.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::OpenAI(_) => "openai",
            Self::Anthropic(_) => "anthropic",
            Self::Stub(_) => "stub",
        }
    }

    /// Display name for log / UI surfaces.
    pub fn name(&self) -> &str {
        match self {
            Self::OpenAI(p) => &p.name,
            Self::Anthropic(p) => &p.name,
            Self::Stub(p) => &p.name,
        }
    }

    /// Default model id (`"gpt-4o"` / `"claude-sonnet-4-20250514"` / etc.).
    pub fn default_model(&self) -> &str {
        match self {
            Self::OpenAI(p) => &p.default_model,
            Self::Anthropic(p) => &p.default_model,
            Self::Stub(p) => &p.default_model,
        }
    }

    /// Resolve the API key environment variable name. Stubs return
    /// `None` because the stub provider needs no key.
    pub fn api_key_env(&self) -> Option<&str> {
        match self {
            Self::OpenAI(p) => Some(&p.api_key_env),
            Self::Anthropic(p) => Some(&p.api_key_env),
            Self::Stub(_) => None,
        }
    }

    /// Build the [`ModelConfig`] this profile projects.
    pub fn model_config(&self) -> ModelConfig {
        match self {
            Self::OpenAI(p) => {
                let caps = p.effective_capabilities();
                ModelConfig {
                    name: p.default_model.clone(),
                    provider: "openai".to_string(),
                    context_window: caps.context_window,
                    max_output_tokens: caps.max_output_tokens,
                    supports_tools: caps.supports_tools,
                    supports_streaming: caps.supports_streaming,
                    supports_reasoning: caps.supports_reasoning,
                }
            }
            Self::Anthropic(p) => {
                let caps = p.effective_capabilities();
                ModelConfig {
                    name: p.default_model.clone(),
                    provider: "anthropic".to_string(),
                    context_window: caps.context_window,
                    max_output_tokens: caps.max_output_tokens,
                    supports_tools: caps.supports_tools,
                    supports_streaming: caps.supports_streaming,
                    supports_reasoning: caps.supports_reasoning,
                }
            }
            Self::Stub(p) => ModelConfig {
                name: p.default_model.clone(),
                provider: "stub".to_string(),
                context_window: DEFAULT_OPENAI_CONTEXT_WINDOW,
                max_output_tokens: DEFAULT_OPENAI_MAX_OUTPUT_TOKENS,
                supports_tools: true,
                supports_streaming: true,
                supports_reasoning: false,
            },
        }
    }

    /// Resolve the API key from the environment.
    pub fn resolve_api_key(&self) -> Result<Sensitive<String>, Error> {
        let env_name = self.api_key_env().ok_or_else(|| {
            Error::config(format!(
                "provider profile '{}' (kind={}) does not require an API key",
                self.name(),
                self.kind()
            ))
        })?;
        let raw = std::env::var(env_name).map_err(|_| {
            Error::config(format!(
                "Missing API key: environment variable {} not set for provider profile '{}'",
                env_name,
                self.name()
            ))
        })?;
        let trimmed = match crate::credential::normalize_api_key(&raw) {
            Ok(s) => s,
            Err(crate::credential::CredentialError::Empty) => {
                return Err(Error::config(format!(
                    "Invalid API key in {env_name}: empty after trim"
                )));
            }
            Err(crate::credential::CredentialError::IllegalCharacters {
                byte_index,
            }) => {
                return Err(Error::config(format!(
                    "Invalid API key in {env_name}: illegal character at byte {byte_index}"
                )));
            }
            Err(crate::credential::CredentialError::UnknownSource {
                source_name,
            }) => {
                return Err(Error::config(format!(
                    "Invalid API key in {env_name}: unknown source {source_name}"
                )));
            }
        };
        Ok(Sensitive::new(trimmed))
    }

    /// Construct the underlying [`ModelProvider`] implementation.
    ///
    /// Resolves the API key from the environment (if the profile
    /// requires one), then constructs the backend-specific provider.
    /// The returned `Box<dyn ModelProvider>` is the same handle the
    /// legacy `WorkspaceConfig::create_provider` returns.
    pub fn build_provider(&self) -> Result<Box<dyn ModelProvider>, Error> {
        match self {
            Self::Stub(_) => Ok(Box::new(ModelProviderStub::new())),
            #[cfg(feature = "openai")]
            Self::OpenAI(p) => {
                let api_key = self.resolve_api_key()?;
                Ok(self.openai_provider(p, api_key.inner()))
            }
            #[cfg(not(feature = "openai"))]
            Self::OpenAI(_) => {
                Err(crate::config::adapter_not_compiled("openai"))
            }
            #[cfg(feature = "anthropic")]
            Self::Anthropic(p) => {
                let api_key = self.resolve_api_key()?;
                Ok(self.anthropic_provider(p, api_key.inner()))
            }
            #[cfg(not(feature = "anthropic"))]
            Self::Anthropic(_) => {
                Err(crate::config::adapter_not_compiled("anthropic"))
            }
        }
    }

    /// Construct the underlying provider without touching the
    /// environment — useful for tests and for lib consumers who
    /// manage keys outside of `std::env::var`.
    pub fn build_provider_with_key(
        &self,
        api_key: &str,
    ) -> Result<Box<dyn ModelProvider>, Error> {
        // No `cfg(not(any(anthropic, openai)))` short-circuit:
        // when neither adapter is compiled, the per-variant
        // `#[cfg(not(feature = "..."))]` arms below still fire and
        // produce the variant-named `adapter_not_compiled` error
        // the cfg-gated tests assert on.
        //
        // With no adapter compiled the key authenticates nothing,
        // so it is deliberately unused in that configuration.
        #[cfg(not(any(feature = "anthropic", feature = "openai")))]
        let _ = api_key;
        match self {
            Self::Stub(_) => Ok(Box::new(ModelProviderStub::new())),
            #[cfg(feature = "openai")]
            Self::OpenAI(p) => Ok(self.openai_provider(p, api_key)),
            #[cfg(not(feature = "openai"))]
            Self::OpenAI(_) => {
                Err(crate::config::adapter_not_compiled("openai"))
            }
            #[cfg(feature = "anthropic")]
            Self::Anthropic(p) => Ok(self.anthropic_provider(p, api_key)),
            #[cfg(not(feature = "anthropic"))]
            Self::Anthropic(_) => {
                Err(crate::config::adapter_not_compiled("anthropic"))
            }
        }
    }

    /// Build the OpenAI-compatible adapter for `p`.
    ///
    /// One construction site for both `build_provider` and
    /// `build_provider_with_key`, so the two paths cannot drift.
    #[cfg(feature = "openai")]
    fn openai_provider(
        &self,
        p: &OpenAIProfile,
        api_key: &str,
    ) -> Box<dyn ModelProvider> {
        Box::new(
            OpenAICompatibleProvider::new(
                p.base_url.clone(),
                self.model_config(),
            )
            .with_api_key(api_key),
        )
    }

    /// Build the Anthropic adapter for `p`, applying the optional
    /// proxy `base_url`.
    #[cfg(feature = "anthropic")]
    fn anthropic_provider(
        &self,
        p: &AnthropicProfile,
        api_key: &str,
    ) -> Box<dyn ModelProvider> {
        let mut provider =
            AnthropicProvider::new(self.model_config()).with_api_key(api_key);
        if let Some(base_url) = &p.base_url {
            provider = provider.with_base_url(base_url);
        }
        Box::new(provider)
    }
}
