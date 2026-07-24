//! Retry strategy with exponential backoff

use std::time::Duration;

/// Retry strategy configuration for error recovery
pub struct RetryStrategy {
    max_retries: u32,
    base_delay_secs: u64,
}

impl RetryStrategy {
    /// Creates a new retry strategy with the specified maximum retry count.
    ///
    /// # Arguments
    /// * `max_retries` - Maximum number of retry attempts before giving up
    pub fn new(max_retries: u32) -> Self {
        Self {
            max_retries,
            base_delay_secs: 2,
        }
    }

    /// Returns whether a retry should be attempted for the given attempt number.
    ///
    /// # Arguments
    /// * `attempt` - The current attempt number (1-based)
    pub fn should_retry(&self, attempt: u32) -> bool {
        attempt <= self.max_retries
    }

    /// Calculates the exponential backoff delay for a given attempt.
    ///
    /// Formula: base_delay * 2^attempt
    ///
    /// # Arguments
    /// * `attempt` - The current attempt number (0-based)
    pub fn calculate_delay(&self, attempt: u32) -> Duration {
        let delay = self.base_delay_secs * 2u64.pow(attempt);
        Duration::from_secs(delay)
    }

    /// Returns the maximum number of retries.
    pub fn max_retries(&self) -> u32 {
        self.max_retries
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_retry_strategy_within_limit() {
        let strategy = RetryStrategy::new(2);

        assert!(strategy.should_retry(1));
        assert!(strategy.should_retry(2));
    }

    #[test]
    fn test_retry_strategy_exceeds_limit() {
        let strategy = RetryStrategy::new(2);
        assert!(!strategy.should_retry(3));
    }

    #[test]
    fn test_retry_strategy_zero_max_retries() {
        let strategy = RetryStrategy::new(0);
        assert!(!strategy.should_retry(1));
    }

    #[test]
    fn test_calculate_delay_exponential_growth() {
        let strategy = RetryStrategy::new(3);

        assert_eq!(strategy.calculate_delay(0), Duration::from_secs(2));
        assert_eq!(strategy.calculate_delay(1), Duration::from_secs(4));
        assert_eq!(strategy.calculate_delay(2), Duration::from_secs(8));
    }

    #[test]
    fn test_max_retries_accessor() {
        let strategy = RetryStrategy::new(5);
        assert_eq!(strategy.max_retries(), 5);
    }
}
