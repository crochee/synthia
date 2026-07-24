//! Error recovery system implementing five-layer recovery:
//! Truncate -> Retry -> Fallback -> Auto-compact -> Reset -> Fail-fast

pub mod compact;
pub mod fallback;
pub mod recovery_cascade;
pub mod reset;
pub mod retry;

use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

pub use recovery_cascade::ConsecutiveFailureTracker;
pub use reset::ResetCoordinator;
use retry::RetryStrategy;

/// Error recovery level
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryLevel {
    /// L1: Truncate output to reduce context size
    L1Truncate,
    /// L2: Retry the failed operation
    L2Retry,
    /// L3: Fallback to an alternative approach
    L3Fallback,
    /// L4: Auto-compact the conversation
    L4Compact,
    /// L5: Reset the agent state
    L5Reset,
}

impl RecoveryLevel {
    /// Returns the next higher recovery level, or None if already at the highest
    pub fn escalate(&self) -> Option<Self> {
        match self {
            Self::L1Truncate => Some(Self::L2Retry),
            Self::L2Retry => Some(Self::L3Fallback),
            Self::L3Fallback => Some(Self::L4Compact),
            Self::L4Compact => Some(Self::L5Reset),
            Self::L5Reset => None,
        }
    }

    /// Returns the numeric level (1-5)
    pub fn level_number(&self) -> u32 {
        match self {
            Self::L1Truncate => 1,
            Self::L2Retry => 2,
            Self::L3Fallback => 3,
            Self::L4Compact => 4,
            Self::L5Reset => 5,
        }
    }
}

/// Result of a recovery attempt
#[derive(Debug, Clone)]
pub enum RecoveryResult {
    /// Recovery succeeded, operation can continue
    Recovered,
    /// Escalated to a higher recovery level
    Escalated(RecoveryLevel),
    /// Cannot recover, entering fail-fast mode
    FailFast(String),
}

/// Error recovery coordinator that tracks error state and determines
/// the appropriate recovery action.
pub struct ErrorRecoveryCoordinator {
    retry_strategy: RetryStrategy,
    consecutive_errors: AtomicU64,
    last_recovery_time: AtomicU64,
    cooldown_secs: u64,
}

impl ErrorRecoveryCoordinator {
    /// Creates a new coordinator with the specified cooldown period.
    ///
    /// # Arguments
    /// * `cooldown_secs` - Minimum seconds between recovery attempts
    pub fn new(cooldown_secs: u64) -> Self {
        Self {
            retry_strategy: RetryStrategy::new(2),
            consecutive_errors: AtomicU64::new(0),
            last_recovery_time: AtomicU64::new(0),
            cooldown_secs,
        }
    }

    /// Handles an error and returns the appropriate recovery action.
    ///
    /// # Arguments
    /// * `error` - Description of the error that occurred
    /// * `current_level` - The current recovery level being attempted
    pub fn handle_error(
        &self,
        error: &str,
        current_level: RecoveryLevel,
    ) -> RecoveryResult {
        // Check cooldown period
        let now = current_timestamp_secs();
        let last = self.last_recovery_time.load(Ordering::Relaxed);

        if last > 0 && now - last < self.cooldown_secs {
            tracing::warn!(
                error,
                cooldown_remaining = self.cooldown_secs - (now - last),
                "Error recovery in cooldown period"
            );
            return RecoveryResult::FailFast("In cooldown period".to_string());
        }

        let errors =
            self.consecutive_errors.fetch_add(1, Ordering::Relaxed) + 1;

        tracing::warn!(
            error,
            errors,
            level = ?current_level,
            "Handling error at recovery level"
        );

        match current_level {
            RecoveryLevel::L1Truncate => {
                // L1 is a preparatory step, escalate to retry
                RecoveryResult::Escalated(RecoveryLevel::L2Retry)
            }
            RecoveryLevel::L2Retry => {
                // Check if we should retry or escalate to fallback
                if self.retry_strategy.should_retry(errors as u32) {
                    RecoveryResult::Escalated(RecoveryLevel::L2Retry)
                } else {
                    RecoveryResult::Escalated(RecoveryLevel::L3Fallback)
                }
            }
            RecoveryLevel::L3Fallback => {
                // Fallback attempted, escalate to compact
                RecoveryResult::Escalated(RecoveryLevel::L4Compact)
            }
            RecoveryLevel::L4Compact => {
                // Compaction attempted, escalate to reset
                RecoveryResult::Escalated(RecoveryLevel::L5Reset)
            }
            RecoveryLevel::L5Reset => {
                // Reset failed, cannot recover further — enter cooldown
                let now = current_timestamp_secs();
                self.last_recovery_time.store(now, Ordering::Relaxed);
                RecoveryResult::FailFast(
                    "Reset failed, entering fail-fast".to_string(),
                )
            }
        }
    }

