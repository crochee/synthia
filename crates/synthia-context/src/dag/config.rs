//! Configuration knobs. Defaults reproduce `pi-lcm`'s documented
//! `LcmConfig`; `validated` clamps absurd values back into range.

/// Configuration knobs. Defaults reproduce `pi-lcm`'s documented
/// `LcmConfig`.
#[derive(Clone, Debug)]
pub struct DagConfig {
    /// Token budget per leaf chunk (pi-lcm default: 4000).
    pub leaf_chunk_tokens: usize,
    /// Unconsumed summaries at depth N that trigger condensation
    /// into one D-(N+1) node (pi-lcm default: 6).
    pub condensation_threshold: usize,
    /// Maximum DAG depth (pi-lcm default: 5).
    pub max_depth: u32,
    /// Maximum condensation cascade passes per `prepare`
    /// (pi-lcm `MAX_CONDENSE_PASSES`: 10).
    pub max_passes: u32,
    /// Parallel summariser waves in the leaf pass (pi-lcm: 4).
    pub leaf_pass_concurrency: usize,
    /// Skip compaction below this many messages (pi-lcm: 10).
    pub min_messages_for_compaction: usize,
    /// Token cap for the assembled summary text (pi-lcm: 8000).
    pub max_summary_tokens: usize,
}

impl Default for DagConfig {
    fn default() -> Self {
        Self {
            leaf_chunk_tokens: 4000,
            condensation_threshold: 6,
            max_depth: 5,
            max_passes: 10,
            leaf_pass_concurrency: 4,
            min_messages_for_compaction: 10,
            max_summary_tokens: 8000,
        }
    }
}

impl DagConfig {
    /// Clamp absurd values back into the safe range (mirrors
    /// `pi-lcm`'s `Math.max` floors).
    pub fn validated(mut self) -> Self {
        self.leaf_chunk_tokens = self.leaf_chunk_tokens.max(500);
        self.condensation_threshold = self.condensation_threshold.max(2);
        self.max_depth = self.max_depth.max(1);
        self.max_passes = self.max_passes.max(1);
        self.leaf_pass_concurrency = self.leaf_pass_concurrency.max(1);
        self.min_messages_for_compaction =
            self.min_messages_for_compaction.max(2);
        self.max_summary_tokens = self.max_summary_tokens.max(500);
        self
    }
}
