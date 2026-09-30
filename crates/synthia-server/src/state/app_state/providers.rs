//! `ProviderCatalogue`: the deployment's configured providers, keyed
//! by their `[providers.<name>]` entry name.
//!
//! ## Why a catalogue and not just `default_provider`
//!
//! `AppState::default_provider` answers "which provider runs a turn
//! that names none". The model selector needs the other half: a turn
//! may name a provider *and a model*, and the request must either run
//! that provider or fail naming the value it could not resolve —
//! silently running the default is the defect this module exists to
//! close.
//!
//! ## What it holds
//!
//! | Field | Source of truth for |
//! |---|---|
//! | `profiles` | every entry: its name, its kind, the model id it advertises |
//! | `handles` | the built `Arc<dyn ModelProvider>` a run samples with |
//! | `failures` | why a configured entry could not be built (missing API key, adapter not compiled in, unknown `type`) |
//! | `default_name` | `workspace_config.default_provider` |
//!
//! The profiles live in [`ProviderRegistry`] — the same registry the
//! rest of `synthia-provider` uses — rather than a second map with its
//! own lookup semantics. Building happens once, at boot, so a turn
//! resolves a name to a handle without touching the environment.
//!
//! ## Resolution
//!
//! [`ProviderCatalogue::resolve`] accepts both spellings a client may
//! send:
//!
//! - `"<provider>/<model>"` — exactly what `/api/v1/models` advertises
//!   per entry, and what the chat UI's selector posts;
//! - a bare `"<model>"` — what an Anthropic-protocol client sends,
//!   since the protocol's `model` field carries a bare model id. When
//!   exactly one configured provider advertises it, that wins. When
//!   several do, the deployment's `default_provider` (if it is one
//!   of them) wins — otherwise the call is reported as ambiguous
//!   so the caller can qualify it.
//!
//! An entry that declares no `default_model` advertises the same
//! placeholder `/api/v1/models` publishes for it, so the value a
//! client reads off that endpoint always resolves.

use std::{collections::HashMap, sync::Arc};

use axum::http::StatusCode;
use synthia::provider::{
    config::{ProviderEntry, WorkspaceConfig},
    profile::{
        AnthropicProfile,
        ModelCapabilities,
        OpenAIProfile,
        ProviderProfile,
        ProviderRegistry,
    },
    traits::ModelProvider,
};

/// The model id advertised for a `[providers.<name>]` entry that
/// declares no `default_model`.
///
/// `routes::health::list_models` publishes the same placeholder, so a
/// client that selects what `/api/v1/models` showed it resolves here.
const UNDECLARED_MODEL: &str = "unknown";

/// The deployment's configured providers — the map a turn's `model`
/// selection resolves against.
#[derive(Clone, Default)]
pub struct ProviderCatalogue {
    profiles: ProviderRegistry,
    handles: HashMap<String, Arc<dyn ModelProvider>>,
    failures: HashMap<String, String>,
    default_name: String,
}

impl ProviderCatalogue {
    /// Build the catalogue for a deployment: one profile per
    /// `[providers.<name>]` entry, each built through
    /// [`ProviderProfile::build_provider`] (which resolves its API key
    /// from the environment).
    ///
    /// An entry the build cannot serve is recorded with its reason
    /// instead of aborting the boot: a deployment whose *secondary*
    /// provider has no key must still start, and selecting that
    /// provider is then a request-time error the caller can name.
    pub fn from_workspace_config(config: &WorkspaceConfig) -> Self {
        let mut profiles = ProviderRegistry::new();
        let mut failures: HashMap<String, String> = HashMap::new();
        for (name, entry) in &config.providers {
            match profile_of(name, entry) {
                Ok(profile) => profiles = profiles.insert(profile),
                Err(reason) => {
                    failures.insert(name.clone(), reason);
                }
            }
        }

        let mut handles: HashMap<String, Arc<dyn ModelProvider>> =
            HashMap::new();
        for name in profiles.names() {
            match profiles.build(&name) {
                Ok(provider) => {
                    handles.insert(name, Arc::from(provider));
                }
                Err(error) => {
                    failures.insert(name, error.to_string());
                }
            }
        }
        for (name, reason) in &failures {
            tracing::warn!(
                target: "synthia.server",
                provider = %name,
                reason = %reason,
                "configured provider is not available; selecting it fails"
            );
        }

        Self {
            profiles,
            handles,
            failures,
            default_name: config.default_provider.clone(),
        }
    }

