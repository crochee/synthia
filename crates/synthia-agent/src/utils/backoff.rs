//! Backoff utility module
//!
//! Provides exponential backoff with jitter for retry logic.

use std::time::Duration;

/// Standard exponential backoff with jitter
/// INITIAL_DELAY_MS=200, BACKOFF_FACTOR=2.0, jitter ∈ [0.9, 1.1]
pub fn backoff(attempt: u64) -> Duration {
    const INITIAL_DELAY_MS: u64 = 200;
    const BACKOFF_FACTOR: u64 = 2;
    const MAX_ATTEMPT: u64 = 10;

    let attempt = attempt.min(MAX_ATTEMPT);
    let base = INITIAL_DELAY_MS * BACKOFF_FACTOR.pow(attempt as u32);

    // Generate jitter in range [0.9, 1.1]
    use std::time::Instant;
    let now = Instant::now();
    let nanos = now.elapsed().as_nanos();
    let jitter_factor = 0.9 + ((nanos as f64 % 200.0) / 1000.0);

    let delay_ms = (base as f64 * jitter_factor) as u64;
    Duration::from_millis(delay_ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backoff_increases() {
        let b0 = backoff(0);
        let b1 = backoff(1);
        let b2 = backoff(2);
        assert!(b1 > b0);
        assert!(b2 > b1);
    }
}
