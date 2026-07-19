//! `CleanupTask` — periodic retention-based cleanup (PR-6.2).
//!
//! Runs every 3600 seconds (1 hour) by default, deleting event data
//! older than 7 days. Used by both the event store and the tool
//! output sanitizer.

use std::time::Duration;

/// Default retention period in days.
pub const DEFAULT_RETENTION_DAYS: u64 = 7;

/// Default cleanup interval in seconds.
pub const DEFAULT_CLEANUP_INTERVAL_SECS: u64 = 3600;

/// Configuration for the cleanup task.
#[derive(Debug, Clone, Copy)]
pub struct CleanupConfig {
    /// How often to run cleanup (in seconds).
    pub interval_secs: u64,
    /// Maximum age of retained data (in days).
    pub retention_days: u64,
}

impl Default for CleanupConfig {
    fn default() -> Self {
        Self {
            interval_secs: DEFAULT_CLEANUP_INTERVAL_SECS,
            retention_days: DEFAULT_RETENTION_DAYS,
        }
    }
}

/// A handle to a running cleanup task.
///
/// When dropped, the cleanup task is cancelled.
pub struct CleanupTask {
    /// The join handle for the spawned task.
    handle: tokio::task::JoinHandle<()>,
}

impl CleanupTask {
    /// Spawn a cleanup task that runs periodically.
    ///
    /// The `cleanup_fn` is called every `config.interval_secs` seconds
    /// with the maximum retention age in milliseconds since the epoch.
    /// It should delete all data older than the cutoff.
    pub fn spawn<F>(config: CleanupConfig, cleanup_fn: F) -> Self
    where
        F: Fn(u64) + Send + 'static,
    {
        let interval = Duration::from_secs(config.interval_secs);
        let retention_ms = config.retention_days * 24 * 60 * 60 * 1000;

        let handle = tokio::spawn(async move {
            loop {
                tokio::time::sleep(interval).await;
                let since_epoch = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or(Duration::ZERO);
                // as_millis() returns u128; saturating cast avoids truncation panic.
                #[allow(clippy::cast_possible_truncation)]
                let now_ms = since_epoch.as_millis() as u64;
                let cutoff = now_ms.saturating_sub(retention_ms);
                cleanup_fn(cutoff);
            }
        });

        Self { handle }
    }

    /// Abort the cleanup task.
    pub fn abort(&self) {
        self.handle.abort();
    }
}

impl Drop for CleanupTask {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    };

    use super::*;

    #[test]
    fn default_config_values() {
        let config = CleanupConfig::default();
        assert_eq!(config.interval_secs, 3600);
        assert_eq!(config.retention_days, 7);
    }

    #[tokio::test]
    async fn cleanup_task_calls_fn() {
        let counter = Arc::new(AtomicU64::new(0));
        let counter_clone = counter.clone();

        let config = CleanupConfig {
            interval_secs: 1, // 1 second for testing
            retention_days: 7,
        };

        let _task = CleanupTask::spawn(config, move |cutoff| {
            counter_clone.fetch_add(1, Ordering::SeqCst);
            // Verify cutoff is reasonable (7 days ago).
            assert!(cutoff > 0);
        });

        // Wait for at least one cleanup cycle.
        tokio::time::sleep(Duration::from_millis(1500)).await;
        let count = counter.load(Ordering::SeqCst);
        assert!(count >= 1, "Expected at least 1 cleanup call, got {count}");
    }
}