    /// Records a successful operation, resetting the error counter.
    /// Note: cooldown timestamp is cleared so subsequent failures are not blocked.
    pub fn record_success(&self) {
        self.consecutive_errors.store(0, Ordering::Relaxed);
        self.last_recovery_time.store(0, Ordering::Relaxed);
    }

    /// Returns the current consecutive error count.
    pub fn consecutive_error_count(&self) -> u64 {
        self.consecutive_errors.load(Ordering::Relaxed)
    }

    /// Returns the retry strategy for this coordinator.
    pub fn retry_strategy(&self) -> &RetryStrategy {
        &self.retry_strategy
    }

    /// Calculates the backoff delay for a retry attempt.
    pub fn calculate_backoff(&self, attempt: u32) -> Duration {
        self.retry_strategy.calculate_delay(attempt)
    }
}

fn current_timestamp_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_recovery_level_escalation() {
        assert_eq!(
            RecoveryLevel::L1Truncate.escalate(),
            Some(RecoveryLevel::L2Retry)
        );
        assert_eq!(
            RecoveryLevel::L2Retry.escalate(),
            Some(RecoveryLevel::L3Fallback)
        );
        assert_eq!(
            RecoveryLevel::L3Fallback.escalate(),
            Some(RecoveryLevel::L4Compact)
        );
        assert_eq!(
            RecoveryLevel::L4Compact.escalate(),
            Some(RecoveryLevel::L5Reset)
        );
        assert_eq!(RecoveryLevel::L5Reset.escalate(), None);
    }

    #[test]
    fn test_recovery_level_numbers() {
        assert_eq!(RecoveryLevel::L1Truncate.level_number(), 1);
        assert_eq!(RecoveryLevel::L2Retry.level_number(), 2);
        assert_eq!(RecoveryLevel::L3Fallback.level_number(), 3);
        assert_eq!(RecoveryLevel::L4Compact.level_number(), 4);
        assert_eq!(RecoveryLevel::L5Reset.level_number(), 5);
    }

    #[test]
    fn test_coordinator_initial_state() {
        let coordinator = ErrorRecoveryCoordinator::new(30);
        assert_eq!(coordinator.consecutive_error_count(), 0);
    }

    #[test]
    fn test_coordinator_handle_error_l1() {
        let coordinator = ErrorRecoveryCoordinator::new(0); // No cooldown
        let result =
            coordinator.handle_error("test", RecoveryLevel::L1Truncate);
        assert!(matches!(
            result,
            RecoveryResult::Escalated(RecoveryLevel::L2Retry)
        ));
    }

    #[test]
    fn test_coordinator_handle_error_l5() {
        let coordinator = ErrorRecoveryCoordinator::new(0);
        let result = coordinator.handle_error("test", RecoveryLevel::L5Reset);
        assert!(matches!(result, RecoveryResult::FailFast(_)));
    }

    #[test]
    fn test_coordinator_record_success_resets_counter() {
        let coordinator = ErrorRecoveryCoordinator::new(0);
        coordinator.handle_error("err1", RecoveryLevel::L1Truncate);
        assert_eq!(coordinator.consecutive_error_count(), 1);
        coordinator.record_success();
        assert_eq!(coordinator.consecutive_error_count(), 0);
    }

    #[test]
    fn test_coordinator_cooldown() {
        let coordinator = ErrorRecoveryCoordinator::new(60);

        // First L1 error — Escalated, NO cooldown entered
        let result1 =
            coordinator.handle_error("test1", RecoveryLevel::L1Truncate);
        assert!(matches!(result1, RecoveryResult::Escalated(_)));

        // Immediate second L1 error — still Escalated (cooldown was never entered on first call)
        let result2 =
            coordinator.handle_error("test2", RecoveryLevel::L1Truncate);
        assert!(matches!(result2, RecoveryResult::Escalated(_)));

        // Now trigger FailFast (L5) — this enters cooldown
        let result3 = coordinator.handle_error("test3", RecoveryLevel::L5Reset);
        assert!(matches!(result3, RecoveryResult::FailFast(_)));

        // Immediate next call — NOW in cooldown, should fail-fast
        let result4 =
            coordinator.handle_error("test4", RecoveryLevel::L1Truncate);
        assert!(matches!(result4, RecoveryResult::FailFast(_)));
    }

    #[test]
    fn test_coordinator_calculates_backoff() {
        let coordinator = ErrorRecoveryCoordinator::new(30);
        assert_eq!(coordinator.calculate_backoff(0), Duration::from_secs(2));
        assert_eq!(coordinator.calculate_backoff(1), Duration::from_secs(4));
        assert_eq!(coordinator.calculate_backoff(2), Duration::from_secs(8));
    }
}
