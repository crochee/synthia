//! The [`HookRunner`] struct itself, plus its construction,
//! setters, accessors, `Default` impl, and the
//! [`SharedHookRunner`] thread-safe wrapper type.
//!
//! Hook loading lives in [`super::load`]; the public event
//! dispatch lives in [`super::fire`]; the actual hook execution
//! lives in [`super::execute`].

use std::{path::PathBuf, sync::Arc};

use regex::Regex;
use tokio::sync::Mutex;

use super::types::HookRunnerConfig;
use crate::types::{FailMode, HookSpec};

/// HookRunner manages hook loading, matching, and execution.
///
/// # Example
/// ```ignore
/// let mut runner = HookRunner::new();
/// runner.load_from_path(Path::new("/path/to/plugin/"))?;
///
/// // Fire an event
/// let metadata = HookMetadata::new("read_file");
/// let results = runner.fire(HookEvent::PreToolUse, metadata).await?;
/// ```
pub struct HookRunner {
    /// Loaded hook specifications, sorted by priority
    pub(crate) configs: Vec<HookSpec>,
    /// Compiled regex matchers (index matches configs)
    pub(crate) matchers: Vec<Option<Regex>>,
    /// Base directory for resolving relative paths
    pub(crate) base_dir: PathBuf,
    /// Default timeout for command hooks (seconds)
    pub(crate) default_timeout: u64,
    /// Configuration for hook execution behavior
    pub(crate) config: HookRunnerConfig,
}

impl HookRunner {
    /// Create a new empty hook runner
    pub fn new() -> Self {
        Self {
            configs: Vec::new(),
            matchers: Vec::new(),
            base_dir: PathBuf::new(),
            default_timeout: 30,
            config: HookRunnerConfig::default(),
        }
    }

    /// Create a hook runner with a base directory
    pub fn with_base_dir(base_dir: impl Into<PathBuf>) -> Self {
        Self {
            configs: Vec::new(),
            matchers: Vec::new(),
            base_dir: base_dir.into(),
            default_timeout: 30,
            config: HookRunnerConfig::default(),
        }
    }

    /// Set the fail mode for hook execution
    pub fn with_fail_mode(mut self, fail_mode: FailMode) -> Self {
        self.config.fail_mode = fail_mode;
        self
    }

    /// Set the default timeout for command hooks (in seconds)
    pub fn with_default_timeout(mut self, timeout_secs: u64) -> Self {
        self.default_timeout = timeout_secs;
        self
    }

    /// Get the number of loaded hooks
    pub fn len(&self) -> usize {
        self.configs.len()
    }

    /// Check if no hooks are loaded
    pub fn is_empty(&self) -> bool {
        self.configs.is_empty()
    }

    /// Get all hook specs (sorted by priority)
    pub fn configs(&self) -> &[HookSpec] {
        &self.configs
    }
}

impl Default for HookRunner {
    fn default() -> Self {
        Self::new()
    }
}

/// Thread-safe wrapper for HookRunner (blocking `std::sync::Mutex`).
pub type SharedHookRunner = Arc<Mutex<HookRunner>>;
