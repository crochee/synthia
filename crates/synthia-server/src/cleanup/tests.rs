//! Unit tests for [`super::CleanupDaemon`].
//!
//! Covers:
//! - 2 session-cleanup tests
//!   (`test_cleanup_expired_session`,
//!   `test_cleanup_sweeps_multiple_users`).
//! - 1 temp-file test (`test_cleanup_temp_files`).
//! - 1 checkpoint-rotation test (`test_checkpoint_rotation`).
//! - 1 orphan-checkpoint test (`test_orphaned_checkpoint_cleanup`).
//! - 1 metrics accumulation test
//!   (`test_cleanup_metrics_accumulation`).
//! - 1 default-config test (`test_default_config_values`).
//!
//! The session / checkpoint / orphan tests use
//! `synthia_session::Store` directly under a
//! `tempfile::TempDir` to keep the on-disk layout under
//! test (no mocks for filesystem behaviour).
//!
//! The temp-file test uses
//! `filetime::set_file_mtime` to set deterministic
//! modification times so the TTL filter exercises its
//! "older than" branch reproducibly.

use std::{path::Path, time::Duration};

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use synthia_session::{
    Store,
    types::{Session, SessionState},
};
use tempfile::TempDir;

use super::{
    CleanupDaemon,
    checkpoints::{cleanup_orphaned_checkpoints, rotate_checkpoints},
    sessions::cleanup_expired_sessions,
    temp_files::cleanup_temp_files,
    types::{CleanupConfig, CleanupMetrics},
};

/// Test user namespace. Picked to be distinct from
/// `SERVER_DEFAULT_USER_ID` so that the new
/// `list_user_ids` based cleanup is exercised.
const TEST_USER: &str = "test-user";

fn make_store() -> (Store, TempDir) {
    let temp = TempDir::new().unwrap();
    let root = temp.path().to_path_buf();
    (Store::new(root), temp)
}

fn make_session(
    id: &str,
    state: SessionState,
    updated_at: DateTime<Utc>,
) -> Session {
    let mut s = Session::new_with_user(id.to_string(), TEST_USER.to_string())
        .expect("non-empty user_id");
    s.state = state;
    // Directly set updated_at via field access
    s.updated_at = updated_at;
    s.created_at = updated_at - ChronoDuration::hours(200);
    s
}

#[tokio::test]
async fn test_cleanup_expired_session() {
    let (store, _temp) = make_store();

    // Create a completed session that is 200 hours old (older than default 168h).
    let old_time = Utc::now() - ChronoDuration::hours(200);
    let session =
        make_session("old-completed", SessionState::Completed, old_time);
    store.save_metadata(&session).unwrap();

    // Create a recent completed session (should not be deleted).
    let recent_time = Utc::now() - ChronoDuration::hours(2);
    let recent_session =
        make_session("recent-completed", SessionState::Completed, recent_time);
    store.save_metadata(&recent_session).unwrap();

    // Create an active session (should not be deleted regardless of age).
    let old_active =
        make_session("old-active", SessionState::WaitingForInput, old_time);
    store.save_metadata(&old_active).unwrap();

    assert!(store.session_exists(TEST_USER, "old-completed"));
    assert!(store.session_exists(TEST_USER, "recent-completed"));
    assert!(store.session_exists(TEST_USER, "old-active"));

    let config = CleanupConfig::default();
    let metrics = CleanupDaemon::cleanup_expired_sessions(&config, &store);

    assert_eq!(metrics.sessions_deleted, 1);
    assert!(!store.session_exists(TEST_USER, "old-completed"));
    assert!(store.session_exists(TEST_USER, "recent-completed"));
    assert!(store.session_exists(TEST_USER, "old-active"));
}

/// Verify that the new `list_user_ids`-based sweep actually
/// visits every user namespace — i.e. an expired session in a
/// second user directory is also cleaned up, not just the
/// first one.
#[tokio::test]
async fn test_cleanup_sweeps_multiple_users() {
    let (store, _temp) = make_store();

    let old_time = Utc::now() - ChronoDuration::hours(200);
    for user in ["alice", "bob"] {
        let session =
            make_session("old-completed", SessionState::Completed, old_time);
        // Re-stamp user_id to match the loop variable.
        let mut per_user = session;
        per_user.user_id = user.to_string();
        store.save_metadata(&per_user).unwrap();
        assert!(store.session_exists(user, "old-completed"));
    }

    let config = CleanupConfig::default();
    let metrics = cleanup_expired_sessions(&config, &store);

    // Both namespaces should have been swept.
    assert_eq!(metrics.sessions_deleted, 2);
    assert!(!store.session_exists("alice", "old-completed"));
    assert!(!store.session_exists("bob", "old-completed"));
}

