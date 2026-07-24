//! Public data types — [`CleanupConfig`] and
//! [`CleanupMetrics`] — plus the
//! `AddAssign<&Self> for CleanupMetrics` impl used to
//! aggregate per-user / per-pass results inside the
//! [`super::CleanupDaemon`] orchestration loop.

use std::{ops::AddAssign, time::Duration};

/// Configuration for the cleanup daemon.
#[derive(Debug, Clone)]
pub struct CleanupConfig {
    /// How often to run the cleanup cycle.
    pub check_interval: Duration,
    /// Delete completed sessions older than this duration.
    pub session_retention_hours: u64,
    /// Delete temp files older than this duration.
    pub temp_file_ttl_hours: u64,
    /// Maximum number of checkpoints to keep per session.
    pub max_checkpoints_per_session: usize,
}

impl Default for CleanupConfig {
    fn default() -> Self {
        Self {
            check_interval: Duration::from_secs(3600), // 1 hour
            session_retention_hours: 168,              // 7 days
            temp_file_ttl_hours: 24,
            max_checkpoints_per_session: 5,
        }
    }
}

/// Metrics collected during a single cleanup cycle.
#[derive(Debug, Clone, Default)]
pub struct CleanupMetrics {
    pub sessions_deleted: usize,
    pub files_deleted: usize,
    pub checkpoints_deleted: usize,
    pub bytes_reclaimed: u64,
}

impl AddAssign for CleanupMetrics {
    fn add_assign(&mut self, rhs: Self) {
        self.sessions_deleted += rhs.sessions_deleted;
        self.files_deleted += rhs.files_deleted;
        self.checkpoints_deleted += rhs.checkpoints_deleted;
        self.bytes_reclaimed += rhs.bytes_reclaimed;
    }
}
