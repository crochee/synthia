//! Checkpoint-management pass — 3 private methods:
//!
//! - [`rotate_checkpoints`] — per-session cap enforcement
//!   (sweeps every user namespace, then calls the
//!   per-user variant).
//! - [`rotate_checkpoints_for_user`] — per-session
//!   `max_checkpoints_per_session` enforcement, oldest
//!   first.
//! - [`cleanup_orphaned_checkpoints`] — removes
//!   `checkpoints/` for sessions whose metadata is gone
//!   (a session without metadata is a deleted session, and
//!   its checkpoints are orphans from the running
//!   daemon's perspective).

use std::{
    fs,
    path::{Path, PathBuf},
};

use synthia_session::Store;
use tracing::{info, warn};

use super::{
    types::{CleanupConfig, CleanupMetrics},
    util::{dir_size, file_size},
};

/// Enforce max checkpoints per session by deleting the oldest on overflow.
///
/// Checkpoints are stored under `{session_dir}/checkpoints/`.
/// Sweeps every user namespace reported by
/// [`Store::list_user_ids`].
pub(super) fn rotate_checkpoints(
    config: &CleanupConfig,
    session_store: &Store,
    _workspace_root: &Path,
) -> CleanupMetrics {
    let mut metrics = CleanupMetrics::default();

    let user_ids = match session_store.list_user_ids() {
        Ok(ids) => ids,
        Err(e) => {
            warn!(error = %e, "failed to list user ids for checkpoint rotation");
            return metrics;
        }
    };

    for user_id in &user_ids {
        metrics += rotate_checkpoints_for_user(config, session_store, user_id);
    }

    if metrics.checkpoints_deleted > 0 {
        info!(
            checkpoints_deleted = metrics.checkpoints_deleted,
            bytes_reclaimed = metrics.bytes_reclaimed,
            "rotated checkpoints"
        );
    }

    metrics
}

fn rotate_checkpoints_for_user(
    config: &CleanupConfig,
    session_store: &Store,
    user_id: &str,
) -> CleanupMetrics {
    let mut metrics = CleanupMetrics::default();

    let sessions = match session_store.list_sessions_with_metadata(user_id) {
        Ok(s) => s,
        Err(e) => {
            warn!(
                user_id = %user_id,
                error = %e,
                "failed to list sessions for checkpoint rotation"
            );
            return metrics;
        }
    };

    for session in &sessions {
        let checkpoint_dir = session_store
            .session_dir(user_id, &session.id)
            .join("checkpoints");

        if !checkpoint_dir.exists() {
            continue;
        }

        let mut checkpoints: Vec<(PathBuf, std::time::SystemTime)> = Vec::new();

        if let Ok(entries) = fs::read_dir(&checkpoint_dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.is_file()
                    && let Ok(metadata) = entry.metadata()
                    && let Ok(modified) = metadata.modified()
                {
                    checkpoints.push((path, modified));
                }
            }
        }

        // Sort by modification time, oldest first.
        checkpoints.sort_by_key(|(_, t)| *t);

        let max = config.max_checkpoints_per_session;
        while checkpoints.len() > max {
            let (oldest, _) = checkpoints.remove(0);
            let size = file_size(&oldest);
            if fs::remove_file(&oldest).is_ok() {
                metrics.checkpoints_deleted += 1;
                metrics.bytes_reclaimed += size;
            }
        }
    }

    metrics
}

/// Detect and clean up checkpoints for sessions that no longer exist.
///
/// Walks every per-user directory under
/// `sessions_root/<user_id>/<session_id>/checkpoints/` and
/// removes the `checkpoints/` subtree when `<session_id>` has
/// no metadata file (or its metadata is unreadable). Sessions
/// with valid metadata are left alone; their per-session
/// `checkpoints/` directory is owned by the active session and
/// is managed by [`rotate_checkpoints`].
pub(super) fn cleanup_orphaned_checkpoints(
    config: &CleanupConfig,
    session_store: &Store,
    workspace_root: &Path,
) -> CleanupMetrics {
    let _ = config; // Reserved for future configuration.
    let mut metrics = CleanupMetrics::default();

    let user_ids = match session_store.list_user_ids() {
        Ok(ids) => ids,
        Err(e) => {
            warn!(error = %e, "failed to list user ids for orphan detection");
            return metrics;
        }
    };

    // Scan all directories under the sessions root.
    let sessions_root = workspace_root.join("sessions");
    if !sessions_root.exists() {
        return metrics;
    }

    for user_id in &user_ids {
        // Valid session ids for this user namespace: only
        // sessions with a parseable metadata.json count as
        // "still alive". We deliberately do NOT use
        // `list_sessions_with_metadata` here because it errors
        // out on the first missing metadata file, masking
        // real orphans.
        let valid_sessions: Vec<String> =
            match session_store.list_session_ids(user_id) {
                Ok(ids) => ids
                    .into_iter()
                    .filter(|id| {
                        session_store
                            .load_metadata(user_id, id)
                            .map(|m| m.owner_user_id == *user_id)
                            .unwrap_or(false)
                    })
                    .collect(),
                Err(e) => {
                    warn!(
                        user_id = %user_id,
                        error = %e,
                        "failed to list sessions for orphan detection"
                    );
                    continue;
                }
            };

        let user_dir = sessions_root.join(user_id);
        let entries = match fs::read_dir(&user_dir) {
            Ok(e) => e,
            Err(e) => {
                warn!(
                    dir = %user_dir.display(),
                    error = %e,
                    "failed to read user directory"
                );
                continue;
            }
        };

        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            let session_id = match path.file_name().and_then(|n| n.to_str()) {
                Some(n) => n.to_string(),
                None => continue,
            };

            // Skip valid sessions.
            if valid_sessions.contains(&session_id) {
                continue;
            }

            // Check if this directory has a checkpoints subdirectory.
            let checkpoint_dir = path.join("checkpoints");
            if checkpoint_dir.exists() {
                let size = dir_size(&checkpoint_dir);
                if let Err(e) = fs::remove_dir_all(&checkpoint_dir) {
                    warn!(
                        user_id = %user_id,
                        session_id = %session_id,
                        error = %e,
                        "failed to remove orphaned checkpoints"
                    );
                } else {
                    metrics.checkpoints_deleted += 1;
                    metrics.bytes_reclaimed += size;
                    info!(
                        user_id = %user_id,
                        session_id = %session_id,
                        bytes_reclaimed = size,
                        "cleaned up orphaned checkpoints"
                    );
                }
            }
        }
    }

    metrics
}