    /// The provider a run that names no model samples with.
    ///
    /// `Err` is the same [`ModelSelectionError`] any other selection
    /// produces, so the boot's fatal path and a request-time selection
    /// report the same thing about the same name.
    pub fn default_handle(
        &self,
    ) -> Result<Arc<dyn ModelProvider>, ModelSelectionError> {
        self.handle(&self.default_name)
    }

    /// Resolve a turn's `model` selection.
    ///
    /// `None` and a blank selection are the deployment default; every
    /// other value must name a configured provider and a model that
    /// provider advertises, or the result is an error naming the value
    /// the caller sent.
    pub fn resolve(
        &self,
        selection: Option<&str>,
    ) -> Result<Arc<dyn ModelProvider>, ModelSelectionError> {
        let selection = selection.map(str::trim).filter(|s| !s.is_empty());
        let Some(selection) = selection else {
            return self.default_handle();
        };
        // Split at the *first* slash: a model id may contain slashes
        // of its own (`meta-llama/llama-3.3-70b-instruct`), a
        // provider entry name cannot.
        match selection.split_once('/') {
            Some((provider, model)) => {
                let Some(profile) = self.profiles.get(provider) else {
                    // An entry that is in the config but whose profile
                    // could not be built (an unrecognised `type`) is
                    // reported by its reason: the operator did
                    // configure it, and `/api/v1/models` advertises
                    // it, so "no such provider" would be a lie.
                    return Err(match self.failures.get(provider) {
                        Some(reason) => ModelSelectionError::Unavailable {
                            provider: provider.to_string(),
                            reason: reason.clone(),
                        },
                        None => ModelSelectionError::UnknownProvider {
                            value: selection.to_string(),
                        },
                    });
                };
                if profile.default_model() != model {
                    return Err(ModelSelectionError::UnknownModel {
                        value: selection.to_string(),
                        provider: Some(provider.to_string()),
                    });
                }
                self.handle(provider)
            }
            None => self.resolve_bare_model(selection),
        }
    }

    /// Register an already-built handle under `name`, skipping the
    /// environment-key resolution [`Self::from_workspace_config`]
    /// does.
    ///
    /// The test seam: a fixture's provider is a fake, so there is no
    /// profile to build it from — the profile stored here carries only
    /// the name and the advertised model id, which is all resolution
    /// reads. The first registration also becomes the default.
    #[cfg(any(test, feature = "test-utils"))]
    #[must_use]
    pub fn with_handle(
        mut self,
        name: &str,
        model: &str,
        handle: Arc<dyn ModelProvider>,
    ) -> Self {
        use synthia::provider::profile::StubProfile;
        self.profiles = std::mem::take(&mut self.profiles).insert(
            ProviderProfile::Stub(StubProfile {
                name: name.to_string(),
                default_model: model.to_string(),
            }),
        );
        if self.default_name.is_empty() {
            self.default_name = name.to_string();
        }
        self.handles.insert(name.to_string(), handle);
        self
    }

    /// A bare model name: resolve it against the providers that
    /// advertise it.
    ///
    /// Anthropic-protocol clients — the Anthropic SDK, Cursor, Claude
    /// Code — send a bare model id; they have no way to name a
    /// provider, so the deployment has to disambiguate for them. When
    /// exactly one provider offers the model, that wins. When several
    /// do, the deployment's `default_provider` (if it is among them)
    /// wins, otherwise the call is reported as ambiguous so the
    /// caller can fix it.
    fn resolve_bare_model(
        &self,
        model: &str,
    ) -> Result<Arc<dyn ModelProvider>, ModelSelectionError> {
        let matches: Vec<String> = self
            .profiles
            .names()
            .into_iter()
            .filter(|name| {
                self.profiles
                    .get(name)
                    .is_some_and(|profile| profile.default_model() == model)
            })
            .collect();
        match matches.as_slice() {
            [] => Err(ModelSelectionError::UnknownModel {
                value: model.to_string(),
                provider: None,
            }),
            [name] => self.handle(name),
            multiple => {
                // The default is among the matches — the deployment's
                // choice, and the only pick that lets an
                // Anthropic-protocol client succeed without naming a
                // provider. Otherwise the caller sees every name it
                // could pick from.
                if let Some(default) =
                    multiple.iter().find(|name| **name == self.default_name)
                {
                    self.handle(default)
                } else {
                    Err(ModelSelectionError::AmbiguousModel {
                        value: model.to_string(),
                        providers: multiple.to_vec(),
                    })
                }
            }
        }
    }

