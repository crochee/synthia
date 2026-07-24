//! Agent-level checkpoint persistence.
//!
//! This module provides serialization-friendly checkpoint types for the
//! agent crate. It builds on `synthia_context::checkpoint` for core
//! save/load/rotate mechanics while defining agent-specific snapshot types
//! (`AgentConfigSnapshot`, `GuardianState`) that mirror runtime structs.
//!
//! # Module layout
//!
//! - [`checkpoint`]: `Checkpoint` struct with builder pattern, save/load/rotate
//!   mechanics for `.agents/checkpoints/{session_id}/step_{N}.json`.
//! - [`recovery`]: Recovery helpers that mark unfinished tool calls as
//!   `"executing"` after a resume.
//! - [`tests`]: Unit tests covering persistence, rotation, and recovery.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use synthia_provider::types::Message;

use crate::types::{AgentConfig, TokenUsage};

mod core;
mod recovery;
#[cfg(test)]
mod tests;

pub use core::Checkpoint;

pub use recovery::patch_tool_calls_recovery;
// Re-export the patch function for recovery workflows.
pub use synthia_context::checkpoint::patch_tool_calls;

/// Maximum number of checkpoint files retained per session.
const MAX_CHECKPOINTS: usize = 5;

/// Serializable snapshot of the agent configuration at checkpoint time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfigSnapshot {
    pub model: String,
    pub max_tokens: usize,
    pub max_iterations: usize,
    pub temperature: Option<f64>,
    pub token_budget: Option<usize>,
}

impl AgentConfigSnapshot {
    pub fn from_config(config: &AgentConfig) -> Self {
        Self {
            model: config.model.clone(),
            max_tokens: config.max_tokens,
            max_iterations: config.max_iterations,
            temperature: config.temperature,
            token_budget: config.token_budget,
        }
    }
}

impl Default for AgentConfigSnapshot {
    fn default() -> Self {
        let cfg = AgentConfig::default();
        Self::from_config(&cfg)
    }
}

/// Serializable guardian state for checkpoint persistence.
///
/// Mirrors the in-memory `synthia_guardian::GuardianState` but uses
/// concrete serializable types instead of detector structs.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GuardianState {
    pub loop_counts: HashMap<String, usize>,
    pub no_progress: bool,
    pub consecutive_errors: usize,
    pub circuit_breaker_open: bool,
}

/// Serializable pending tool call for checkpoint storage.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PendingToolCall {
    pub id: String,
    pub name: String,
    pub input: serde_json::Value,
}

/// Full checkpoint data written to disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointData {
    pub session_id: String,
    pub step: usize,
    pub timestamp: String,
    pub messages: Vec<Message>,
    pub pending_tool_calls: Vec<PendingToolCall>,
    pub agent_config: AgentConfigSnapshot,
    pub guardian_state: GuardianState,
    pub token_usage: TokenUsage,
    pub iteration: usize,
}
