//! Session-cleanup pass — public
//! [`cleanup_expired_sessions`] (sweeps every user
//! namespace) and private
//! [`cleanup_expired_sessions_for_user`] (per-user
//! terminal-state + age filter).
//!
//! Both are free functions (not methods on
//! [`super::CleanupDaemon`]) so the test suite can invoke
//! them directly without going through
//! [`super::daemon::CleanupDaemon::run_cycle`].

use chrono::{DateTime, Utc};
use synthia_session::{Store, types::SessionState};
use tracing::{info, warn};

use super::{
    types::{CleanupConfig, CleanupMetrics},
    util::dir_size,
};

/// Delete completed/cancelled/error sessions older than the retention period.
///
/// Sweeps every user namespace reported by
/// [`Store::list_user_ids`] so that no user is left behind
/// (and no user is implicitly shared). Per-user results are
/// aggregated into the returned metrics.
pub fn cleanup_expired_sessions(
    config: &CleanupConfig,
    session_store: &Store,
) -> CleanupMetrics {
    let mut metrics = CleanupMetrics::default();
    let cutoff = Utc::now()
        - chrono::Duration::hours(config.session_retention_hours as i64);

    let user_ids = match session_store.list_user_ids() {
        Ok(ids) => ids,
        Err(e) => {
            warn!(error = %e, "failed to list user ids for cleanup");
            return metrics;
        }
    };

    for user_id in &user_ids {
        metrics += cleanup_expired_sessions_for_user(
            config,
            session_store,
            user_id,
            cutoff,
        );
    }
    metrics
}

/// Per-user sweep for completed/cancelled/error sessions older
/// than the retention period.
fn cleanup_expired_sessions_for_user(
    _config: &CleanupConfig,
    session_store: &Store,
    user_id: &str,
    cutoff: DateTime<Utc>,
) -> CleanupMetrics {
    let mut metrics = CleanupMetrics::default();

    let sessions = match session_store.list_sessions_with_metadata(user_id) {
        Ok(s) => s,
        Err(e) => {
            warn!(
                user_id = %user_id,
                error = %e,
                "failed to list sessions for cleanup"
            );
            return metrics;
        }
    };

    for session in &sessions {
        let is_terminal = matches!(
            session.state,
            SessionState::Completed
                | SessionState::Cancelled
                | SessionState::Error
        );

        if !is_terminal {
            continue;
        }

        let updated_at = match DateTime::parse_from_rfc3339(&session.updated_at)
        {
            Ok(dt) => dt.with_timezone(&Utc),
            Err(e) => {
                warn!(
                    user_id = %user_id,
                    session_id = %session.id,
                    error = %e,
                    "failed to parse session updated_at, skipping"
                );
                continue;
            }
        };

        if updated_at < cutoff {
            let dir = session_store.session_dir(user_id, &session.id);
            let size = dir_size(&dir);
            if let Err(e) = session_store.delete_session(user_id, &session.id) {
                warn!(
                    user_id = %user_id,
                    session_id = %session.id,
                    error = %e,
                    "failed to delete expired session"
                );
            } else {
                info!(
                    user_id = %user_id,
                    session_id = %session.id,
                    age_hours = (cutoff - updated_at).num_hours(),
                    bytes_reclaimed = size,
                    "deleted expired session"
                );
                metrics.sessions_deleted += 1;
                metrics.bytes_reclaimed += size;
            }
        }
    }

    metrics
}
