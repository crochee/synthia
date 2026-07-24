//! In-memory index of active sessions plus their per-session
//! `SessionStateMachine` and supporting bookkeeping (LRU message
//! cache, approval-timeout timers, incremental-save offsets).
//!
//! The single 1188-line monolith has been split along concern
//! boundaries so each method group lives next to its own locks
//! and helpers:
//!
//! - [`types`]: `SessionFilter`, `SessionInfo`, the internal
//!   `CachedMessages` entry, and `MAX_CACHED_SESSIONS`.
//! - [`core`]: CRUD: create / get / remove / list / delete, user
//!   binding (`create_with_user`, `assign_user`, `restore`), and
//!   the `user_id_for` helper.
//! - [`cache`]: the LRU message cache
//!   (`load_messages_recent_cached`, `load_messages_all_cached`,
//!   `invalidate_cache`).
//! - [`approval`]: approval-timeout timer (`start_approval_timer`,
//!   `cancel_approval_timer`, `check_approval_timeout`).
//! - [`persistence`]: `save_metadata` / `append_message` /
//!   `load_messages_*` passthroughs, the
//!   `save_after_tool_call` / `save_on_shutdown` / `save_on_pause`
//!   triggers, and `load_messages_paginated`.
//! - [`queries`]: privileged `list_persisted_sessions`,
//!   `list_sessions_for_user`, the legacy
//!   `list_sessions_with_metadata` alias, and the `store` accessor.
//! - [`state`]: `update_session_state` orchestration plus the
//!   `Registry<SessionInfo>` trait impl.

mod approval;
mod cache;
mod core;
mod persistence;
mod queries;
mod state;
mod types;

pub use core::SessionManager;

