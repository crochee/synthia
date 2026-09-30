//! Typed provider profile — the lego-compatible configuration surface
//! for `synthia_provider`.
//!
//! ## Motivation
//!
//! [`WorkspaceConfig`](crate::config::WorkspaceConfig) — the historical
//! on-disk provider config — keys providers by a string in a
//! `HashMap<String, ProviderEntry>` and dispatches on the entry's
//! `r#type: String`. A lib consumer writing
//!
//! ```ignore
//! let cfg = WorkspaceConfig::default();
//! let provider = cfg.create_provider("openai")?;
//! ```
//!
//! gets no compile-time feedback if `"openai"` is misspelled, if the
//! entry's `r#type` is something the `match` statement doesn't yet
//! recognise, or if a feature (e.g. `supports_reasoning`) is absent.
//!
//! [`ProviderProfile`] is the typed alternative: a closed `enum` whose
//! variants carry exactly the configuration their backend needs.
//! lib consumers write
//!
//! ```ignore
//! use synthia_provider::{ProviderProfile, OpenAIProfile, AnthropicProfile};
//!
//! let profile = ProviderProfile::Anthropic(AnthropicProfile::claude_sonnet_4());
//! let provider: Box<dyn ModelProvider> = profile.build_provider()?;
//! ```
//!
//! and the type system carries the entire decision:
//!
//! - spelling — `ProviderProfile::OpenAI` vs `…::Anthropic` is a token,
//!   not a string lookup;
//! - per-profile defaults — `AnthropicProfile::claude_sonnet_4()`
//!   carries the right `context_window = 200_000` and
//!   `supports_reasoning = true`;
//! - error surface — `build_provider` returns a typed
//!   [`synthia_core::Error`] (missing API key, unknown model) instead
//!   of panicking on a string mismatch.
//!
//! ## Layering
//!
//! ```text
//! lib consumer
//!   │
//!   ▼
//! ProviderProfile::build_provider() ─▶ ModelProvider (Box<dyn>)
//!                                       │
//!                                       ▼
//!                            OpenAICompatibleProvider / AnthropicProvider /
//!                            ModelProviderStub / custom
//! ```
//!
//! ## Backward compatibility
//!
//! `ProviderProfile::from_entry(&ProviderEntry)` and
//! `ProviderEntry::from_profile(&profile)` round-trip the legacy
//! config-file shape so existing `.agents/config.toml` files keep
//! loading. The `WorkspaceConfig::create_provider` path is preserved
//! unchanged for the on-disk-config workflow.
//!
//! ## Reference
//!
//! Adopted from dsh `packages/llm/llm/src/profile.ts` (preset
//! pattern): a single typed enum that the consumer selects from
//! and the builder uses to construct the underlying `ModelProvider`.
//!
//! ## Module layout
//!
//! | Module | Responsibility |
//! |---|---|
//! | `capabilities` | [`ModelCapabilities`] — the shared flags struct + defaults. |
//! | `openai` | [`OpenAIProfile`] + helpers. |
//! | `anthropic` | [`AnthropicProfile`] + helpers. |
//! | `stub` | [`StubProfile`] for offline tests and tutorials. |
//! | `enum_` | [`ProviderProfile`] (the closed enum) + the `build_*` / `resolve_*` surface. |
//! | `registry` | [`ProviderRegistry`] — the in-memory named-profile map. |

mod anthropic;
mod capabilities;
mod enum_;
mod openai;
mod registry;
mod stub;

pub use anthropic::AnthropicProfile;
pub use capabilities::{
    DEFAULT_ANTHROPIC_CONTEXT_WINDOW,
    DEFAULT_ANTHROPIC_MAX_OUTPUT_TOKENS,
    DEFAULT_OPENAI_CONTEXT_WINDOW,
    DEFAULT_OPENAI_MAX_OUTPUT_TOKENS,
    ModelCapabilities,
};
pub use enum_::ProviderProfile;
pub use openai::OpenAIProfile;
pub use registry::ProviderRegistry;
pub use stub::StubProfile;

