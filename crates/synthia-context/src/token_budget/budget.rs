/// Status returned by `TokenBudget::check`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BudgetStatus {
    /// Token usage is within safe limits.
    Ok,
    /// Token usage has reached the notice threshold (default 70%).
    Notice,
    /// Token usage has reached the warning threshold (default 85%).
    Warning,
    /// Token usage has reached the compaction threshold (default 90%).
    MustCompact,
}

/// Configurable percentage thresholds for token budget alerts.
///
/// All values are ratios (0.0-1.0) representing the fraction of the hard limit.
#[derive(Debug, Clone)]
pub struct TokenBudgetConfig {
    /// Ratio at which to emit a notice (default 0.70).
    pub notice_ratio: f64,
    /// Ratio at which to emit a warning (default 0.85).
    pub warning_ratio: f64,
    /// Ratio at which compaction is required (default 0.90).
    pub compaction_ratio: f64,
}

impl Default for TokenBudgetConfig {
    fn default() -> Self {
        Self {
            notice_ratio: 0.70,
            warning_ratio: 0.85,
            compaction_ratio: 0.90,
        }
    }
}

impl TokenBudgetConfig {
    /// Create a custom configuration.
    ///
    /// Ratios should be in the range 0.0-1.0. Values outside this range
    /// will be clamped.
    pub fn new(
        notice_ratio: f64,
        warning_ratio: f64,
        compaction_ratio: f64,
    ) -> Self {
        Self {
            notice_ratio: notice_ratio.clamp(0.0, 1.0),
            warning_ratio: warning_ratio.clamp(0.0, 1.0),
            compaction_ratio: compaction_ratio.clamp(0.0, 1.0),
        }
    }
}

/// Token budget tracker with soft and hard limits.
///
/// Tracks the total context window size (hard_limit), a soft limit for
/// proactive alerts, and a compaction threshold that triggers when context
/// must be reduced.
#[derive(Debug, Clone)]
pub struct TokenBudget {
    /// Absolute maximum tokens the context window can hold.
    pub hard_limit: usize,
    /// Soft limit for proactive alerting (typically lower than hard_limit).
    pub soft_limit: usize,
    /// Token count at which compaction should be triggered.
    pub compaction_at: usize,
    /// The percentage-based configuration used to compute thresholds.
    config: TokenBudgetConfig,
}

impl TokenBudget {
    /// Create a new TokenBudget with the given hard limit and default config.
    ///
    /// The soft_limit is set to the hard_limit by default. Thresholds
    /// (compaction_at, etc.) are computed from the default config ratios.
    pub fn new(hard_limit: usize) -> Self {
        Self::with_config(hard_limit, TokenBudgetConfig::default())
    }

    /// Create a TokenBudget with a custom configuration.
    pub fn with_config(hard_limit: usize, config: TokenBudgetConfig) -> Self {
        let soft_limit = hard_limit;
        let compaction_at =
            ((hard_limit as f64) * config.compaction_ratio) as usize;
        Self {
            hard_limit,
            soft_limit,
            compaction_at,
            config,
        }
    }

    /// Create a TokenBudget with a custom soft limit.
    ///
    /// The soft_limit determines when compaction should be triggered
    /// (when token_count exceeds soft_limit). The compaction_at threshold
    /// is computed from the default config ratio (90%).
    pub fn with_soft_limit(hard_limit: usize, soft_limit: usize) -> Self {
        let compaction_at = ((hard_limit as f64)
            * TokenBudgetConfig::default().compaction_ratio)
            as usize;
        Self {
            hard_limit,
            soft_limit,
            compaction_at,
            config: TokenBudgetConfig::default(),
        }
    }

    /// Check the current token usage against budget thresholds.
    ///
    /// Returns a `BudgetStatus` indicating how close the current usage is
    /// to the limits.
    pub fn check(&self, current_tokens: usize) -> BudgetStatus {
        if current_tokens >= self.compaction_at {
            return BudgetStatus::MustCompact;
        }

        let warning_at =
            ((self.hard_limit as f64) * self.config.warning_ratio) as usize;
        if current_tokens >= warning_at {
            return BudgetStatus::Warning;
        }

        let notice_at =
            ((self.hard_limit as f64) * self.config.notice_ratio) as usize;
        if current_tokens >= notice_at {
            return BudgetStatus::Notice;
        }

        BudgetStatus::Ok
    }

    /// Returns the number of tokens remaining before the hard limit is reached.
    pub fn remaining(&self, current_tokens: usize) -> usize {
        self.hard_limit.saturating_sub(current_tokens)
    }

    /// Returns the number of tokens remaining before compaction is triggered.
    pub fn remaining_until_compaction(&self, current_tokens: usize) -> usize {
        self.compaction_at.saturating_sub(current_tokens)
    }

    /// Returns true if current token usage has reached the compaction threshold.
    pub fn needs_compaction(&self, current_tokens: usize) -> bool {
        matches!(self.check(current_tokens), BudgetStatus::MustCompact)
    }

    /// Returns the current usage as a ratio of the hard limit (0.0-1.0+).
    pub fn usage_ratio(&self, current_tokens: usize) -> f64 {
        if self.hard_limit == 0 {
            return 0.0;
        }
        current_tokens as f64 / self.hard_limit as f64
    }
}
