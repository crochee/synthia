//! The [`TokenBudget`] + [`TokenBudgetStatus`] pair + the 2
//! package-level `CONTEXT_*` constants.
//!
//! The budget is computed at construction time from
//! `hard_limit`: `soft_limit = 0.7 * hard_limit`,
//! `compaction_at = 0.85 * hard_limit`,
//! `must_compact_at = 0.9 * hard_limit`. Use
//! [`TokenBudget::with_thresholds`] when the ratio defaults
//! need to be overridden (e.g. for gpt-4o's 8K context where
//! the 90% `must_compact_at` would leave too little room for a
//! pre-sampling compaction).

/// Below this many tokens of free space, every context write
/// is rejected with "Context window below hard minimum (16K
/// tokens)".
pub const CONTEXT_HARD_MIN: usize = 16_384;

/// Below this many tokens of free space, the agent logs a
/// warning but still permits the write.
pub const CONTEXT_WARN_BELOW: usize = 32_768;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TokenBudgetStatus {
    Ok,
    Notice,
    Warning,
    MustCompact,
}

#[derive(Debug, Clone)]
pub struct TokenBudget {
    pub soft_limit: usize,
    pub hard_limit: usize,
    pub compaction_at: usize,
    pub must_compact_at: usize,
}

impl Default for TokenBudget {
    fn default() -> Self {
        Self::new(128_000)
    }
}

impl TokenBudget {
    pub fn new(hard_limit: usize) -> Self {
        Self {
            soft_limit: (hard_limit as f64 * 0.7) as usize,
            hard_limit,
            compaction_at: (hard_limit as f64 * 0.85) as usize,
            must_compact_at: (hard_limit as f64 * 0.9) as usize,
        }
    }

    pub fn with_thresholds(
        hard_limit: usize,
        pre_sampling: f64,
        mid_turn: f64,
    ) -> Self {
        Self {
            hard_limit,
            soft_limit: (hard_limit as f64 * pre_sampling) as usize,
            compaction_at: (hard_limit as f64 * mid_turn) as usize,
            must_compact_at: (hard_limit as f64 * 0.9) as usize,
        }
    }

    pub fn check(&self, current: usize) -> TokenBudgetStatus {
        if current >= self.must_compact_at {
            TokenBudgetStatus::MustCompact
        } else if current >= self.compaction_at {
            TokenBudgetStatus::Warning
        } else if current >= self.soft_limit {
            TokenBudgetStatus::Notice
        } else {
            TokenBudgetStatus::Ok
        }
    }

    pub fn threshold_name(status: &TokenBudgetStatus) -> &'static str {
        match status {
            TokenBudgetStatus::Ok => "ok",
            TokenBudgetStatus::Notice => "70%",
            TokenBudgetStatus::Warning => "85%",
            TokenBudgetStatus::MustCompact => "90%",
        }
    }

    pub fn threshold_value(&self, status: &TokenBudgetStatus) -> usize {
        match status {
            TokenBudgetStatus::Ok => 0,
            TokenBudgetStatus::Notice => self.soft_limit,
            TokenBudgetStatus::Warning => self.compaction_at,
            TokenBudgetStatus::MustCompact => self.must_compact_at,
        }
    }
}
