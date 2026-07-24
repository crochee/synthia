//! Integration tests for the resource cleanup daemon.
//!
//! Tests verify:
//! - Session retention: completed sessions older than `session_retention_hours` are deleted
//! - Temp file cleanup: files in `.agents/tmp/` older than `temp_file_ttl_hours` are deleted
//! - Cleanup daemon spawns and runs cycles correctly

use std::{fs, sync::Arc, time::Duration};

use chrono::Utc;
use filetime::{self, FileTime};
use synthia_server::cleanup::{CleanupConfig, CleanupDaemon};
use synthia_session::{
    store::Store,
    types::{Session, SessionState},
};
use tempfile::TempDir;

const TEST_USER: &str = "test-user";

fn make_store() -> (Store, TempDir) {
    let temp = TempDir::new().unwrap();
    let root = temp.path().to_path_buf();
    (Store::new(root), temp)
}

fn make_session_with_timestamp(
    id: &str,
    state: SessionState,
    created_hours_ago: i64,
    updated_hours_ago: i64,
) -> Session {
    let now = Utc::now();
    let mut session =
        Session::new_with_user(id.to_string(), TEST_USER.to_string())
            .expect("non-empty user_id");
    session.state = state;
    session.created_at = now - chrono::Duration::hours(created_hours_ago);
    session.updated_at = now - chrono::Duration::hours(updated_hours_ago);
    session
}

#[tokio::test]
async fn test_cleanup_daemon_deletes_expired_completed_sessions() {
    let (store, _temp) = make_store();

    // Create an old completed session (200 hours old, past 168h default retention).
    let old_session = make_session_with_timestamp(
        "old-completed",
        SessionState::Completed,
        200,
        200,
    );
    store.save_metadata(&old_session).unwrap();

    // Create a recent completed session (should survive).
    let recent_session = make_session_with_timestamp(
        "recent-completed",
        SessionState::Completed,
        48,
        2,
    );
    store.save_metadata(&recent_session).unwrap();

    // Create an old active session (should survive because not terminal state).
    let old_active = make_session_with_timestamp(
        "old-active",
        SessionState::WaitingForInput,
        200,
        200,
    );
    store.save_metadata(&old_active).unwrap();

    assert!(store.session_exists(TEST_USER, "old-completed"));
    assert!(store.session_exists(TEST_USER, "recent-completed"));
    assert!(store.session_exists(TEST_USER, "old-active"));

    let config = CleanupConfig::default();
    CleanupDaemon::cleanup_expired_sessions(&config, &store);

    assert!(!store.session_exists(TEST_USER, "old-completed"));
    assert!(store.session_exists(TEST_USER, "recent-completed"));
    assert!(store.session_exists(TEST_USER, "old-active"));
}

#[tokio::test]
async fn test_cleanup_daemon_deletes_cancelled_and_error_sessions() {
    let (store, _temp) = make_store();

    // Old cancelled session.
    let cancelled = make_session_with_timestamp(
        "old-cancelled",
        SessionState::Cancelled,
        200,
        200,
    );
    store.save_metadata(&cancelled).unwrap();

    // Old error session.
    let errored = make_session_with_timestamp(
        "old-errored",
        SessionState::Error,
        200,
        200,
    );
    store.save_metadata(&errored).unwrap();

    let config = CleanupConfig::default();
    let metrics = CleanupDaemon::cleanup_expired_sessions(&config, &store);

    assert_eq!(metrics.sessions_deleted, 2);
    assert!(!store.session_exists(TEST_USER, "old-cancelled"));
    assert!(!store.session_exists(TEST_USER, "old-errored"));
}

#[tokio::test]
async fn test_cleanup_daemon_removes_session_directory_and_metadata() {
    let (store, _temp) = make_store();

    let old_session = make_session_with_timestamp(
        "full-cleanup",
        SessionState::Completed,
        200,
        200,
    );
    store.save_metadata(&old_session).unwrap();
    store
        .append_message(
            TEST_USER,
            "full-cleanup",
            &serde_json::json!({ "msg": "test" }),
        )
        .unwrap();

    let session_dir = store.session_dir(TEST_USER, "full-cleanup");
    assert!(session_dir.exists());
    assert!(session_dir.join("metadata.json").exists());
    assert!(session_dir.join("messages.jsonl").exists());

    let config = CleanupConfig::default();
    CleanupDaemon::cleanup_expired_sessions(&config, &store);

    assert!(!session_dir.exists());
}

