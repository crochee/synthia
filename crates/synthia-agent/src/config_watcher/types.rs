//! Core data types — [`ConfigType`], [`SynthiaConfig`],
//! [`HotReloadableFields`], [`SharedConfig`], and the
//! [`ConfigChangeCallback`] type alias.
//!
//! [`SynthiaConfig`] owns the TOML load/validate logic
//! ([`SynthiaConfig::load_from_file`] + [`SynthiaConfig::validate`]).
//! [`HotReloadableFields`] owns the diff logic that the
//! debouncer and the manual `reload()` entry point both
//! call into.

use std::{path::Path, pin::Pin, sync::Arc};

use futures::Future;
use serde::{Deserialize, Serialize};
use synthia_core::Error;
use tokio::sync::RwLock;

/// Async callback invoked on every hot-reload event.
///
/// The `serde_json::Value` payload carries the changed field
/// names and the `config_type` (currently always `"main"`).
pub type ConfigChangeCallback = Arc<
    dyn Fn(serde_json::Value) -> Pin<Box<dyn Future<Output = ()> + Send>>
        + Send
        + Sync,
>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConfigType {
    Main,
    Provider,
    Skill,
    Permission,
    Mcp,
}

impl std::fmt::Display for ConfigType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigType::Main => write!(f, "main"),
            ConfigType::Provider => write!(f, "provider"),
            ConfigType::Skill => write!(f, "skill"),
            ConfigType::Permission => write!(f, "permission"),
            ConfigType::Mcp => write!(f, "mcp"),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SynthiaConfig {
    pub default_provider: String,
    pub default_model: String,
    pub token_budget: usize,
    pub compression_threshold: f64,
    pub max_iterations: usize,
    pub max_concurrent_tools: usize,
    pub loop_detection_threshold: usize,
    pub bm25_threshold: f64,
}

impl Default for SynthiaConfig {
    fn default() -> Self {
        Self {
            default_provider: "openai".to_string(),
            default_model: "gpt-4o".to_string(),
            token_budget: 128_000,
            compression_threshold: 0.85,
            max_iterations: 90,
            max_concurrent_tools: 5,
            loop_detection_threshold: 5,
            bm25_threshold: 0.3,
        }
    }
}

impl SynthiaConfig {
    pub fn validate(&self) -> Result<(), Error> {
        if self.default_provider.is_empty() {
            return Err(Error::Validation(
                "Field 'default_provider' must not be empty".into(),
            ));
        }
        if self.default_model.is_empty() {
            return Err(Error::Validation(
                "Field 'default_model' must not be empty".into(),
            ));
        }
        if self.token_budget == 0 {
            return Err(Error::Validation(
                "Field 'token_budget' must not be zero".into(),
            ));
        }
        if !(0.0..=1.0).contains(&self.compression_threshold) {
            return Err(Error::Validation(
                "Field 'compression_threshold' is out of valid range".into(),
            ));
        }
        if self.max_iterations == 0 {
            return Err(Error::Validation(
                "Field 'max_iterations' must not be zero".into(),
            ));
        }
        if self.max_concurrent_tools == 0 {
            return Err(Error::Validation(
                "Field 'max_concurrent_tools' must not be zero".into(),
            ));
        }
        if self.loop_detection_threshold == 0 {
            return Err(Error::Validation(
                "Field 'loop_detection_threshold' must not be zero".into(),
            ));
        }
        Ok(())
    }