#[cfg(test)]
mod tests {
    use serde_json;

    use super::*;

    #[test]
    fn openai_default_profile_resolves_kind_and_model() {
        let p = ProviderProfile::OpenAI(OpenAIProfile::openai_default());
        assert_eq!(p.kind(), "openai");
        assert_eq!(p.name(), "openai");
        assert_eq!(p.default_model(), "gpt-4o");
        assert_eq!(p.api_key_env(), Some("OPENAI_API_KEY"));
    }

    #[test]
    fn anthropic_default_profile_resolves_kind_and_model() {
        let p = ProviderProfile::Anthropic(AnthropicProfile::claude_sonnet_4());
        assert_eq!(p.kind(), "anthropic");
        assert_eq!(p.default_model(), "claude-sonnet-4-20250514");
        assert_eq!(p.api_key_env(), Some("ANTHROPIC_API_KEY"));
    }

    #[test]
    fn stub_profile_has_no_api_key_env() {
        let p = ProviderProfile::Stub(StubProfile::text_only());
        assert_eq!(p.kind(), "stub");
        assert_eq!(p.api_key_env(), None);
        // Stubs always build successfully regardless of env.
        let provider = p.build_provider().expect("stub provider builds");
        let cfg = provider.model_config();
        assert_eq!(cfg.provider, "stub");
    }

    #[test]
    fn model_config_carries_capability_overrides() {
        let caps = ModelCapabilities {
            supports_tools: true,
            supports_streaming: false,
            supports_reasoning: false,
            max_output_tokens: 1024,
            context_window: 4096,
        };
        let p = ProviderProfile::OpenAI(
            OpenAIProfile::openai_mini().with_capabilities(caps.clone()),
        );
        let cfg = p.model_config();
        assert_eq!(cfg.context_window, 4096);
        assert_eq!(cfg.max_output_tokens, 1024);
        assert!(!cfg.supports_streaming);
        assert!(!cfg.supports_reasoning);
    }

    #[cfg(feature = "openai")]
    #[test]
    fn build_provider_with_key_avoids_env_lookup() {
        // No OPENAI_API_KEY in the test env; with_key bypasses the
        // lookup and the provider still builds.
        // SAFETY: tests are single-threaded with no other env readers.
        unsafe { std::env::remove_var("OPENAI_API_KEY") };
        let p = ProviderProfile::OpenAI(OpenAIProfile::openai_default());
        let provider = p.build_provider_with_key("test-key");
        assert!(provider.is_ok());
    }

    #[cfg(feature = "openai")]
    #[test]
    fn build_provider_missing_api_key_returns_typed_error() {
        // SAFETY: tests are single-threaded with no other env readers.
        unsafe { std::env::remove_var("OPENAI_API_KEY") };
        let p = ProviderProfile::OpenAI(OpenAIProfile::openai_default());
        let result = p.build_provider();
        assert!(result.is_err(), "build_provider must error without key");
        let err = result.err().unwrap();
        let formatted = format!("{err:?}");
        assert!(
            formatted.contains("OPENAI_API_KEY"),
            "error must name the env var, got: {formatted}"
        );
    }

    /// A build without the `openai` adapter must name the feature to
    /// enable: the profile is valid data, the binary just has no
    /// adapter for it.
    #[cfg(not(feature = "openai"))]
    #[test]
    fn openai_profile_without_the_adapter_names_the_feature() {
        let p = ProviderProfile::OpenAI(OpenAIProfile::openai_default());
        let Err(err) = p.build_provider_with_key("test-key") else {
            panic!("a build without the `openai` adapter must not build one");
        };
        let formatted = format!("{err:?}");
        assert!(
            formatted.contains("enable the `openai` feature"),
            "error must name the feature, got: {formatted}"
        );
    }

