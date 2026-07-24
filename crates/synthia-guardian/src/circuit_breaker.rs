//! Circuit breaker for compaction failures.
//!
//! Implements the fail-fast principle: after consecutive_compact_failures
//! reaches the threshold, the circuit breaker opens and prevents further
//! attempts until explicitly reset.

use tracing::warn;

/// Default maximum number of consecutive compaction failures before opening the circuit.
const DEFAULT_MAX_COMPACT_FAILURES: usize = 3;

/// Circuit breaker for tracking consecutive compaction failures.
///
/// Follows the standard circuit breaker pattern:
/// - Closed: normal operation, failures are tracked
/// - Open: after max failures, further attempts are blocked
/// - Reset: manually return to closed state
pub struct CircuitBreaker {
    /// Consecutive compaction failure count
    consecutive_compact_failures: usize,
    /// Maximum allowed consecutive failures before opening
    max_compact_failures: usize,
    /// Whether the circuit breaker is currently open
    pub open: bool,
}

impl CircuitBreaker {
    /// Creates a new circuit breaker with the specified failure threshold.
    #[must_use]
    pub fn new(max_compact_failures: usize) -> Self {
        Self {
            consecutive_compact_failures: 0,
            max_compact_failures,
            open: false,
        }
    }

    /// Records a successful compaction attempt.
    /// Resets the failure counter and closes the circuit.
    pub fn record_success(&mut self) {
        self.consecutive_compact_failures = 0;
        self.open = false;
    }

    /// Records a failed compaction attempt.
    ///
    /// Returns `true` if the circuit breaker has opened due to reaching
    /// the failure threshold.
    pub fn record_failure(&mut self) -> bool {
        self.consecutive_compact_failures += 1;

        if self.consecutive_compact_failures >= self.max_compact_failures {
            warn!(
                failures = self.consecutive_compact_failures,
                "Circuit breaker opened after consecutive compaction failures"
            );
            self.open = true;
            return true;
        }

        false
    }

    /// Manually resets the circuit breaker to closed state.
    pub fn reset(&mut self) {
        self.consecutive_compact_failures = 0;
        self.open = false;
    }

    /// Returns the current count of consecutive failures.
    #[must_use]
    pub fn failure_count(&self) -> usize {
        self.consecutive_compact_failures
    }

    /// Returns the maximum allowed consecutive failures.
    #[must_use]
    pub fn max_failures(&self) -> usize {
        self.max_compact_failures
    }
}

impl Default for CircuitBreaker {
    fn default() -> Self {
        Self::new(DEFAULT_MAX_COMPACT_FAILURES)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_circuit_breaker() {
        let cb = CircuitBreaker::default();
        assert!(!cb.open);
        assert_eq!(cb.failure_count(), 0);
        assert_eq!(cb.max_failures(), DEFAULT_MAX_COMPACT_FAILURES);
    }

    #[test]
    fn test_opens_after_max_failures() {
        let mut cb = CircuitBreaker::new(3);

        assert!(!cb.record_failure());
        assert_eq!(cb.failure_count(), 1);
        assert!(!cb.open);

        assert!(!cb.record_failure());
        assert_eq!(cb.failure_count(), 2);
        assert!(!cb.open);

        assert!(cb.record_failure());
        assert_eq!(cb.failure_count(), 3);
        assert!(cb.open);
    }

    #[test]
    fn test_reset_clears_state() {
        let mut cb = CircuitBreaker::new(2);
        cb.record_failure();
        cb.record_failure();
        assert!(cb.open);

        cb.reset();
        assert!(!cb.open);
        assert_eq!(cb.failure_count(), 0);
    }

    #[test]
    fn test_success_resets_failures() {
        let mut cb = CircuitBreaker::new(3);
        cb.record_failure();
        cb.record_failure();
        assert_eq!(cb.failure_count(), 2);

        cb.record_success();
        assert_eq!(cb.failure_count(), 0);
        assert!(!cb.open);
    }
}
