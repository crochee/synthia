//! All simple data types used by the hook runner: the
//! deserialization helpers ([`RawHook`], [`RawHooks`]), the error
//! enum ([`HookRunnerError`]), the per-call result type
//! ([`SingleHookResult`]), the event-metadata ([`HookMetadata`]),
//! and the runner-wide config ([`HookRunnerConfig`]).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::types::{FailMode, HookEvent, HookHandler, HookResult, HookSpec};

/// Raw hook deserialization structure (used internally)
#[derive(serde::Deserialize)]
pub(super) struct RawHook {
    pub(super) event: HookEvent,
    #[serde(default)]
    pub(super) matcher: Option<String>,
    pub(super) handler: HookHandler,
    #[serde(default)]
    pub(super) priority: Option<i32>,
}

/// Raw hooks wrapper structure
#[derive(serde::Deserialize)]
pub(super) struct RawHooks {
    pub(super) hooks: Vec<RawHook>,
}

/// Errors that can occur during hook execution
#[derive(Debug, thiserror::Error)]
pub enum HookRunnerError {
    #[error("Failed to read hooks.json: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Failed to parse hooks.json: {0}")]
    ParseError(#[from] serde_json::Error),

    #[error("Invalid regex pattern '{0}': {1}")]
    InvalidRegex(String, #[source] regex::Error),

    #[error("Hook execution failed: {0}")]
    ExecutionFailed(String),

    #[error("Hook execution timed out after {0}s")]
    Timeout(u64),
}

/// Metadata passed with each hook event for matcher evaluation
#[derive(Debug, Clone, Default)]
pub struct HookMetadata {
    /// Target name (e.g., tool name, agent id)
    pub target: Option<String>,
    /// Additional key-value metadata
    pub extras: HashMap<String, String>,
}

impl HookMetadata {
    /// Create new metadata with a target
    pub fn new(target: impl Into<String>) -> Self {
        Self {
            target: Some(target.into()),
            extras: HashMap::new(),
        }
    }

    /// Add a key-value extra
    pub fn with_extra(
        mut self,
        key: impl Into<String>,
        value: impl Into<String>,
    ) -> Self {
        self.extras.insert(key.into(), value.into());
        self
    }

    /// Get the target string for regex matching
    pub fn target_str(&self) -> String {
        self.target.clone().unwrap_or_default()
    }
}

/// Configuration for hook runner behavior.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HookRunnerConfig {
    /// Fail mode: controls whether hook failures block execution
    #[serde(default)]
    pub fail_mode: FailMode,
}

impl HookRunnerConfig {
    /// Create a new config with default settings (fail-open)
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the fail mode
    pub fn with_fail_mode(mut self, fail_mode: FailMode) -> Self {
        self.fail_mode = fail_mode;
        self
    }

    /// Use fail-closed mode (hook failure blocks execution)
    pub fn fail_closed(self) -> Self {
        self.with_fail_mode(FailMode::Closed)
    }

    /// Use fail-open mode (hook failure allows execution)
    pub fn fail_open(self) -> Self {
        self.with_fail_mode(FailMode::Open)
    }
}

/// Individual hook execution result
#[derive(Debug)]
pub struct SingleHookResult {
    /// The hook spec that was executed
    pub config: HookSpec,
    /// The result of execution
    pub result: Result<HookResult, HookRunnerError>,
    /// Execution duration in milliseconds
    pub duration_ms: u64,
}