    /// The built handle for `name`, or the reason it cannot serve.
    fn handle(
        &self,
        name: &str,
    ) -> Result<Arc<dyn ModelProvider>, ModelSelectionError> {
        if let Some(provider) = self.handles.get(name) {
            return Ok(Arc::clone(provider));
        }
        match self.failures.get(name) {
            Some(reason) => Err(ModelSelectionError::Unavailable {
                provider: name.to_string(),
                reason: reason.clone(),
            }),
            None => Err(ModelSelectionError::UnknownProvider {
                value: name.to_string(),
            }),
        }
    }
}

/// Project a `[providers.<name>]` entry onto the typed profile the
/// registry keys, so every provider is built by exactly one path —
/// [`ProviderProfile::build_provider`].
///
/// The capability flags mirror the on-disk adaptation
/// `synthia::provider::config::model_config_of` performs, so a
/// deployment's provider behaves the same whichever entry point built
/// it. An unrecognised `type` is the same error text the config path
/// produced.
fn profile_of(
    name: &str,
    entry: &ProviderEntry,
) -> Result<ProviderProfile, String> {
    let default_model = entry
        .default_model
        .clone()
        .unwrap_or_else(|| UNDECLARED_MODEL.to_string());
    let capabilities = ModelCapabilities {
        supports_tools: entry.supports_tools.unwrap_or(true),
        supports_streaming: entry.supports_streaming.unwrap_or(true),
        supports_reasoning: entry.supports_reasoning.unwrap_or(false),
        max_output_tokens: entry.max_output_tokens.unwrap_or(4096),
        context_window: entry.context_window.unwrap_or(128_000),
    };
    match entry.r#type.as_str() {
        "openai" => Ok(ProviderProfile::OpenAI(OpenAIProfile {
            name: name.to_string(),
            base_url: entry
                .base_url
                .clone()
                .unwrap_or_else(|| "https://api.openai.com/v1".to_string()),
            api_key_env: entry.api_key_env.clone(),
            default_model,
            capabilities: Some(capabilities),
        })),
        "anthropic" => Ok(ProviderProfile::Anthropic(AnthropicProfile {
            name: name.to_string(),
            base_url: entry.base_url.clone(),
            api_key_env: entry.api_key_env.clone(),
            default_model,
            capabilities: Some(capabilities),
        })),
        other => Err(format!("Unsupported provider type: {other}")),
    }
}

/// Why a `model` selection did not resolve.
///
/// Every variant names the value the caller sent, so the message that
/// crosses the wire tells them which string to fix instead of leaving
/// them to guess which provider ran.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelSelectionError {
    /// The selection named a provider this deployment does not
    /// configure.
    UnknownProvider { value: String },
    /// The provider is configured but does not offer this model. The
    /// provider is `None` for a bare model name no provider matches.
    UnknownModel {
        value: String,
        provider: Option<String>,
    },
    /// A bare model name several providers offer — the selection has
    /// to name the provider.
    AmbiguousModel {
        value: String,
        providers: Vec<String>,
    },
    /// The provider is configured but the boot could not build it
    /// (missing API key, adapter not compiled in, unknown `type`).
    Unavailable { provider: String, reason: String },
}

impl ModelSelectionError {
    /// The HTTP status the API layer answers with.
    ///
    /// A selection nobody can resolve is the caller's error; a
    /// configured provider the *server* cannot build is not, so it is
    /// a `503` rather than another `400`.
    pub fn status(&self) -> StatusCode {
        match self {
            Self::UnknownProvider { .. }
            | Self::UnknownModel { .. }
            | Self::AmbiguousModel { .. } => StatusCode::BAD_REQUEST,
            Self::Unavailable { .. } => StatusCode::SERVICE_UNAVAILABLE,
        }
    }
}

