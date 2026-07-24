//! Unit tests for the `store` module.
//!
//! Coverage map (30 tests):
//!
//! - Metadata CRUD: 1 test
//!   ([`test_save_and_load_metadata`]).
//! - Messages: 8 tests
//!   ([`test_append_and_load_messages_recent`],
//!   [`test_load_messages_all`],
//!   [`test_load_recent_empty_session`],
//!   [`test_load_all_empty_session`],
//!   [`test_load_recent_with_limit`],
//!   [`test_load_messages_older_than`],
//!   [`test_load_messages_older_than_fewer_than_skip`],
//!   [`test_load_messages_older_than_nonexistent_session`]).
//! - Deletion: 2 tests
//!   ([`test_delete_session`],
//!   [`test_delete_nonexistent_session`]).
//! - Per-user session listing: 3 tests
//!   ([`test_list_session_ids`],
//!   [`test_list_session_ids_empty_user_dir`],
//!   [`test_list_session_ids_isolates_users`]).
//! - Path layout: 1 test
//!   ([`test_session_dir_path`]).
//! - Cross-user access control: 5 tests
//!   ([`test_save_metadata_rejects_empty_user_id`],
//!   [`test_list_sessions_with_metadata_empty_user_id_rejected`],
//!   [`test_load_metadata_rejects_legacy_owner_user_id`],
//!   [`test_load_metadata_cross_user_directory_rejected`],
//!   [`test_user_dir_segregation`],
//!   [`test_legacy_single_tenant_promoted_via_assign_user`]).
//! - `list_sessions_with_metadata`: 1 test
//!   ([`test_list_sessions_with_metadata_filters_to_caller`]).
//! - Unix permissions: 1 test
//!   ([`test_session_directory_has_0o700_permissions`]).
//! - `list_user_ids`: 7 tests
//!   ([`list_user_ids_returns_empty_when_root_does_not_exist`],
//!   [`list_user_ids_returns_empty_when_no_user_dirs`],
//!   [`list_user_ids_returns_single_user`],
//!   [`list_user_ids_returns_multiple_users`],
//!   [`list_user_ids_includes_server_default_namespace`],
//!   [`list_user_ids_filters_out_files_and_special_names`],
//!   [`list_user_ids_skips_empty_subdir`]).

use serde_json;
use tempfile::TempDir;

use super::*;
use crate::types::{Session, SessionState};

const TEST_USER: &str = "alice";

fn make_store() -> (Store, TempDir) {
    let temp = TempDir::new().unwrap();
    let root = temp.path().to_path_buf();
    (Store::new(root), temp)
}

fn make_session(id: &str) -> Session {
    Session::new_with_user(id.to_string(), TEST_USER.to_string()).unwrap()
}

#[test]
fn test_save_and_load_metadata() {
    let (store, _temp) = make_store();
    let session = make_session("test-session");

    store.save_metadata(&session).unwrap();
    let metadata = store.load_metadata(TEST_USER, "test-session").unwrap();

    assert_eq!(metadata.id, "test-session");
    assert_eq!(metadata.owner_user_id, TEST_USER);
    assert_eq!(metadata.state, SessionState::Initializing);
}

#[test]
fn test_append_and_load_messages_recent() {
    let (store, _temp) = make_store();

    // Use serde_json::Value for simple test messages
    store
        .append_message(
            TEST_USER,
            "s1",
            &serde_json::json!({"role": "user", "content": "Hello"}),
        )
        .unwrap();
    store
        .append_message(
            TEST_USER,
            "s1",
            &serde_json::json!({"role": "assistant", "content": "Hi"}),
        )
        .unwrap();
    store
        .append_message(
            TEST_USER,
            "s1",
            &serde_json::json!({"role": "user", "content": "How are you"}),
        )
        .unwrap();

    let messages: Vec<serde_json::Value> =
        store.load_messages_recent(TEST_USER, "s1", 2).unwrap();
    assert_eq!(messages.len(), 2);
    // Last 2 messages should be assistant and the last user
    assert_eq!(messages[1]["content"], "How are you");
}

#[test]
fn test_load_messages_all() {
    let (store, _temp) = make_store();

    store
        .append_message(TEST_USER, "s2", &serde_json::json!({"msg": 1}))
        .unwrap();
    store
        .append_message(TEST_USER, "s2", &serde_json::json!({"msg": 2}))
        .unwrap();
    store
        .append_message(TEST_USER, "s2", &serde_json::json!({"msg": 3}))
        .unwrap();

    let messages: Vec<serde_json::Value> =
        store.load_messages_all(TEST_USER, "s2").unwrap();
    assert_eq!(messages.len(), 3);
    assert_eq!(messages[0]["msg"], 1);
    assert_eq!(messages[2]["msg"], 3);
}