#[tokio::test]
async fn test_cleanup_daemon_temp_file_ttl() {
    let temp_dir = TempDir::new().unwrap();
    let workspace_root = temp_dir.path();

    // Create .agents/tmp/ directory.
    let tmp_dir = workspace_root.join(".agents").join("tmp");
    fs::create_dir_all(&tmp_dir).unwrap();

    // Create old temp file (48 hours old).
    let old_file = tmp_dir.join("cache-old.json");
    fs::write(&old_file, "old data").unwrap();
    let old_mtime = FileTime::from_unix_time(
        (Utc::now() - chrono::Duration::hours(48)).timestamp(),
        0,
    );
    filetime::set_file_mtime(&old_file, old_mtime).unwrap();

    // Create recent temp file (2 hours old).
    let new_file = tmp_dir.join("cache-new.json");
    fs::write(&new_file, "new data").unwrap();

    // Create nested old file.
    let subdir = tmp_dir.join("subdir");
    fs::create_dir_all(&subdir).unwrap();
    let nested_file = subdir.join("nested-old.txt");
    fs::write(&nested_file, "nested old").unwrap();
    filetime::set_file_mtime(&nested_file, old_mtime).unwrap();

    let config = CleanupConfig {
        check_interval: Duration::from_millis(200),
        temp_file_ttl_hours: 24,
        ..Default::default()
    };

    // We need a store to spawn the daemon. Create one from the same temp dir.
    let store = Store::new(workspace_root.join("sessions"));
    let handle = CleanupDaemon::spawn(&config, Arc::new(store), workspace_root);
    // Wait for two full cycles (first tick is skipped, then two intervals).
    tokio::time::sleep(Duration::from_millis(700)).await;
    handle.abort();

    assert!(!old_file.exists());
    assert!(!nested_file.exists());
    assert!(new_file.exists());
    // The subdir should be removed since it's now empty.
    assert!(!subdir.exists());
}

#[tokio::test]
async fn test_cleanup_daemon_temp_file_missing_directory() {
    let temp_dir = TempDir::new().unwrap();
    // No .agents/tmp/ directory exists.
    let workspace_root = temp_dir.path();

    // We can't directly test the private method, so we verify via spawn
    // that a cycle with no tmp dir doesn't crash.
    let config = CleanupConfig {
        check_interval: Duration::from_millis(300),
        ..Default::default()
    };

    let (store, _store_temp) = make_store();
    let handle = CleanupDaemon::spawn(&config, Arc::new(store), workspace_root);
    tokio::time::sleep(Duration::from_millis(600)).await;
    handle.abort();
    // If we get here without panic, the test passes.
}

#[tokio::test]
async fn test_cleanup_daemon_spawn_runs() {
    let (store, temp) = make_store();
    let workspace_root = temp.path().to_path_buf();

    // Create an expired session.
    let old_session = make_session_with_timestamp(
        "spawn-test",
        SessionState::Completed,
        200,
        200,
    );
    store.save_metadata(&old_session).unwrap();
    assert!(store.session_exists(TEST_USER, "spawn-test"));

    let config = CleanupConfig {
        check_interval: Duration::from_millis(500), // Fast interval for testing.
        session_retention_hours: 168,
        ..Default::default()
    };

    let handle =
        CleanupDaemon::spawn(&config, Arc::new(store.clone()), &workspace_root);

    // Wait for one cycle to complete.
    tokio::time::sleep(Duration::from_millis(1200)).await;

    // The session should have been deleted.
    assert!(!store.session_exists(TEST_USER, "spawn-test"));

    // Abort the daemon.
    handle.abort();
    let _ = handle.await;
}

#[tokio::test]
async fn test_cleanup_daemon_custom_retention_hours() {
    let (store, _temp) = make_store();

    // Create a session 4 hours old (should survive with 2h retention, die with 8h retention).
    let session = make_session_with_timestamp(
        "custom-retention",
        SessionState::Completed,
        4,
        4,
    );
    store.save_metadata(&session).unwrap();

    // With 2h retention, it should be deleted.
    let config_short = CleanupConfig {
        session_retention_hours: 2,
        ..Default::default()
    };
    CleanupDaemon::cleanup_expired_sessions(&config_short, &store);
    assert!(
        !store
            .list_session_ids(TEST_USER)
            .unwrap()
            .contains(&"custom-retention".to_string())
    );
}
