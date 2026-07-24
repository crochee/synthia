//! [`CleanupDaemon`] — the background Tokio task that
//! drives the cleanup pass on a configurable interval.
//!
//! The daemon itself is a unit struct: all state lives in
//! the borrowed arguments of each method. [`spawn`]
//! returns a `JoinHandle` that owns the loop; [`run_cycle`]
//! is the single-pass entry point used by the test suite
//! (the background loop also calls it once per tick).
//!
//! [`spawn`]: CleanupDaemon::spawn
//! [`run_cycle`]: CleanupDaemon::run_cycle

use std::{path::Path, sync::Arc};

use synthia_session::Store;
use tokio::task::JoinHandle;
use tracing::info;

use super::{
    checkpoints::{cleanup_orphaned_checkpoints, rotate_checkpoints},
    sessions::cleanup_expired_sessions,
    temp_files::cleanup_temp_files,
    types::{CleanupConfig, CleanupMetrics},
};

/// Background daemon that periodically cleans up resources.
pub struct CleanupDaemon;

impl CleanupDaemon {
    /// Spawn the cleanup daemon as a background Tokio task.
    ///
    /// Returns a `JoinHandle` that can be used to await or abort the daemon.
    pub fn spawn(
        config: &CleanupConfig,
        session_store: Arc<Store>,
        workspace_root: &Path,
    ) -> JoinHandle<()> {
        let config = config.clone();
        let workspace_root = workspace_root.to_path_buf();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(config.check_interval);
            // Skip the first tick so we don't run immediately on startup.
            interval.set_missed_tick_behavior(
                tokio::time::MissedTickBehavior::Skip,
            );
            interval.tick().await;

            loop {
                interval.tick().await;
                let metrics =
                    Self::run_cycle(&config, &session_store, &workspace_root)
                        .await;
                if metrics.sessions_deleted > 0
                    || metrics.files_deleted > 0
                    || metrics.checkpoints_deleted > 0
                {
                    info!(
                        sessions_deleted = metrics.sessions_deleted,
                        files_deleted = metrics.files_deleted,
                        checkpoints_deleted = metrics.checkpoints_deleted,
                        bytes_reclaimed = metrics.bytes_reclaimed,
                        "cleanup cycle completed"
                    );
                }
            }
        })
    }

    /// Execute one full cleanup cycle and return metrics.
    pub async fn run_cycle(
        config: &CleanupConfig,
        session_store: &Store,
        workspace_root: &Path,
    ) -> CleanupMetrics {
        let mut metrics = CleanupMetrics::default();

        metrics += Self::cleanup_expired_sessions(config, session_store);
        metrics += Self::cleanup_temp_files(config, workspace_root);
        metrics +=
            Self::rotate_checkpoints(config, session_store, workspace_root);
        metrics += Self::cleanup_orphaned_checkpoints(
            config,
            session_store,
            workspace_root,
        );

        metrics
    }

    // The 4 associated methods below are the "public
    // surface" the test suite and external callers invoke
    // as `CleanupDaemon::cleanup_expired_sessions(...)` etc.
    // They are thin pass-throughs to the per-concern free
    // functions in the sibling submodules — the per-concern
    // free functions are the ones that contain the actual
    // logic, the associated methods exist to keep the
    // original `CleanupDaemon::method(...)` call sites
    // working unchanged.

    /// Delete completed/cancelled/error sessions older than the retention period.
    ///
    /// Sweeps every user namespace reported by
    /// [`Store::list_user_ids`].
    pub fn cleanup_expired_sessions(
        config: &CleanupConfig,
        session_store: &Store,
    ) -> CleanupMetrics {
        cleanup_expired_sessions(config, session_store)
    }

    /// Delete files in `.agents/tmp/` older than the configured TTL.
    pub fn cleanup_temp_files(
        config: &CleanupConfig,
        workspace_root: &Path,
    ) -> CleanupMetrics {
        cleanup_temp_files(config, workspace_root)
    }

    /// Enforce max checkpoints per session by deleting the oldest on overflow.
    pub fn rotate_checkpoints(
        config: &CleanupConfig,
        session_store: &Store,
        workspace_root: &Path,
    ) -> CleanupMetrics {
        rotate_checkpoints(config, session_store, workspace_root)
    }

    /// Detect and clean up checkpoints for sessions that no longer exist.
    pub fn cleanup_orphaned_checkpoints(
        config: &CleanupConfig,
        session_store: &Store,
        workspace_root: &Path,
    ) -> CleanupMetrics {
        cleanup_orphaned_checkpoints(config, session_store, workspace_root)
    }
}
