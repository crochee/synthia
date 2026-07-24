//! [`DeadlockPrevention`] — utility class for detecting and
//! preventing deadlocks in reset operations.

/// Deadlock prevention utilities.
pub struct DeadlockPrevention;

impl DeadlockPrevention {
    /// Returns a timeout for reset operations.
    pub fn reset_timeout_secs() -> u64 {
        30
    }

    /// Checks if we're in a potential deadlock situation.
    ///
    /// # Arguments
    /// * `last_activity_timestamp` - Unix timestamp of last agent activity
    /// * `current_timestamp` - Current unix timestamp
    /// * `threshold_secs` - Seconds of inactivity to consider a deadlock
    pub fn is_deadlocked(
        last_activity_timestamp: u64,
        current_timestamp: u64,
        threshold_secs: u64,
    ) -> bool {
        current_timestamp - last_activity_timestamp > threshold_secs
    }

    /// Default deadlock detection threshold.
    pub fn default_threshold_secs() -> u64 {
        120 // 2 minutes
    }
}