#[test]
fn test_load_recent_empty_session() {
    let (store, _temp) = make_store();
    let messages: Vec<serde_json::Value> = store
        .load_messages_recent(TEST_USER, "nonexistent", 10)
        .unwrap();
    assert!(messages.is_empty());
}

#[test]
fn test_load_all_empty_session() {
    let (store, _temp) = make_store();
    let messages: Vec<serde_json::Value> =
        store.load_messages_all(TEST_USER, "nonexistent").unwrap();
    assert!(messages.is_empty());
}

#[test]
fn test_delete_session() {
    let (store, _temp) = make_store();
    let session = make_session("to-delete");
    store.save_metadata(&session).unwrap();
    store
        .append_message(
            TEST_USER,
            "to-delete",
            &serde_json::json!({"msg": "test"}),
        )
        .unwrap();

    assert!(store.session_exists(TEST_USER, "to-delete"));
    store.delete_session(TEST_USER, "to-delete").unwrap();
    assert!(!store.session_exists(TEST_USER, "to-delete"));
}

#[test]
fn test_delete_nonexistent_session() {
    let (store, _temp) = make_store();
    // Should not error if directory doesn't exist
    store.delete_session(TEST_USER, "nonexistent").unwrap();
}

#[test]
fn test_list_session_ids() {
    let (store, _temp) = make_store();
    let session = make_session("list-test-1");
    store.save_metadata(&session).unwrap();

    let session2 = make_session("list-test-2");
    store.save_metadata(&session2).unwrap();

    let ids = store.list_session_ids(TEST_USER).unwrap();
    assert_eq!(ids.len(), 2);
    assert!(ids.contains(&"list-test-1".to_string()));
    assert!(ids.contains(&"list-test-2".to_string()));
}

#[test]
fn test_list_session_ids_empty_user_dir() {
    let (store, _temp) = make_store();
    let ids = store.list_session_ids(TEST_USER).unwrap();
    assert!(ids.is_empty());
}

#[test]
fn test_list_session_ids_isolates_users() {
    let (store, _temp) = make_store();
    let alice_session = make_session("alice-s");
    store.save_metadata(&alice_session).unwrap();

    let bob_session =
        Session::new_with_user("bob-s".to_string(), "bob".to_string()).unwrap();
    store.save_metadata(&bob_session).unwrap();

    let alice_ids = store.list_session_ids("alice").unwrap();
    let bob_ids = store.list_session_ids("bob").unwrap();

    assert_eq!(alice_ids, vec!["alice-s".to_string()]);
    assert_eq!(bob_ids, vec!["bob-s".to_string()]);
}

#[test]
fn test_session_dir_path() {
    let (store, _temp) = make_store();
    let dir = store.session_dir(TEST_USER, "my-session");
    let s = dir.to_string_lossy();
    assert!(s.contains("alice"), "path should include user_id: {s}");
    assert!(
        s.contains("my-session"),
        "path should include session_id: {s}"
    );
}

#[test]
fn test_load_recent_with_limit() {
    let (store, _temp) = make_store();

    // Add more than limit messages
    for i in 0..150 {
        store
            .append_message(TEST_USER, "s3", &serde_json::json!({"index": i}))
            .unwrap();
    }

    // Should return last 100
    let messages: Vec<serde_json::Value> =
        store.load_messages_recent(TEST_USER, "s3", 100).unwrap();
    assert_eq!(messages.len(), 100);
    assert_eq!(messages[0]["index"], 50);
    assert_eq!(messages[99]["index"], 149);
}

#[test]
fn test_load_messages_older_than() {
    let (store, _temp) = make_store();

    for i in 0..200 {
        store
            .append_message(TEST_USER, "s4", &serde_json::json!({"index": i}))
            .unwrap();
    }

    let recent: Vec<serde_json::Value> =
        store.load_messages_recent(TEST_USER, "s4", 100).unwrap();
    assert_eq!(recent.len(), 100);
    assert_eq!(recent[0]["index"], 100);

    let older: Vec<serde_json::Value> = store
        .load_messages_older_than(TEST_USER, "s4", 100, 100)
        .unwrap();
    assert_eq!(older.len(), 100);
    assert_eq!(older[0]["index"], 0);
    assert_eq!(older[99]["index"], 99);
}

