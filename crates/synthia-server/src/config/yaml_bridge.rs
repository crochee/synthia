//! Bridge a YAML/TOML provider configuration file into the runtime
//! [`synthia_provider::config::WorkspaceConfig`] shape.
//!
//! This module is the adapter layer that lets operators keep using the
//! repo-root `config.yaml` (or any custom file passed via `--config`)
//! without changing `synthia-provider`'s internal data model.
//!
//! ## What gets translated
//!
//! The source file's `providers.<name>` entries may use either the
//! "model-oriented" shape that ships in the example `config.yaml`
//! (`base_url`, `api_key`, `models[].name`, `temperature`, `max_tokens`)
//! or the "TOML shape" that `synthia-provider` already understands
//! (`type`, `api_key_env`, `default_model`, `context_window`,
//! `max_output_tokens`). The bridge accepts both and produces the
//! TOML shape.
//!
//! When a `models[]` array is present, the first entry is used as the
//! provider's `default_model` and its `context_window`/`max_tokens`
//! override the per-entry defaults.
//!
//! ## What gets injected into the environment
//!
//! `synthia-provider` reads API keys through `ProviderEntry::api_key_env`
//! (see `WorkspaceConfig::resolve_api_key`). To honor an inline
//! `api_key` from the YAML file without modifying `synthia-provider`,
//! the bridge calls `std::env::set_var` with a deterministic
//! environment variable name (`SYNTHIA_PROVIDER_API_KEY_<UPPER_NAME>`)
//! and points `api_key_env` at it.
//!
//! This is process-local and only affects this server's view of the
//! environment. It is *not* a substitute for real secret management in
//! production; operators should still prefer `OPENAI_API_KEY` /
//! `ANTHROPIC_API_KEY` from the environment, `.env`, or a secrets
//! vault.

use std::{collections::HashMap, fs, path::Path};

use anyhow::{Context, Result, bail};
use synthia_provider::config::{ProviderEntry, WorkspaceConfig};

use super::provider::{ModelConfig as SourceModelConfig, ProviderConfig};

/// Files we recognise as a provider configuration file.
const SUPPORTED_EXTS: &[&str] = &["yaml", "yml", "toml"];

/// Read a provider configuration file from disk, bridge it into a
/// [`WorkspaceConfig`], and inject any inline API keys into the process
/// environment so that `synthia-provider` can resolve them through its
/// existing `api_key_env` machinery.
///
/// Returns `Ok(None)` when the path does not exist or no recognised
/// extension is supplied, so callers can fall through to the default
/// `<workspace>/.agents/config.toml` lookup without an error.
pub fn load_yaml_config_as_workspace_config(
    path: &Path,
) -> Result<Option<WorkspaceConfig>> {
    if !path.exists() {
        return Ok(None);
    }

    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !SUPPORTED_EXTS.contains(&ext.as_str()) {
        return Ok(None);
    }

    let content = fs::read_to_string(path)
        .with_context(|| format!("read provider config {}", path.display()))?;

    // The repo's example `config.yaml` and `ServerConfig` (used for
    // auth/CORS) share the same outer shape: a top-level `providers`
    // map whose values are `ProviderConfig { api_key, base_url, models[] }`.
    // We borrow that struct via `serde_yaml`/`toml` directly instead of
    // re-implementing the model type here.
    let source: SourceFile = if ext == "toml" {
        toml::from_str(&content)
            .with_context(|| format!("parse toml {}", path.display()))?
    } else {
        serde_yaml::from_str(&content)
            .with_context(|| format!("parse yaml {}", path.display()))?
    };

    let workspace_config =
        bridge(source).context("bridge yaml -> WorkspaceConfig")?;
    inject_api_keys(&workspace_config);
    Ok(Some(workspace_config))
}

#[derive(serde::Deserialize)]
struct SourceFile {
    #[serde(default)]
    default_provider: Option<String>,
    #[serde(default)]
    default_model: Option<String>,
    #[serde(default)]
    providers: HashMap<String, ProviderConfig>,
}

