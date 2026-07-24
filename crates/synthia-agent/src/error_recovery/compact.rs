//! Auto-compact strategy for L4 recovery
//!
//! Handles conversation compaction when error recovery reaches L4.

/// Result of a compaction attempt
#[derive(Debug, Clone)]
pub struct CompactResult {
    /// Whether compaction was actually performed
    pub compacted: bool,
    /// Token count before compaction
    pub tokens_before: usize,
    /// Token count after compaction (same as before if not compacted)
    pub tokens_after: usize,
}

impl CompactResult {
    /// Creates a result indicating no compaction was needed
    pub fn no_compaction_needed(token_count: usize) -> Self {
        Self {
            compacted: false,
            tokens_before: token_count,
            tokens_after: token_count,
        }
    }

    /// Creates a result indicating compaction was performed
    pub fn compacted(before: usize, after: usize) -> Self {
        Self {
            compacted: true,
            tokens_before: before,
            tokens_after: after,
        }
    }

    /// Returns the reduction ratio (0.0-1.0)
    pub fn reduction_ratio(&self) -> f64 {
        if self.tokens_before == 0 {
            return 0.0;
        }
        (self.tokens_before - self.tokens_after) as f64
            / self.tokens_before as f64
    }
}

/// Auto-compact coordinator for L4 recovery
pub struct CompactCoordinator;

impl CompactCoordinator {
    /// Determines if compaction should be triggered based on token usage
    ///
    /// # Arguments
    /// * `current_tokens` - Current token count
    /// * `hard_limit` - Maximum token limit
    /// * `compaction_threshold` - Ratio at which to trigger compaction (0.0-1.0)
    pub fn should_compact(
        current_tokens: usize,
        hard_limit: usize,
        compaction_threshold: f64,
    ) -> bool {
        if hard_limit == 0 {
            return false;
        }
        let ratio = current_tokens as f64 / hard_limit as f64;
        ratio >= compaction_threshold
    }

    /// Creates a no-op compaction result
    pub fn no_op() -> CompactResult {
        CompactResult::no_compaction_needed(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_compact_above_threshold() {
        assert!(CompactCoordinator::should_compact(80_000, 100_000, 0.8));
        assert!(CompactCoordinator::should_compact(90_000, 100_000, 0.8));
    }

    #[test]
    fn test_should_compact_below_threshold() {
        assert!(!CompactCoordinator::should_compact(70_000, 100_000, 0.8));
        assert!(!CompactCoordinator::should_compact(50_000, 100_000, 0.8));
    }

    #[test]
    fn test_should_compact_zero_limit() {
        assert!(!CompactCoordinator::should_compact(1000, 0, 0.8));
    }

    #[test]
    fn test_compact_result_reduction_ratio() {
        let result = CompactResult::compacted(1000, 700);
        assert!((result.reduction_ratio() - 0.3).abs() < 0.001);
    }

    #[test]
    fn test_compact_result_no_reduction() {
        let result = CompactResult::no_compaction_needed(1000);
        assert_eq!(result.reduction_ratio(), 0.0);
    }

    #[test]
    fn test_compact_result_zero_tokens() {
        let result = CompactResult::no_compaction_needed(0);
        assert_eq!(result.reduction_ratio(), 0.0);
    }
}
