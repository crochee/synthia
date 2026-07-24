//! Guardian circuit breaker for tracking denial patterns per session.
//!
//! Implements the fail-fast principle: after consecutive_denials reaches 3
//! or total_denials reaches 10, the circuit breaker triggers session interrupt.

/// Circuit breaker for tracking Guardian denials per session.
///
/// Triggers session interrupt after 3 consecutive denials or 10 total denials.
#[derive(Debug, Clone)]
pub struct GuardianCircuitBreaker {
    consecutive_denials: u8,
    total_denials: u32,
    session_interrupt: bool,
}

impl GuardianCircuitBreaker {
    /// Creates a new circuit breaker with default thresholds (3 consecutive / 10 total)
    #[must_use]
    pub fn new() -> Self {
        Self {
            consecutive_denials: 0,
            total_denials: 0,
            session_interrupt: false,
        }
    }

    /// Records a Guardian denial, updates counters, checks thresholds
    pub fn record_denial(&mut self) {
        self.consecutive_denials += 1;
        self.total_denials += 1;

        if self.consecutive_denials >= 3 || self.total_denials >= 10 {
            tracing::warn!(
                consecutive = self.consecutive_denials,
                total = self.total_denials,
                "Guardian circuit breaker triggered - session interrupt"
            );
            self.session_interrupt = true;
        }
    }

    /// Records a Guardian approval, resets consecutive counter
    pub fn record_approval(&mut self) {
        self.consecutive_denials = 0;
    }

    /// Returns true if session should be interrupted
    #[must_use]
    pub fn should_interrupt(&self) -> bool {
        self.session_interrupt
    }

    /// Resets all counters and interrupt flag
    pub fn reset(&mut self) {
        self.consecutive_denials = 0;
        self.total_denials = 0;
        self.session_interrupt = false;
    }

    /// Returns current consecutive denial count
    #[must_use]
    pub fn consecutive_denials(&self) -> u8 {
        self.consecutive_denials
    }

    /// Returns total denial count
    #[must_use]
    pub fn total_denials(&self) -> u32 {
        self.total_denials
    }
}

impl Default for GuardianCircuitBreaker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_consecutive_denials_triggers_interrupt() {
        let mut cb = GuardianCircuitBreaker::new();
        assert!(!cb.should_interrupt());

        cb.record_denial();
        cb.record_denial();
        assert!(!cb.should_interrupt());

        cb.record_denial(); // 3rd consecutive
        assert!(cb.should_interrupt());
    }

    #[test]
    fn test_total_denials_counter_increments() {
        let mut cb = GuardianCircuitBreaker::new();

        // Record 9 denials with approvals in between to test total counter
        for _ in 1..=9 {
            cb.record_denial();
            cb.record_approval(); // Reset consecutive but total keeps incrementing
        }
        // After 9 denials (even with approvals), total should be 9
        assert_eq!(cb.total_denials(), 9);
        assert!(!cb.should_interrupt()); // But consecutive is 0 due to approvals

        // One more denial brings total to 10 but consecutive to 1
        cb.record_denial();
        assert_eq!(cb.total_denials(), 10);
        // Note: interrupt fires because total >= 10, not because consecutive >= 3
        assert!(cb.should_interrupt());
    }

    #[test]
    fn test_approval_resets_consecutive() {
        let mut cb = GuardianCircuitBreaker::new();
        cb.record_denial();
        cb.record_denial();
        assert_eq!(cb.consecutive_denials(), 2);

        cb.record_approval();
        assert_eq!(cb.consecutive_denials(), 0);
    }

    #[test]
    fn test_interrupt_persists_after_approval() {
        let mut cb = GuardianCircuitBreaker::new();
        cb.record_denial();
        cb.record_denial();
        cb.record_denial(); // interrupt triggered
        assert!(cb.should_interrupt());

        cb.record_approval();
        assert!(cb.should_interrupt()); // still true
    }

    #[test]
    fn test_reset_clears_all() {
        let mut cb = GuardianCircuitBreaker::new();
        cb.record_denial();
        cb.record_denial();
        cb.record_denial();
        assert!(cb.should_interrupt());

        cb.reset();
        assert!(!cb.should_interrupt());
        assert_eq!(cb.consecutive_denials(), 0);
        assert_eq!(cb.total_denials(), 0);
    }

    #[test]
    fn test_interleaved_denials_and_approvals() {
        let mut cb = GuardianCircuitBreaker::new();

        // Deny, approve, deny, approve, deny, deny, deny (3 consecutive)
        cb.record_denial(); // 1 total
        cb.record_approval();

        cb.record_denial(); // 2 total
        cb.record_approval();

        cb.record_denial(); // 3 total
        cb.record_denial(); // 4 total
        cb.record_denial(); // 5 total - 3 consecutive now
        assert!(cb.should_interrupt());
    }
}