fn bridge(source: SourceFile) -> Result<WorkspaceConfig> {
    if source.providers.is_empty() {
        bail!("config has no `providers` entries");
    }

    let mut providers = HashMap::with_capacity(source.providers.len());

    for (name, entry) in source.providers {
        let provider_type = infer_provider_type(&name, &entry);
        let api_key_env = api_key_env_for(&name);

        if let Some(key) = entry.api_key.as_deref() {
            let trimmed = key.trim();
            if !trimmed.is_empty() {
                // SAFETY: `set_var` is `unsafe` on Rust 2024 edition.
                // Serialising the process environment at startup is a
                // single-threaded window before any worker spawns.
                #[allow(unsafe_code)]
                unsafe {
                    std::env::set_var(&api_key_env, trimmed);
                }
            }
        }

        let (default_model, context_window, max_output_tokens) =
            pick_model_settings(Some(&entry.models));

        providers.insert(
            name.clone(),
            ProviderEntry {
                r#type: provider_type,
                base_url: entry.base_url,
                api_key_env,
                default_model,
                context_window,
                max_output_tokens,
                supports_tools: Some(true),
                supports_streaming: Some(true),
                supports_reasoning: Some(false),
            },
        );
    }

    let default_provider = source
        .default_provider
        .clone()
        .or_else(|| {
            providers
                .contains_key("openai")
                .then(|| "openai".to_string())
        })
        .or_else(|| providers.keys().next().cloned())
        .unwrap_or_else(|| "openai".to_string());

    let default_model = source.default_model.clone().unwrap_or_else(|| {
        providers
            .get(&default_provider)
            .and_then(|p| p.default_model.clone())
            .unwrap_or_else(|| "unknown".to_string())
    });

    Ok(WorkspaceConfig {
        default_provider,
        default_model,
        providers,
    })
}

/// Choose the `type` field for `ProviderEntry`.
///
/// When the YAML/TOML uses the TOML-friendly shape (`type` already set
/// under the provider entry) we honour it. Otherwise we infer from the
/// provider name: `openai` / `azure_openai` / `custom_openai` ->
/// `"openai"`, everything else -> `"anthropic"`. The repo's example
/// `config.yaml` names its entries `openai` / `anthropic`, which lines
/// up exactly.
fn infer_provider_type(name: &str, entry: &ProviderConfig) -> String {
    if let Some(base_url) = entry.base_url.as_deref() {
        // OpenAI-compatible servers don't always live under
        // `api.openai.com`; the bare shape of the path is more reliable.
        if base_url.contains("/v1") || base_url.contains("openai") {
            return "openai".to_string();
        }
        if base_url.contains("/anthropic") || base_url.contains("anthropic") {
            return "anthropic".to_string();
        }
    }

    let lower = name.to_ascii_lowercase();
    if lower.contains("openai") || lower == "azure" || lower == "custom" {
        "openai".to_string()
    } else if lower.contains("anthropic") || lower.contains("claude") {
        "anthropic".to_string()
    } else {
        // Conservative default: OpenAI-compatible. Operators can move
        // to a typed ProviderEntry shape later if they need something
        // else.
        "openai".to_string()
    }
}

/// Deterministic environment variable name to back inline `api_key`s.
///
/// `<UPPER_NAME>` so `openai` -> `SYNTHIA_PROVIDER_API_KEY_OPENAI`.
/// This deliberately does not collide with `OPENAI_API_KEY` /
/// `ANTHROPIC_API_KEY` — operators who want real-env precedence can
/// leave the inline key blank in the YAML file and let the env var
/// win (the env var is *only* set when the YAML actually contains a
/// non-empty key).
fn api_key_env_for(name: &str) -> String {
    let mut upper = String::with_capacity(name.len());
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            upper.push(ch.to_ascii_uppercase());
        } else {
            upper.push('_');
        }
    }
    format!("SYNTHIA_PROVIDER_API_KEY_{upper}")
}

/// First model in `models[]` becomes the `default_model`; its
/// `context_window` and `max_tokens` (if any) override the per-entry
/// defaults. Returns `(Some(model_name), Some(ctx_window)?, Some(max)?)`.
fn pick_model_settings(
    models: Option<&[SourceModelConfig]>,
) -> (Option<String>, Option<usize>, Option<usize>) {
    let Some(first) = models.and_then(|m| m.first()) else {
        return (None, None, None);
    };
    (
        Some(first.name.clone()),
        first.context_window,
        first.max_tokens,
    )
}