#[test]
fn test_load_messages_older_than_fewer_than_skip() {
    let (store, _temp) = make_store();

    for i in 0..30 {
        store
            .append_message(TEST_USER, "s5", &serde_json::json!({"index": i}))
            .unwrap();
    }

    let recent: Vec<serde_json::Value> =
        store.load_messages_recent(TEST_USER, "s5", 100).unwrap();
    assert_eq!(recent.len(), 30);

    let older: Vec<serde_json::Value> = store
        .load_messages_older_than(TEST_USER, "s5", 30, 100)
        .unwrap();
    assert!(older.is_empty());
}

#[test]
fn test_load_messages_older_than_nonexistent_session() {
    let (store, _temp) = make_store();
    let older: Vec<serde_json::Value> = store
        .load_messages_older_than(TEST_USER, "nonexistent", 100, 100)
        .unwrap();
    assert!(older.is_empty());
}

#[test]
fn test_save_metadata_rejects_empty_user_id() {
    let (store, _temp) = make_store();
    let session = Session::new("legacy-session".to_string());
    // The store refuses to persist a session whose user_id is empty
    // even if the caller managed to build one through the legacy
    // constructor.
    let err = store.save_metadata(&session).unwrap_err();
    let msg = format!("{:#}", err);
    assert!(
        msg.contains("Empty user_id"),
        "expected EmptyUserId error, got: {msg}"
    );
}

#[test]
fn test_list_sessions_with_metadata_empty_user_id_rejected() {
    let (store, _temp) = make_store();
    let err = store.list_sessions_with_metadata("").unwrap_err();
    let msg = format!("{:#}", err);
    assert!(msg.contains("Empty user_id"), "got: {msg}");
}

#[test]
fn test_load_metadata_rejects_legacy_owner_user_id() {
    // Simulate a session that was written with owner_user_id ""
    // (legacy single-tenant). Loading it under any concrete user_id
    // must return CrossUserAccess rather than silently binding the
    // session to the caller's namespace.
    let (store, _temp) = make_store();
    // Place the legacy metadata file under the caller's namespace so
    // that load_metadata actually finds it on disk; the mismatched
    // `owner_user_id = ""` is what must trip the CrossUserAccess guard.
    let legacy = store.session_dir(TEST_USER, "legacy-s");
    fs::create_dir_all(&legacy).unwrap();
    let meta = serde_json::json!({
        "version": 1,
        "id": "legacy-s",
        "owner_user_id": "",
        "state": "Initializing",
        "token_usage": {
            "prompt_tokens": 0,
            "completion_tokens": 0,
            "total_tokens": 0,
            "cached_prompt_tokens": null,
        },
        "created_at": "2026-01-01T00:00:00Z",
        "updated_at": "2026-01-01T00:00:00Z",
        "config": {
            "model": "gpt-4o",
            "max_tokens": 4096,
        },
        "message_count": 0,
    });
    fs::write(legacy.join("metadata.json"), meta.to_string()).unwrap();

    let err = store.load_metadata(TEST_USER, "legacy-s").unwrap_err();
    let msg = format!("{:#}", err);
    assert!(
        msg.contains("CrossUserAccess"),
        "expected CrossUserAccess, got: {msg}"
    );
}

#[test]
fn test_load_metadata_cross_user_directory_rejected() {
    // Session was saved under alice/ but the on-disk metadata claims
    // bob owns it. Reading it as alice must fail.
    let (store, _temp) = make_store();
    let session = make_session("mixed-s");
    store.save_metadata(&session).unwrap();

    // Tamper with the metadata so owner_user_id no longer matches
    // the directory it's in.
    let path = store
        .session_dir(TEST_USER, "mixed-s")
        .join("metadata.json");
    let mut text = fs::read_to_string(&path).unwrap();
    text = text.replace(TEST_USER, "bob");
    fs::write(&path, text).unwrap();

    let err = store.load_metadata(TEST_USER, "mixed-s").unwrap_err();
    let msg = format!("{:#}", err);
    assert!(msg.contains("CrossUserAccess"), "got: {msg}");
}

#[test]
fn test_list_sessions_with_metadata_filters_to_caller() {
    let (store, _temp) = make_store();
    let alice = make_session("a1");
    store.save_metadata(&alice).unwrap();
    let alice2 = make_session("a2");
    store.save_metadata(&alice2).unwrap();
    let bob =
        Session::new_with_user("b1".to_string(), "bob".to_string()).unwrap();
    store.save_metadata(&bob).unwrap();

    let alice_metas = store.list_sessions_with_metadata(TEST_USER).unwrap();
    let bob_metas = store.list_sessions_with_metadata("bob").unwrap();

    assert_eq!(alice_metas.len(), 2);
    assert!(alice_metas.iter().all(|m| m.owner_user_id == TEST_USER));
    assert_eq!(bob_metas.len(), 1);
    assert_eq!(bob_metas[0].id, "b1");
}