    pub fn load_from_file(path: &Path) -> Result<Self, Error> {
        let content = std::fs::read_to_string(path).map_err(Error::Io)?;
        let raw: toml::Value = toml::from_str(&content)
            .map_err(|e| Error::Parse(format!("TOML parse error: {}", e)))?;
        let table = raw
            .as_table()
            .ok_or_else(|| Error::Parse("root is not a table".to_string()))?;

        let mut config = SynthiaConfig::default();

        if let Some(provider) = table.get("provider").and_then(|v| v.as_table())
        {
            if let Some(v) =
                provider.get("default_provider").and_then(|v| v.as_str())
            {
                config.default_provider = v.to_string();
            }
            if let Some(v) =
                provider.get("default_model").and_then(|v| v.as_str())
            {
                config.default_model = v.to_string();
            }
        }

        if let Some(context) = table.get("context").and_then(|v| v.as_table()) {
            if let Some(v) =
                context.get("token_budget").and_then(|v| v.as_integer())
            {
                config.token_budget = v as usize;
            }
            if let Some(v) = context
                .get("compression_threshold")
                .and_then(|v| v.as_float())
            {
                config.compression_threshold = v;
            }
        }

        if let Some(agent) = table.get("agent").and_then(|v| v.as_table()) {
            if let Some(v) =
                agent.get("max_iterations").and_then(|v| v.as_integer())
            {
                config.max_iterations = v as usize;
            }
            if let Some(v) = agent
                .get("max_concurrent_tools")
                .and_then(|v| v.as_integer())
            {
                config.max_concurrent_tools = v as usize;
            }
        }

        if let Some(guardian) = table.get("guardian").and_then(|v| v.as_table())
            && let Some(v) = guardian
                .get("loop_detection_threshold")
                .and_then(|v| v.as_integer())
        {
            config.loop_detection_threshold = v as usize;
        }

        if let Some(skill) = table.get("skill").and_then(|v| v.as_table())
            && let Some(v) =
                skill.get("bm25_threshold").and_then(|v| v.as_float())
        {
            config.bm25_threshold = v;
        }

        config.validate()?;
        Ok(config)
    }
}

#[derive(Clone, Debug, Default)]
pub struct HotReloadableFields {
    pub default_provider_changed: bool,
    pub default_model_changed: bool,
    pub token_budget_changed: bool,
    pub compression_threshold_changed: bool,
    pub max_iterations_changed: bool,
    pub max_concurrent_tools_changed: bool,
    pub loop_detection_threshold_changed: bool,
    pub bm25_threshold_changed: bool,
}

impl HotReloadableFields {
    pub fn diff(old: &SynthiaConfig, new: &SynthiaConfig) -> Self {
        Self {
            default_provider_changed: old.default_provider
                != new.default_provider,
            default_model_changed: old.default_model != new.default_model,
            token_budget_changed: old.token_budget != new.token_budget,
            compression_threshold_changed: (old.compression_threshold
                - new.compression_threshold)
                .abs()
                > f64::EPSILON,
            max_iterations_changed: old.max_iterations != new.max_iterations,
            max_concurrent_tools_changed: old.max_concurrent_tools
                != new.max_concurrent_tools,
            loop_detection_threshold_changed: old.loop_detection_threshold
                != new.loop_detection_threshold,
            bm25_threshold_changed: (old.bm25_threshold - new.bm25_threshold)
                .abs()
                > f64::EPSILON,
        }
    }

    pub fn changed_field_names(&self) -> Vec<&str> {
        let mut names = Vec::new();
        if self.default_provider_changed {
            names.push("provider.default_provider");
        }
        if self.default_model_changed {
            names.push("provider.default_model");
        }
        if self.token_budget_changed {
            names.push("context.token_budget");
        }
        if self.compression_threshold_changed {
            names.push("context.compression_threshold");
        }
        if self.max_iterations_changed {
            names.push("agent.max_iterations");
        }
        if self.max_concurrent_tools_changed {
            names.push("agent.max_concurrent_tools");
        }
        if self.loop_detection_threshold_changed {
            names.push("guardian.loop_detection_threshold");
        }
        if self.bm25_threshold_changed {
            names.push("skill.bm25_threshold");
        }
        names
    }

    pub fn is_empty(&self) -> bool {
        !self.default_provider_changed
            && !self.default_model_changed
            && !self.token_budget_changed
            && !self.compression_threshold_changed
            && !self.max_iterations_changed
            && !self.max_concurrent_tools_changed
            && !self.loop_detection_threshold_changed
            && !self.bm25_threshold_changed
    }
}

/// Shared, atomically-swappable reference to the active
/// [`SynthiaConfig`].
pub type SharedConfig = Arc<RwLock<SynthiaConfig>>;