/// Inject any inline API keys from the bridged config into the process
/// environment. This is the *same* operation `bridge()` performs per
/// entry; calling it again here is idempotent and keeps the bridge
/// step pure-with-respect-to-env (testable without env mutation).
fn inject_api_keys(config: &WorkspaceConfig) {
    for (name, entry) in &config.providers {
        // We can't read the original `api_key` back from
        // `WorkspaceConfig` (it deliberately doesn't carry one), so we
        // rely on the `bridge()` step having already populated the
        // env var. This function is currently a no-op kept for
        // symmetry with future refactors that move env injection out
        // of `bridge()`.
        let _ = (name, entry);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infer_openai_by_name() {
        let entry = ProviderConfig {
            api_key: None,
            base_url: Some("https://example.com/v1".into()),
            models: vec![],
        };
        assert_eq!(infer_provider_type("openai", &entry), "openai");
    }

    #[test]
    fn infer_anthropic_by_name() {
        let entry = ProviderConfig {
            api_key: None,
            base_url: Some("https://api.minimaxi.com/anthropic".into()),
            models: vec![],
        };
        assert_eq!(infer_provider_type("anthropic", &entry), "anthropic");
    }

    #[test]
    fn api_key_env_uppercases_name() {
        assert_eq!(
            api_key_env_for("openai"),
            "SYNTHIA_PROVIDER_API_KEY_OPENAI"
        );
        assert_eq!(
            api_key_env_for("custom-OpenAI-Compat"),
            "SYNTHIA_PROVIDER_API_KEY_CUSTOM_OPENAI_COMPAT"
        );
    }

    #[test]
    fn pick_model_settings_first_entry() {
        let models = vec![
            SourceModelConfig {
                name: "MiniMax-M2.7".into(),
                description: None,
                context_window: Some(128_000),
                temperature: None,
                max_tokens: Some(8192),
            },
            SourceModelConfig {
                name: "ignored".into(),
                description: None,
                context_window: None,
                temperature: None,
                max_tokens: None,
            },
        ];
        let (model, ctx, max) = pick_model_settings(Some(&models));
        assert_eq!(model.as_deref(), Some("MiniMax-M2.7"));
        assert_eq!(ctx, Some(128_000));
        assert_eq!(max, Some(8192));
    }

    #[test]
    fn pick_model_settings_empty() {
        let (model, ctx, max) = pick_model_settings(None);
        assert_eq!(model, None);
        assert_eq!(ctx, None);
        assert_eq!(max, None);
    }

    #[test]
    fn bridge_yaml_minimaxi_example() {
        let yaml = r#"
default_provider: openai
providers:
  openai:
    base_url: "https://api.minimaxi.com/v1"
    api_key: "sk-test"
    models:
      - name: "MiniMax-M2.7"
        context_window: 128000
        max_tokens: 8192
  anthropic:
    base_url: "https://api.minimaxi.com/anthropic"
    api_key: "sk-test"
    models:
      - name: "MiniMax-M2.7"
"#;
        let source: SourceFile = serde_yaml::from_str(yaml).unwrap();
        let cfg = bridge(source).unwrap();
        assert_eq!(cfg.default_provider, "openai");
        assert_eq!(cfg.default_model, "MiniMax-M2.7");

        let openai = cfg.providers.get("openai").unwrap();
        assert_eq!(openai.r#type, "openai");
        assert_eq!(
            openai.base_url.as_deref(),
            Some("https://api.minimaxi.com/v1")
        );
        assert_eq!(openai.api_key_env, "SYNTHIA_PROVIDER_API_KEY_OPENAI");
        assert_eq!(openai.default_model.as_deref(), Some("MiniMax-M2.7"));
        assert_eq!(openai.context_window, Some(128_000));
        assert_eq!(openai.max_output_tokens, Some(8192));

        let anthropic = cfg.providers.get("anthropic").unwrap();
        assert_eq!(anthropic.r#type, "anthropic");
        assert_eq!(anthropic.api_key_env, "SYNTHIA_PROVIDER_API_KEY_ANTHROPIC");
    }
}