#[test]
#[cfg(unix)]
fn test_session_directory_has_0o700_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let (store, _temp) = make_store();
    let session = make_session("perms-test");
    store.save_metadata(&session).unwrap();

    let dir = store.session_dir(TEST_USER, "perms-test");
    let mode = dir.metadata().unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o700, "expected 0o700, got {:o}", mode);
}

#[test]
fn test_user_dir_segregation() {
    // alice's session must NOT be visible to bob even if bob
    // guesses the session_id.
    let (store, _temp) = make_store();
    let session = make_session("secret");
    store.save_metadata(&session).unwrap();

    assert!(store.session_exists(TEST_USER, "secret"));
    assert!(!store.session_exists("bob", "secret"));
}

#[test]
fn test_legacy_single_tenant_promoted_via_assign_user() {
    // Backward-compat: a session created via the legacy
    // `Session::new` constructor (which leaves `user_id` empty) MUST
    // round-trip through `assign_user` and persist under the new
    // namespace, so existing v0 callers can be migrated without
    // data loss. The on-disk path must be `{root}/{new_user}/id/`,
    // not `{root}/id/`.
    let (store, _temp) = make_store();
    let mut session = Session::new("promote-me".to_string());
    assert!(
        session.user_id.is_empty(),
        "Session::new must leave user_id empty"
    );

    // Persistence refuses an empty user_id...
    assert!(store.save_metadata(&session).is_err());

    // ...but succeeds once the caller binds a real user.
    session.assign_user("alice".to_string());
    store.save_metadata(&session).unwrap();

    // The directory lives under the user's namespace, not at the
    // legacy root. Other users cannot see it.
    assert!(store.session_exists("alice", "promote-me"));
    assert!(!store.session_exists("bob", "promote-me"));
    let legacy_root = store.session_dir("alice", "promote-me");
    assert!(legacy_root.is_dir());

    // Reload and confirm owner_user_id was persisted.
    let metadata = store.load_metadata("alice", "promote-me").unwrap();
    assert_eq!(metadata.owner_user_id, "alice");
    assert_eq!(metadata.id, "promote-me");
}

// ===== list_user_ids =====

#[test]
fn list_user_ids_returns_empty_when_root_does_not_exist() {
    // Fresh install: sessions_root hasn't been created yet.
    let temp = tempfile::tempdir().unwrap();
    let store = Store::new(temp.path().join("does-not-exist"));
    let ids = store.list_user_ids().unwrap();
    assert!(ids.is_empty());
}

#[test]
fn list_user_ids_returns_empty_when_no_user_dirs() {
    let (store, _temp) = make_store();
    let ids = store.list_user_ids().unwrap();
    assert!(ids.is_empty());
}

#[test]
fn list_user_ids_returns_single_user() {
    let (store, _temp) = make_store();
    store.save_metadata(&make_session("s1")).unwrap();
    let mut ids = store.list_user_ids().unwrap();
    ids.sort();
    assert_eq!(ids, vec![TEST_USER.to_string()]);
}

#[test]
fn list_user_ids_returns_multiple_users() {
    let (store, _temp) = make_store();
    store.save_metadata(&make_session("alice-s1")).unwrap();
    // bob's session via the manual constructor
    let bob = Session::new_with_user("bob-s1".to_string(), "bob".to_string())
        .unwrap();
    store.save_metadata(&bob).unwrap();

    let mut ids = store.list_user_ids().unwrap();
    ids.sort();
    assert_eq!(ids, vec!["alice".to_string(), "bob".to_string()]);
}

#[test]
fn list_user_ids_includes_server_default_namespace() {
    // The legacy `_legacy_` (now `SERVER_DEFAULT_USER_ID`) directory
    // is a real user namespace and must be visible to the cleanup
    // daemon. Sweeping it is a feature, not a bug.
    let (store, _temp) = make_store();
    let legacy = Session::new_with_user(
        "legacy-s".to_string(),
        SERVER_DEFAULT_USER_ID.to_string(),
    )
    .unwrap();
    store.save_metadata(&legacy).unwrap();
    let mut ids = store.list_user_ids().unwrap();
    ids.sort();
    assert_eq!(ids, vec![SERVER_DEFAULT_USER_ID.to_string()]);
}