    /// Same contract for the Anthropic adapter.
    #[cfg(not(feature = "anthropic"))]
    #[test]
    fn anthropic_profile_without_the_adapter_names_the_feature() {
        let p = ProviderProfile::Anthropic(AnthropicProfile::claude_sonnet_4());
        let Err(err) = p.build_provider_with_key("test-key") else {
            panic!(
                "a build without the `anthropic` adapter must not build one"
            );
        };
        let formatted = format!("{err:?}");
        assert!(
            formatted.contains("enable the `anthropic` feature"),
            "error must name the feature, got: {formatted}"
        );
    }

    #[test]
    fn provider_registry_insert_and_build_default() {
        let reg = ProviderRegistry::new()
            .insert(ProviderProfile::Anthropic(
                AnthropicProfile::claude_sonnet_4(),
            ))
            .insert(ProviderProfile::OpenAI(OpenAIProfile::openai_default()));
        // First inserted is the default.
        assert_eq!(reg.default_profile().unwrap().kind(), "anthropic");
        assert_eq!(
            reg.names(),
            vec!["anthropic".to_string(), "openai".to_string()]
        );
        assert_eq!(reg.len(), 2);
    }

    #[test]
    fn provider_registry_with_default_overrides() {
        let reg = ProviderRegistry::new()
            .insert(ProviderProfile::Anthropic(
                AnthropicProfile::claude_sonnet_4(),
            ))
            .insert(ProviderProfile::OpenAI(OpenAIProfile::openai_default()))
            .with_default("openai");
        assert_eq!(reg.default_profile().unwrap().kind(), "openai");
    }
    #[test]
    fn provider_registry_build_unknown_name_errors() {
        let reg = ProviderRegistry::new().insert(ProviderProfile::Anthropic(
            AnthropicProfile::claude_sonnet_4(),
        ));
        let result = reg.build("openai");
        assert!(result.is_err(), "build unknown name must error");
        let err = result.err().unwrap();
        let formatted = format!("{err:?}");
        assert!(
            formatted.contains("openai"),
            "error names the missing key, got: {formatted}"
        );
    }

    #[test]
    fn provider_registry_build_default_requires_set_default() {
        let reg = ProviderRegistry::new();
        let result = reg.build_default();
        assert!(result.is_err(), "empty registry must error");
        let err = result.err().unwrap();
        let formatted = format!("{err:?}");
        assert!(
            formatted.contains("no default profile"),
            "error names the empty-registry cause, got: {formatted}"
        );
    }

    #[test]
    fn anthropic_proxy_profile_carries_base_url() {
        let p = ProviderProfile::Anthropic(AnthropicProfile::anthropic_proxy(
            "bedrock-anthropic",
            "https://bedrock-runtime.us-east-1.amazonaws.com",
            "anthropic.claude-sonnet-4-20250514-v1:0",
        ));
        assert_eq!(p.name(), "bedrock-anthropic");
        // The profile's ModelConfig must project the model id verbatim.
        assert_eq!(
            p.model_config().name,
            "anthropic.claude-sonnet-4-20250514-v1:0"
        );
    }

    #[test]
    fn profile_serialization_roundtrip() {
        let p = ProviderProfile::OpenAI(OpenAIProfile::openai_default());
        let json = serde_json::to_string(&p).expect("serialize");
        let back: ProviderProfile =
            serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.kind(), p.kind());
        assert_eq!(back.default_model(), p.default_model());
    }

    #[test]
    fn frontier_chat_capabilities_match_default_openai_window() {
        let caps = ModelCapabilities::frontier_chat();
        assert_eq!(caps.context_window, DEFAULT_OPENAI_CONTEXT_WINDOW);
        assert_eq!(caps.max_output_tokens, DEFAULT_OPENAI_MAX_OUTPUT_TOKENS);
        assert!(caps.supports_tools);
        assert!(caps.supports_streaming);
        assert!(caps.supports_reasoning);
    }
}
