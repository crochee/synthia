use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub const CHECKPOINT_MAX_COUNT: usize = 5;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointData {
    pub session_id: String,
    pub step: usize,
    pub timestamp: String,
    pub messages: Vec<synthia_provider::types::Message>,
    pub pending_tool_calls: Vec<PendingToolCall>,
    pub config_snapshot: AgentConfigSnapshot,
    pub guardian_state: GuardianState,
    pub token_usage: synthia_provider::types::TokenUsage,
    pub iteration_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PendingToolCall {
    pub id: String,
    pub name: String,
    pub input: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentConfigSnapshot {
    pub model: String,
    pub max_iterations: usize,
    pub max_tokens: usize,
    pub temperature: f64,
}

pub struct Checkpoint {
    pub session_id: String,
    pub checkpoint_dir: PathBuf,
    pub step: usize,
    pub guardian_state: GuardianState,
    pub messages: Vec<synthia_provider::types::Message>,
    pub pending_tool_calls: Vec<PendingToolCall>,
    pub config_snapshot: AgentConfigSnapshot,
    pub token_usage: synthia_provider::types::TokenUsage,
    pub iteration_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GuardianState {
    pub loop_detection_counts: std::collections::HashMap<String, usize>,
    pub no_progress_count: usize,
    pub consecutive_errors: usize,
    pub circuit_breaker_open: bool,
}
