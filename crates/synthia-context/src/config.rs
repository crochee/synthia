use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextConfig {
    pub max_context_tokens: usize,
    pub pre_sampling_threshold: f64,
    pub mid_turn_threshold: f64,
    pub max_iterations: usize,
    pub hot_memory_path: Option<PathBuf>,
    pub protection_zone: ProtectionZoneConfig,
    /// Token budget that triggers compaction. Default: 20,000.
    #[serde(default = "default_compaction_buffer")]
    pub compaction_buffer: usize,
    /// Tokens to keep after compaction. Default: 8,000.
    #[serde(default = "default_keep_tokens")]
    pub keep_tokens: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtectionZoneConfig {
    pub min_rounds: usize,
    pub token_ratio: f64,
}

fn default_compaction_buffer() -> usize {
    20_000
}

fn default_keep_tokens() -> usize {
    8_000
}

impl Default for ContextConfig {
    fn default() -> Self {
        Self {
            max_context_tokens: 128_000,
            pre_sampling_threshold: 0.7,
            mid_turn_threshold: 0.85,
            max_iterations: 50,
            hot_memory_path: None,
            protection_zone: ProtectionZoneConfig::default(),
            compaction_buffer: default_compaction_buffer(),
            keep_tokens: default_keep_tokens(),
        }
    }
}

impl Default for ProtectionZoneConfig {
    fn default() -> Self {
        Self {
            min_rounds: 3,
            token_ratio: 0.35,
        }
    }
}