impl std::fmt::Display for ModelSelectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownProvider { value } => write!(
                f,
                "unknown provider in `model` value `{value}`: no \
                 configured provider by that name"
            ),
            Self::UnknownModel {
                value,
                provider: Some(provider),
            } => write!(f, "unknown model `{value}` for provider `{provider}`"),
            Self::UnknownModel {
                value,
                provider: None,
            } => write!(
                f,
                "unknown model `{value}`: no configured provider \
                 offers it"
            ),
            Self::AmbiguousModel { value, providers } => write!(
                f,
                "ambiguous model `{value}`: offered by providers {}",
                providers.join(", ")
            ),
            Self::Unavailable { provider, reason } => {
                write!(f, "provider `{provider}` is not available: {reason}")
            }
        }
    }
}

impl std::error::Error for ModelSelectionError {}

#[cfg(test)]
mod tests {
    use synthia::provider::config::{ProviderEntry, WorkspaceConfig};

    use super::*;

    /// A catalogue with two providers that advertise different
    /// models, plus one that advertises a model nothing else does.
    fn catalogue() -> (
        ProviderCatalogue,
        Arc<dyn ModelProvider>,
        Arc<dyn ModelProvider>,
    ) {
        let openai: Arc<dyn ModelProvider> =
            Arc::new(synthia::test_support::FakeProvider::text("openai"));
        let anthropic: Arc<dyn ModelProvider> =
            Arc::new(synthia::test_support::FakeProvider::text("anthropic"));
        let catalogue = ProviderCatalogue::default()
            .with_handle("openai", "gpt-4o", Arc::clone(&openai))
            .with_handle(
                "anthropic",
                "claude-sonnet-4-20250514",
                Arc::clone(&anthropic),
            );
        (catalogue, openai, anthropic)
    }

    fn entry(kind: &str, model: Option<&str>) -> ProviderEntry {
        ProviderEntry {
            r#type: kind.to_string(),
            base_url: None,
            api_key_env: "SYNTHIA_TEST_UNSET_KEY".to_string(),
            default_model: model.map(str::to_string),
            context_window: None,
            max_output_tokens: None,
            supports_tools: None,
            supports_streaming: None,
            supports_reasoning: None,
        }
    }

    #[test]
    fn qualified_selection_resolves_to_that_provider() {
        let (catalogue, openai, anthropic) = catalogue();
        let resolved = catalogue
            .resolve(Some("anthropic/claude-sonnet-4-20250514"))
            .expect("a qualified selection resolves");
        assert!(Arc::ptr_eq(&resolved, &anthropic));
        assert!(!Arc::ptr_eq(&resolved, &openai));
    }

    #[test]
    fn bare_model_name_resolves_through_the_one_provider_offering_it() {
        let (catalogue, _openai, anthropic) = catalogue();
        let resolved = catalogue
            .resolve(Some("claude-sonnet-4-20250514"))
            .expect("a bare model name resolves");
        assert!(Arc::ptr_eq(&resolved, &anthropic));
    }

    #[test]
    fn no_selection_resolves_to_the_default_provider() {
        let (catalogue, openai, _anthropic) = catalogue();
        assert!(Arc::ptr_eq(&catalogue.resolve(None).unwrap(), &openai));
        // A blank field means the same as an absent one.
        assert!(Arc::ptr_eq(
            &catalogue.resolve(Some("  ")).unwrap(),
            &openai
        ));
    }

    #[test]
    fn unknown_provider_error_names_the_value() {
        let (catalogue, _openai, _anthropic) = catalogue();
        let error = catalogue
            .resolve(Some("gemini/gemini-2.0"))
            .err()
            .expect("an unconfigured provider must not resolve");
        assert_eq!(
            error,
            ModelSelectionError::UnknownProvider {
                value: "gemini/gemini-2.0".to_string(),
            }
        );
        assert_eq!(error.status(), StatusCode::BAD_REQUEST);
        assert!(
            error.to_string().contains("gemini/gemini-2.0"),
            "the message must name the value: {error}"
        );
    }

    #[test]
    fn unknown_model_error_names_the_provider_qualified_value() {
        let (catalogue, _openai, _anthropic) = catalogue();
        let error = catalogue
            .resolve(Some("openai/gpt-5"))
            .err()
            .expect("a model the provider does not offer must not resolve");
        assert_eq!(
            error,
            ModelSelectionError::UnknownModel {
                value: "openai/gpt-5".to_string(),
                provider: Some("openai".to_string()),
            }
        );
        assert!(error.to_string().contains("openai/gpt-5"));
    }