#[test]
fn list_user_ids_filters_out_files_and_special_names() {
    // A stray file at the root (e.g. a README.md) must not be
    // surfaced as a user_id. . and .. are also filtered even
    // though they cannot appear in fs::read_dir output — the
    // guard is defense-in-depth.
    let (store, temp) = make_store();
    std::fs::write(temp.path().join("README.md"), "hi").unwrap();
    store.save_metadata(&make_session("s1")).unwrap();

    let ids = store.list_user_ids().unwrap();
    assert_eq!(ids, vec![TEST_USER.to_string()]);
}

#[test]
fn list_user_ids_skips_empty_subdir() {
    // A user directory with no session subdirectories should
    // still appear in the result (it represents a registered
    // user who has not yet created a session). The function
    // walks one level deep; whether the user has sessions is
    // a separate question for `list_session_ids`.
    let (store, temp) = make_store();
    std::fs::create_dir(temp.path().join("empty-user")).unwrap();
    store.save_metadata(&make_session("s1")).unwrap();

    let mut ids = store.list_user_ids().unwrap();
    ids.sort();
    assert_eq!(ids, vec![TEST_USER.to_string(), "empty-user".to_string()]);
}

// ===== Backward compatibility =====

#[test]
fn test_load_metadata_without_parent_id_defaults_to_none() {
    // Metadata files written before `parent_id` was introduced must
    // deserialize with the field set to `None`.
    let (store, _temp) = make_store();
    let old_meta = serde_json::json!({
        "version": 1,
        "id": "no-parent",
        "owner_user_id": TEST_USER,
        "state": "Initializing",
        "token_usage": {
            "prompt_tokens": 0,
            "completion_tokens": 0,
            "total_tokens": 0,
            "cached_prompt_tokens": null,
        },
        "created_at": "2026-01-01T00:00:00Z",
        "updated_at": "2026-01-01T00:00:00Z",
        "config": {
            "model": "gpt-4o",
            "max_tokens": 4096,
        },
        "message_count": 0,
    });
    let session_dir = store.session_dir(TEST_USER, "no-parent");
    fs::create_dir_all(&session_dir).unwrap();
    fs::write(
        session_dir.join("metadata.json"),
        serde_json::to_string_pretty(&old_meta).unwrap(),
    )
    .unwrap();

    let metadata = store.load_metadata(TEST_USER, "no-parent").unwrap();
    assert_eq!(metadata.parent_id, None, "parent_id should default to None");
}

#[test]
fn test_load_metadata_with_old_format_no_new_fields() {
    // Verify that metadata.json files written before the
    // 2026-06-20 session persistence changes (which added
    // `end_reason`, `iteration`, `cumulative_tokens`,
    // `context_token_limit`) still deserialize correctly.
    let (store, _temp) = make_store();

    // Manually construct an old-format metadata.json without
    // the four new fields.
    let old_meta = serde_json::json!({
        "version": 1,
        "id": "old-format-session",
        "owner_user_id": TEST_USER,
        "state": "Initializing",
        "token_usage": {
            "prompt_tokens": 100,
            "completion_tokens": 50,
            "total_tokens": 150,
            "cached_prompt_tokens": null,
        },
        "created_at": "2026-01-01T00:00:00Z",
        "updated_at": "2026-01-01T00:00:00Z",
        "config": {
            "model": "gpt-4o",
            "max_tokens": 4096,
        },
        "message_count": 5,
    });

    // Write the old-format metadata to disk under the user
    // namespace.
    let session_dir = store.session_dir(TEST_USER, "old-format-session");
    fs::create_dir_all(&session_dir).unwrap();
    fs::write(
        session_dir.join("metadata.json"),
        serde_json::to_string_pretty(&old_meta).unwrap(),
    )
    .unwrap();

    // Load the metadata — must succeed and fill in defaults for
    // the new fields.
    let metadata = store
        .load_metadata(TEST_USER, "old-format-session")
        .unwrap();

    assert_eq!(metadata.id, "old-format-session");
    assert_eq!(metadata.owner_user_id, TEST_USER);
    assert_eq!(metadata.message_count, 5);
    assert_eq!(metadata.state, SessionState::Initializing);
    assert_eq!(metadata.token_usage.total_tokens, 150);

    // New fields must be set to their default values.
    assert_eq!(
        metadata.end_reason, None,
        "end_reason should default to None"
    );
    assert_eq!(metadata.iteration, 0, "iteration should default to 0");
    assert_eq!(
        metadata.cumulative_tokens, 0,
        "cumulative_tokens should default to 0"
    );
    assert_eq!(
        metadata.context_token_limit, None,
        "context_token_limit should default to None"
    );
}
