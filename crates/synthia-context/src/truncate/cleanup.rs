//! Cleanup helpers for offloaded tool output.
//!
//! Files older than the configured retention period are removed
//! recursively from the tool-output store. Both synchronous and
//! asynchronous variants are provided.

use std::{
    path::Path,
    time::{Duration, SystemTime},
};

/// Delete files under `base_dir` whose modification time is older than
/// `retention`. Returns the number of files deleted.
///
/// Directories are only removed when they become empty after their
/// contained files are deleted; this prevents deleting nested session
/// directories that still hold recent output.
pub fn cleanup_tool_output_store(
    base_dir: &Path,
    retention: Duration,
) -> std::io::Result<usize> {
    let now = SystemTime::now();
    let mut deleted = 0usize;
    cleanup_dir(base_dir, retention, now, &mut deleted)?;
    Ok(deleted)
}

fn cleanup_dir(
    dir: &Path,
    retention: Duration,
    now: SystemTime,
    deleted: &mut usize,
) -> std::io::Result<()> {
    if !dir.exists() {
        return Ok(());
    }

    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;

        if file_type.is_dir() {
            cleanup_dir(&path, retention, now, deleted)?;
            // Best-effort removal of empty directories; ignore errors.
            let _ = std::fs::remove_dir(&path);
        } else if file_type.is_file() {
            let should_delete = entry
                .metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|modified| now.duration_since(modified).ok())
                .map(|age| age > retention)
                .unwrap_or(false);

            if should_delete {
                std::fs::remove_file(&path)?;
                *deleted += 1;
            }
        }
    }

    Ok(())
}

/// Async variant of [`cleanup_tool_output_store`].
///
/// The synchronous walk and deletion are offloaded to Tokio's blocking
/// pool so the calling async task is not blocked on filesystem I/O.
pub async fn cleanup_tool_output_store_async(
    base_dir: &Path,
    retention: Duration,
) -> tokio::io::Result<usize> {
    let base_dir = base_dir.to_path_buf();
    tokio::task::spawn_blocking(move || {
        cleanup_tool_output_store(&base_dir, retention)
    })
    .await
    .map_err(tokio::io::Error::other)?
    .map_err(tokio::io::Error::other)
}

/// Standard retention period for offloaded tool output: 7 days.
pub const DEFAULT_RETENTION: Duration = Duration::from_secs(7 * 24 * 60 * 60);