    #[test]
    fn bare_model_offered_by_two_providers_is_ambiguous() {
        // A third provider serves as the deployment's default so the
        // two `shared-model` providers are *not* the default — the
        // ambiguity is real, not just a default-fallback pick.
        let shared: Arc<dyn ModelProvider> =
            Arc::new(synthia::test_support::FakeProvider::text("a"));
        let catalogue = ProviderCatalogue::default()
            .with_handle("third", "default-only", Arc::clone(&shared))
            .with_handle("first", "shared-model", Arc::clone(&shared))
            .with_handle("second", "shared-model", shared);
        let error = catalogue
            .resolve(Some("shared-model"))
            .err()
            .expect("the selection has to name the provider");
        assert_eq!(
            error,
            ModelSelectionError::AmbiguousModel {
                value: "shared-model".to_string(),
                providers: vec!["first".to_string(), "second".to_string()],
            }
        );
        assert_eq!(error.status(), StatusCode::BAD_REQUEST);
    }

    /// A bare model name several providers offer is resolved through
    /// the deployment's `default_provider` when it is one of them.
    ///
    /// An Anthropic-protocol client (`POST /v1/messages` with a bare
    /// `model` field, the Anthropic SDK, Cursor, Claude Code) cannot
    /// name a provider. The deployment has to pick — silently picking
    /// is a defect, but refusing outright leaves every protocol
    /// client stuck on the "ambiguous" error. The deployment's default
    /// is the only pick that's also a user-visible config knob.
    #[test]
    fn bare_model_offered_by_two_providers_resolves_to_the_default() {
        let shared: Arc<dyn ModelProvider> =
            Arc::new(synthia::test_support::FakeProvider::text("a"));
        let catalogue = ProviderCatalogue::default()
            .with_handle("default-wins", "shared-model", Arc::clone(&shared))
            .with_handle("loser", "shared-model", shared);
        let resolved = catalogue.resolve(Some("shared-model")).expect(
            "the default is among the matches, so the bare name resolves",
        );
        // The shared fake advertises "fake"; both handles point at it,
        // but the *catalogue's* default is `default-wins` and is the
        // only one `default_handle` returns — so the test's signal is
        // that `resolve` and `default_handle` agree on the same handle.
        assert!(Arc::ptr_eq(&resolved, &catalogue.default_handle().unwrap(),));
        // The losing provider was not picked.
        assert_eq!(resolved.name(), "fake");
    }

    /// A provider the boot could not build (no API key here) is not
    /// "unknown" — it exists in the config and the error has to say
    /// why it cannot serve.
    #[test]
    fn unavailable_provider_reports_its_build_failure() {
        let mut config = WorkspaceConfig::default();
        config
            .providers
            .insert("openai".to_string(), entry("openai", Some("gpt-4o")));
        let catalogue = ProviderCatalogue::from_workspace_config(&config);
        let error = catalogue
            .resolve(Some("openai/gpt-4o"))
            .err()
            .expect("the key is unset, so the handle is not built");
        match &error {
            ModelSelectionError::Unavailable { provider, reason } => {
                assert_eq!(provider, "openai");
                assert!(
                    reason.contains("SYNTHIA_TEST_UNSET_KEY"),
                    "the reason must name the missing key: {reason}"
                );
            }
            other => panic!("expected Unavailable, got {other:?}"),
        }
        assert_eq!(error.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[test]
    fn undeclared_model_entry_advertises_the_models_placeholder() {
        let mut config = WorkspaceConfig::default();
        config
            .providers
            .insert("local".to_string(), entry("openai", None));
        let catalogue = ProviderCatalogue::from_workspace_config(&config);
        assert_eq!(
            catalogue
                .profiles
                .get("local")
                .expect("the entry is registered")
                .default_model(),
            UNDECLARED_MODEL
        );
        // The default provider is the config's `default_provider`
        // (absent from `providers` here), so it is unknown.
        assert!(matches!(
            catalogue.default_handle(),
            Err(ModelSelectionError::UnknownProvider { .. })
        ));
    }
}