pub use types::{SessionFilter, SessionInfo, SessionSummary};

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::{
        session::SessionError,
        types::{Session, SessionState},
    };

    const TEST_USER: &str = "alice";

    fn make_manager() -> (SessionManager, tempfile::TempDir) {
        let temp = tempfile::TempDir::new().unwrap();
        let manager = SessionManager::new(temp.path().to_path_buf());
        (manager, temp)
    }

    async fn make_session(manager: &SessionManager, id: &str) -> Session {
        manager
            .create_with_user(id.to_string(), TEST_USER.to_string())
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn test_create_and_get_session() {
        let (manager, _temp) = make_manager();
        make_session(&manager, "s1").await;
        let session = manager.get("s1").await;
        assert!(session.is_some());
        assert_eq!(session.unwrap().id, "s1");
    }

    #[tokio::test]
    async fn test_remove_session() {
        let (manager, _temp) = make_manager();
        make_session(&manager, "s1").await;
        let removed = manager.remove("s1").await;
        assert!(removed.is_some());
        assert!(manager.get("s1").await.is_none());
    }

    #[tokio::test]
    async fn test_list_sessions() {
        let (manager, _temp) = make_manager();
        make_session(&manager, "s1").await;
        make_session(&manager, "s2").await;
        let list = manager.list().await;
        assert_eq!(list.len(), 2);
    }

    #[tokio::test]
    async fn test_save_and_load_messages() {
        let (manager, _temp) = make_manager();
        make_session(&manager, "s1").await;

        manager
            .append_message(
                "s1",
                &serde_json::json!({"role": "user", "content": "Hello"}),
            )
            .unwrap();

        let messages: Vec<serde_json::Value> =
            manager.load_messages_recent("s1", 10).unwrap();
        assert_eq!(messages.len(), 1);
    }

    #[tokio::test]
    async fn test_delete_session() {
        let (manager, _temp) = make_manager();
        let session = make_session(&manager, "s1").await;
        manager.save_metadata(&session).unwrap();

        manager.delete_session("s1").unwrap();
        let metas = manager.list_sessions_for_user(TEST_USER).unwrap();
        assert!(!metas.iter().any(|m| m.id == "s1"));
    }

    #[tokio::test]
    async fn test_incremental_save_tracks_offset() {
        let (manager, _temp) = make_manager();
        make_session(&manager, "s1").await;
        manager
            .append_message(
                "s1",
                &serde_json::json!({"role": "user", "content": "msg1"}),
            )
            .unwrap();
        manager
            .append_message(
                "s1",
                &serde_json::json!({"role": "assistant", "content": "reply1"}),
            )
            .unwrap();

        manager.incremental_save("s1").await.unwrap();
        let offset =
            manager.last_saved_offsets.read().expect("RwLock poisoned");
        assert_eq!(*offset.get("s1").unwrap(), 2);
    }

    #[tokio::test]
    async fn test_load_messages_paginated() {
        let (manager, _temp) = make_manager();
        make_session(&manager, "s1").await;
        for i in 0..10 {
            manager
                .append_message("s1", &serde_json::json!({"msg": i}))
                .unwrap();
        }

        let page0: Vec<serde_json::Value> =
            manager.load_messages_paginated("s1", 0, 3).await.unwrap();
        assert_eq!(page0.len(), 3);
        assert_eq!(page0[0]["msg"], 0);

        let page1: Vec<serde_json::Value> =
            manager.load_messages_paginated("s1", 1, 3).await.unwrap();
        assert_eq!(page1.len(), 3);
        assert_eq!(page1[0]["msg"], 3);

        let page3: Vec<serde_json::Value> =
            manager.load_messages_paginated("s1", 3, 3).await.unwrap();
        assert_eq!(page3.len(), 1);
        assert_eq!(page3[0]["msg"], 9);

        let page4: Vec<serde_json::Value> =
            manager.load_messages_paginated("s1", 4, 3).await.unwrap();
        assert!(page4.is_empty());
    }

    #[tokio::test]
    async fn test_approval_timer_timeout() {
        let (mut manager, _temp) = make_manager();
        manager.approval_timeout = Duration::from_millis(50);
        make_session(&manager, "s1").await;

        manager.start_approval_timer("s1").await;
        assert!(!manager.check_approval_timeout("s1").await);

        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(manager.check_approval_timeout("s1").await);

        manager.cancel_approval_timer("s1").await;
        assert!(!manager.check_approval_timeout("s1").await);
    }

    #[tokio::test]
    async fn test_update_session_state_transitions() {
        let (manager, _temp) = make_manager();
        make_session(&manager, "s1").await;

        manager
            .update_session_state("s1", SessionState::WaitingForInput)
            .await
            .unwrap();

        manager
            .update_session_state("s1", SessionState::LlmCalling)
            .await
            .unwrap();

        let session = manager.get("s1").await.unwrap();
        assert_eq!(session.state, SessionState::LlmCalling);
    }

    #[tokio::test]
    async fn test_save_after_tool_call() {
        let (manager, _temp) = make_manager();
        make_session(&manager, "s1").await;

        manager
            .append_message(
                "s1",
                &serde_json::json!({"role": "user", "content": "tool request"}),
            )
            .unwrap();
        manager
            .append_message(
                "s1",
                &serde_json::json!({"role": "tool", "content": "tool result"}),
            )
            .unwrap();

        manager.save_after_tool_call("s1").await.unwrap();

        let metadata = manager.store().load_metadata(TEST_USER, "s1").unwrap();
        assert_eq!(metadata.message_count, 2);
    }

    #[tokio::test]
    async fn test_save_on_shutdown() {
        let (manager, _temp) = make_manager();
        make_session(&manager, "s1").await;

        for i in 0..5 {
            manager
                .append_message("s1", &serde_json::json!({"msg": i}))
                .unwrap();
        }

        manager.save_on_shutdown("s1").await.unwrap();

        let metadata = manager.store().load_metadata(TEST_USER, "s1").unwrap();
        assert_eq!(metadata.message_count, 5);
    }

    #[tokio::test]
    async fn test_save_on_pause() {
        let (manager, _temp) = make_manager();
        make_session(&manager, "s1").await;

        manager
            .append_message("s1", &serde_json::json!({"msg": "before pause"}))
            .unwrap();

        manager.save_on_pause("s1").await.unwrap();

        let metadata = manager.store().load_metadata(TEST_USER, "s1").unwrap();
        assert_eq!(metadata.message_count, 1);
    }

    #[tokio::test]
    async fn test_message_cache_basic() {
        let (manager, _temp) = make_manager();
        make_session(&manager, "cache-test").await;

        for i in 0..5 {
            manager
                .append_message("cache-test", &serde_json::json!({"index": i}))
                .unwrap();
        }

        let first: Vec<serde_json::Value> = manager
            .load_messages_recent_cached::<serde_json::Value>("cache-test", 10)
            .await
            .unwrap();
        assert_eq!(first.len(), 5);

        let second: Vec<serde_json::Value> = manager
            .load_messages_recent_cached::<serde_json::Value>("cache-test", 10)
            .await
            .unwrap();
        assert_eq!(second.len(), 5);
    }

    #[tokio::test]
    async fn test_message_cache_lru_eviction() {
        let (manager, _temp) = make_manager();

        for i in 0..15 {
            let id = format!("session-{}", i);
            make_session(&manager, &id).await;
            manager
                .append_message(&id, &serde_json::json!({"index": i}))
                .unwrap();
        }

        for i in 0..15 {
            let id = format!("session-{}", i);
            manager
                .load_messages_recent_cached::<serde_json::Value>(&id, 10)
                .await
                .unwrap();
        }

        make_session(&manager, "session-0").await;
        manager
            .append_message("session-0", &serde_json::json!({"updated": true}))
            .unwrap();

        manager.invalidate_cache("session-0").await;
    }

    #[tokio::test]
    async fn test_create_with_user_assigns_user_id() {
        let (manager, _temp) = make_manager();
        let session = make_session(&manager, "s1").await;
        assert_eq!(session.user_id, TEST_USER);
    }

    #[tokio::test]
    async fn test_assign_user_promotes_legacy_session() {
        let (manager, _temp) = make_manager();
        // Legacy path: session created without a user.
        manager.create("legacy-s".to_string()).await;
        // Append must fail until a user is bound.
        let err = manager
            .append_message(
                "legacy-s",
                &serde_json::json!({"role": "user", "content": "Hi"}),
            )
            .unwrap_err();
        assert!(format!("{:#}", err).contains("Empty user_id"));

        // Promote and retry.
        manager
            .assign_user("legacy-s", TEST_USER.to_string())
            .await
            .unwrap();
        manager
            .append_message(
                "legacy-s",
                &serde_json::json!({"role": "user", "content": "Hi"}),
            )
            .unwrap();
        let s = manager.get("legacy-s").await.unwrap();
        assert_eq!(s.user_id, TEST_USER);
    }

    #[tokio::test]
    async fn test_assign_user_rejects_double_binding() {
        let (manager, _temp) = make_manager();
        make_session(&manager, "s1").await;
        let err = manager
            .assign_user("s1", "bob".to_string())
            .await
            .unwrap_err();
        let msg = format!("{:#}", err);
        assert!(msg.contains("already bound"), "got: {msg}");
    }

    #[tokio::test]
    async fn test_list_sessions_for_user_isolates_users() {
        let (manager, _temp) = make_manager();
        // `list_sessions_for_user` reads from the on-disk store, so the
        // sessions must be persisted via `save_metadata` after creation.
        let alice = make_session(&manager, "a1").await;
        manager.save_metadata(&alice).unwrap();
        let bob = manager
            .create_with_user("b1".to_string(), "bob".to_string())
            .await
            .unwrap();
        manager.save_metadata(&bob).unwrap();

        let alice_list = manager.list_sessions_for_user(TEST_USER).unwrap();
        let bob_list = manager.list_sessions_for_user("bob").unwrap();

        assert_eq!(alice_list.len(), 1);
        assert_eq!(alice_list[0].id, "a1");
        assert_eq!(bob_list.len(), 1);
        assert_eq!(bob_list[0].id, "b1");
    }

    #[tokio::test]
    async fn test_list_for_user_isolates_users() {
        let (manager, _temp) = make_manager();
        make_session(&manager, "a1").await;
        make_session(&manager, "a2").await;
        manager
            .create_with_user("b1".to_string(), "bob".to_string())
            .await
            .unwrap();

        let alice_list = manager.list_for_user(TEST_USER).await;
        let bob_list = manager.list_for_user("bob").await;

        assert_eq!(alice_list.len(), 2);
        assert!(alice_list.iter().all(|s| s.id == s.title));
        assert!(alice_list.iter().any(|s| s.id == "a1"));
        assert!(alice_list.iter().any(|s| s.id == "a2"));
        assert_eq!(bob_list.len(), 1);
        assert_eq!(bob_list[0].id, "b1");
    }

    #[tokio::test]
    async fn test_get_for_user_returns_session_for_owner_and_not_found_for_others()
     {
        let (manager, _temp) = make_manager();
        make_session(&manager, "a1").await;

        let owner = manager.get_for_user(TEST_USER, "a1").await;
        assert!(owner.is_ok());
        assert_eq!(owner.unwrap().id, "a1");

        let non_owner = manager.get_for_user("bob", "a1").await;
        assert!(matches!(non_owner, Err(SessionError::NotFound)));

        let missing = manager.get_for_user(TEST_USER, "missing").await;
        assert!(matches!(missing, Err(SessionError::NotFound)));
    }

    #[tokio::test]
    async fn test_delete_for_user_rejects_cross_user_deletion() {
        let (manager, _temp) = make_manager();
        let session = make_session(&manager, "a1").await;
        manager.save_metadata(&session).unwrap();

        let cross_user = manager.delete_for_user("bob", "a1").await;
        assert!(matches!(cross_user, Err(SessionError::NotFound)));

        // Session is still present and owned by alice.
        assert!(manager.get_for_user(TEST_USER, "a1").await.is_ok());

        manager.delete_for_user(TEST_USER, "a1").await.unwrap();
        assert!(manager.get_for_user(TEST_USER, "a1").await.is_err());
    }

    #[tokio::test]
    async fn test_create_child_sets_parent_id_and_persists() {
        let (manager, _temp) = make_manager();
        let parent = make_session(&manager, "parent").await;
        manager.save_metadata(&parent).unwrap();

        let child = manager
            .create_child(
                TEST_USER.to_string(),
                "parent".to_string(),
                Some("child-1".to_string()),
            )
            .await
            .unwrap();

        assert_eq!(child.user_id, TEST_USER);
        assert_eq!(child.parent_id, Some("parent".to_string()));

        // Must be discoverable on disk.
        let from_disk =
            manager.store().load_metadata(TEST_USER, "child-1").unwrap();
        assert_eq!(from_disk.parent_id, Some("parent".to_string()));
    }

    #[tokio::test]
    async fn test_create_child_generates_id_when_none_given() {
        let (manager, _temp) = make_manager();
        let parent = make_session(&manager, "parent").await;
        manager.save_metadata(&parent).unwrap();

        let child = manager
            .create_child(TEST_USER.to_string(), "parent".to_string(), None)
            .await
            .unwrap();

        assert!(!child.id.is_empty());
        assert_eq!(child.parent_id, Some("parent".to_string()));
    }

    #[tokio::test]
    async fn test_create_child_rejects_missing_parent() {
        let (manager, _temp) = make_manager();
        let err = manager
            .create_child(
                TEST_USER.to_string(),
                "missing-parent".to_string(),
                Some("child-1".to_string()),
            )
            .await
            .unwrap_err();
        let msg = format!("{:#}", err);
        assert!(msg.contains("not found"), "got: {msg}");
    }

    #[tokio::test]
    async fn test_create_child_rejects_cross_user_parent() {
        let (manager, _temp) = make_manager();
        let bob_parent = manager
            .create_with_user("bob-parent".to_string(), "bob".to_string())
            .await
            .unwrap();
        manager.save_metadata(&bob_parent).unwrap();

        // Alice cannot create a child under bob's parent.
        let err = manager
            .create_child(
                TEST_USER.to_string(),
                "bob-parent".to_string(),
                Some("child-1".to_string()),
            )
            .await
            .unwrap_err();
        let msg = format!("{:#}", err);
        assert!(msg.contains("not found"), "got: {msg}");
    }

    #[tokio::test]
    async fn test_list_children_isolates_users() {
        let (manager, _temp) = make_manager();
        let parent = make_session(&manager, "parent").await;
        manager.save_metadata(&parent).unwrap();

        manager
            .create_child(
                TEST_USER.to_string(),
                "parent".to_string(),
                Some("child-a".to_string()),
            )
            .await
            .unwrap();
        manager
            .create_child(
                TEST_USER.to_string(),
                "parent".to_string(),
                Some("child-b".to_string()),
            )
            .await
            .unwrap();

        // Different parent for the same user.
        let other_parent = make_session(&manager, "other-parent").await;
        manager.save_metadata(&other_parent).unwrap();
        manager
            .create_child(
                TEST_USER.to_string(),
                "other-parent".to_string(),
                Some("child-other".to_string()),
            )
            .await
            .unwrap();

        // Bob's own parent and child.
        let bob_parent = manager
            .create_with_user("bob-parent".to_string(), "bob".to_string())
            .await
            .unwrap();
        manager.save_metadata(&bob_parent).unwrap();
        manager
            .create_child(
                "bob".to_string(),
                "bob-parent".to_string(),
                Some("bob-child".to_string()),
            )
            .await
            .unwrap();

        let children = manager.list_children(TEST_USER, "parent").unwrap();
        assert_eq!(children.len(), 2);
        assert!(children.iter().any(|c| c.id == "child-a"));
        assert!(children.iter().any(|c| c.id == "child-b"));
        assert!(
            children
                .iter()
                .all(|c| c.parent_id == Some("parent".to_string()))
        );

        let other_children =
            manager.list_children(TEST_USER, "other-parent").unwrap();
        assert_eq!(other_children.len(), 1);
        assert_eq!(other_children[0].id, "child-other");

        // Alice must not see bob's child even if she guesses the parent id.
        let alice_sees_bob =
            manager.list_children(TEST_USER, "bob-parent").unwrap();
        assert!(alice_sees_bob.is_empty());

        let bob_children = manager.list_children("bob", "bob-parent").unwrap();
        assert_eq!(bob_children.len(), 1);
        assert_eq!(bob_children[0].id, "bob-child");
    }

    #[tokio::test]
    async fn test_list_children_rejects_empty_user_id() {
        let (manager, _temp) = make_manager();
        let err = manager.list_children("", "parent").unwrap_err();
        let msg = format!("{:#}", err);
        assert!(msg.contains("Empty user_id"), "got: {msg}");
    }
}