#[tokio::test]
async fn test_cleanup_temp_files() {
    let temp_dir = TempDir::new().unwrap();
    let workspace_root = temp_dir.path();

    // Create .agents/tmp/ directory structure.
    let tmp_dir = workspace_root.join(".agents").join("tmp");
    std::fs::create_dir_all(&tmp_dir).unwrap();

    // Create an old temp file.
    let old_file = tmp_dir.join("old-file.txt");
    std::fs::write(&old_file, "old content").unwrap();
    // Set modification time to 48 hours ago.
    let old_time = filetime::FileTime::from_unix_time(
        (Utc::now() - ChronoDuration::hours(48)).timestamp(),
        0,
    );
    filetime::set_file_mtime(&old_file, old_time).unwrap();

    // Create a recent temp file.
    let new_file = tmp_dir.join("new-file.txt");
    std::fs::write(&new_file, "new content").unwrap();

    assert!(old_file.exists());
    assert!(new_file.exists());

    let config = CleanupConfig {
        temp_file_ttl_hours: 24,
        ..Default::default()
    };
    let metrics = cleanup_temp_files(&config, workspace_root);

    assert_eq!(metrics.files_deleted, 1);
    assert!(!old_file.exists());
    assert!(new_file.exists());
}

#[tokio::test]
async fn test_checkpoint_rotation() {
    let (store, _temp) = make_store();

    // Create a session directory with checkpoints.
    let mut session = Session::new_with_user(
        "rotation-test".to_string(),
        TEST_USER.to_string(),
    )
    .expect("non-empty user_id");
    session.state = SessionState::Completed;
    store.save_metadata(&session).unwrap();

    let checkpoint_dir = store
        .session_dir(TEST_USER, "rotation-test")
        .join("checkpoints");
    std::fs::create_dir_all(&checkpoint_dir).unwrap();

    // Create 7 checkpoints with staggered timestamps.
    for i in 0..7 {
        let path = checkpoint_dir.join(format!("checkpoint_{i}.json"));
        std::fs::write(&path, format!("checkpoint data {i}")).unwrap();
        // Set each checkpoint to be 1 hour apart.
        let mtime = filetime::FileTime::from_unix_time(
            Utc::now().timestamp() - (7 - i) * 3600,
            0,
        );
        filetime::set_file_mtime(&path, mtime).unwrap();
    }

    let config = CleanupConfig {
        max_checkpoints_per_session: 5,
        ..Default::default()
    };
    let metrics =
        rotate_checkpoints(&config, &store, Path::new("/nonexistent"));

    assert_eq!(metrics.checkpoints_deleted, 2);

    // Verify oldest 2 checkpoints were deleted.
    assert!(!checkpoint_dir.join("checkpoint_0.json").exists());
    assert!(!checkpoint_dir.join("checkpoint_1.json").exists());
    assert!(checkpoint_dir.join("checkpoint_2.json").exists());
    assert!(checkpoint_dir.join("checkpoint_6.json").exists());
}

#[tokio::test]
async fn test_orphaned_checkpoint_cleanup() {
    let temp = TempDir::new().unwrap();
    let workspace_root = temp.path();

    // Create the sessions root under workspace.
    let sessions_root = workspace_root.join("sessions");
    let store = Store::new(sessions_root.clone());

    // Create a valid session.
    let valid_session = Session::new_with_user(
        "valid-session".to_string(),
        TEST_USER.to_string(),
    )
    .expect("non-empty user_id");
    store.save_metadata(&valid_session).unwrap();

    // Manually create a directory for a "deleted" session with checkpoints.
    let orphan_dir = sessions_root.join(TEST_USER).join("orphan-session");
    let orphan_checkpoint_dir = orphan_dir.join("checkpoints");
    std::fs::create_dir_all(&orphan_checkpoint_dir).unwrap();
    std::fs::write(orphan_checkpoint_dir.join("orphan.json"), "data").unwrap();

    let config = CleanupConfig::default();
    let metrics = cleanup_orphaned_checkpoints(&config, &store, workspace_root);

    assert_eq!(metrics.checkpoints_deleted, 1);
    assert!(!orphan_checkpoint_dir.exists());
    // Valid session's checkpoint directory should not be touched.
}

#[test]
fn test_cleanup_metrics_accumulation() {
    let mut m1 = CleanupMetrics {
        sessions_deleted: 1,
        files_deleted: 5,
        checkpoints_deleted: 2,
        bytes_reclaimed: 1000,
    };
    let m2 = CleanupMetrics {
        sessions_deleted: 3,
        files_deleted: 10,
        checkpoints_deleted: 0,
        bytes_reclaimed: 5000,
    };
    m1 += m2;

    assert_eq!(m1.sessions_deleted, 4);
    assert_eq!(m1.files_deleted, 15);
    assert_eq!(m1.checkpoints_deleted, 2);
    assert_eq!(m1.bytes_reclaimed, 6000);
}

#[test]
fn test_default_config_values() {
    let config = CleanupConfig::default();
    assert_eq!(config.check_interval, Duration::from_secs(3600));
    assert_eq!(config.session_retention_hours, 168);
    assert_eq!(config.temp_file_ttl_hours, 24);
    assert_eq!(config.max_checkpoints_per_session, 5);
}
