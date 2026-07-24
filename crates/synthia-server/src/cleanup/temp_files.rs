//! Temp-file cleanup pass — both methods are private
//! (the daemon invokes them through
//! [`super::daemon::CleanupDaemon::run_temp_files_cleanup`]).
//!
//! [`cleanup_temp_files`] is the public-facing TTL filter on
//! `.agents/tmp/`. [`walk_and_delete`] is the recursive
//! walker with empty-dir pruning — kept as a separate
//! function so the test suite can exercise it in isolation
//! (the test for it lives in
//! [`super::daemon`] via the
//! `test_cleanup_temp_files` case).

use std::{fs, path::Path};

use chrono::{DateTime, Utc};
use tracing::{info, warn};

use super::types::{CleanupConfig, CleanupMetrics};

/// Delete files in `.agents/tmp/` older than the configured TTL.
pub(super) fn cleanup_temp_files(
    config: &CleanupConfig,
    workspace_root: &Path,
) -> CleanupMetrics {
    let mut metrics = CleanupMetrics::default();
    let tmp_dir = workspace_root.join(".agents").join("tmp");

    if !tmp_dir.exists() {
        return metrics;
    }

    let cutoff =
        Utc::now() - chrono::Duration::hours(config.temp_file_ttl_hours as i64);

    // Walk files recursively in the tmp directory.
    walk_and_delete(&tmp_dir, &cutoff, &mut metrics);

    if metrics.files_deleted > 0 {
        info!(
            files_deleted = metrics.files_deleted,
            bytes_reclaimed = metrics.bytes_reclaimed,
            "cleaned up temp files"
        );
    }

    metrics
}

/// Recursively walk a directory tree, deleting files older than the cutoff.
fn walk_and_delete(
    dir: &Path,
    cutoff: &DateTime<Utc>,
    metrics: &mut CleanupMetrics,
) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => {
            warn!(dir = %dir.display(), error = %e, "failed to read directory");
            return;
        }
    };

    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_dir() {
            // Recurse into subdirectories.
            walk_and_delete(&path, cutoff, metrics);
            // If the subdirectory is now empty, remove it.
            if let Ok(entries) = fs::read_dir(&path)
                && entries.count() == 0
            {
                let _ = fs::remove_dir(&path);
            }
        } else if let Ok(metadata) = entry.metadata()
            && let Ok(modified) = metadata.modified()
        {
            let modified_utc = DateTime::<Utc>::from(modified);
            if modified_utc < *cutoff {
                let size = metadata.len();
                if fs::remove_file(&path).is_ok() {
                    metrics.files_deleted += 1;
                    metrics.bytes_reclaimed += size;
                }
            }
        }
    }
}
