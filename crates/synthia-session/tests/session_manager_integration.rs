use synthia_session::{
    SessionState,
    TokenBudgetStatus,
    manager::SessionManager,
};
use tempfile::TempDir;

/// Test-scoped user identifier. The synthia-session store now refuses to
/// persist any session whose `user_id` is empty, so every integration
/// test must bind its sessions to a concrete namespace.
const TEST_USER: &str = "_legacy_";

fn make_manager() -> (SessionManager, TempDir) {
    let temp = TempDir::new().unwrap();
    let manager = SessionManager::new(temp.path().to_path_buf());
    (manager, temp)
}

/// Test helper: `create` + persist so subsequent store-level operations
/// (`session_exists`, `load_metadata`, `load_messages_older_than`) see the
/// session on disk.
async fn create_persisted(manager: &SessionManager, id: &str) {
    let session = manager
        .create_with_user(id.to_string(), TEST_USER.to_string())
        .await
        .unwrap();
    manager.save_metadata(&session).unwrap();
}

#[tokio::test]
async fn test_session_manager_delete_removes_from_memory_and_disk() {
    let (manager, _temp) = make_manager();

    create_persisted(&manager, "test-delete").await;
    manager
        .append_message(
            "test-delete",
            &serde_json::json!({"role": "user", "content": "hello"}),
        )
        .unwrap();

    assert!(manager.get("test-delete").await.is_some());
    assert!(manager.store().session_exists(TEST_USER, "test-delete"));

    manager.delete("test-delete").await.unwrap();

    assert!(manager.get("test-delete").await.is_none());
    assert!(!manager.store().session_exists(TEST_USER, "test-delete"));
}

#[tokio::test]
async fn test_session_manager_delete_nonexistent_session() {
    let (manager, _temp) = make_manager();

    // `delete` now resolves the owning user_id before tearing down
    // in-memory state, so deleting a session that was never created
    // returns `Err` rather than silently succeeding. This catches
    // typos / orphaned callers early.
    let result = manager.delete("nonexistent").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_session_manager_incremental_save_no_duplicates() {
    let (manager, _temp) = make_manager();
    create_persisted(&manager, "s1").await;

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

    let messages: Vec<serde_json::Value> =
        manager.load_messages_all("s1").unwrap();
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0]["content"], "msg1");
    assert_eq!(messages[1]["content"], "reply1");
}

#[tokio::test]
async fn test_session_manager_incremental_save_multiple_times() {
    let (manager, _temp) = make_manager();
    create_persisted(&manager, "s1").await;

    manager
        .append_message(
            "s1",
            &serde_json::json!({"role": "user", "content": "msg1"}),
        )
        .unwrap();

    manager.incremental_save("s1").await.unwrap();

    manager
        .append_message(
            "s1",
            &serde_json::json!({"role": "assistant", "content": "reply1"}),
        )
        .unwrap();

    manager.incremental_save("s1").await.unwrap();

    let messages: Vec<serde_json::Value> =
        manager.load_messages_all("s1").unwrap();
    assert_eq!(messages.len(), 2);
}

#[tokio::test]
async fn test_session_manager_lazy_loading_default_100() {
    let (manager, _temp) = make_manager();
    create_persisted(&manager, "s1").await;

    for i in 0..250 {
        manager
            .append_message(
                "s1",
                &serde_json::json!({"role": "user", "content": format!("msg-{}", i)}),
            )
            .unwrap();
    }

    let recent: Vec<serde_json::Value> =
        manager.load_messages_recent("s1", 100).unwrap();
    assert_eq!(recent.len(), 100);
    assert_eq!(recent[0]["content"], "msg-150");
    assert_eq!(recent[99]["content"], "msg-249");
}

#[tokio::test]
async fn test_session_manager_load_older_messages_on_demand() {
    let (manager, _temp) = make_manager();
    create_persisted(&manager, "s1").await;

    for i in 0..200 {
        manager
            .append_message(
                "s1",
                &serde_json::json!({"role": "user", "content": format!("msg-{}", i)}),
            )
            .unwrap();
    }

    let recent: Vec<serde_json::Value> =
        manager.load_messages_recent("s1", 100).unwrap();
    assert_eq!(recent.len(), 100);

    let older: Vec<serde_json::Value> = manager
        .store()
        .load_messages_older_than(TEST_USER, "s1", 100, 100)
        .unwrap();
    assert_eq!(older.len(), 100);
    assert_eq!(older[0]["content"], "msg-0");
    assert_eq!(older[99]["content"], "msg-99");
}

#[tokio::test]
async fn test_session_manager_cached_load_respects_limit() {
    let (manager, _temp) = make_manager();
    create_persisted(&manager, "s1").await;

    for i in 0..50 {
        manager
            .append_message(
                "s1",
                &serde_json::json!({"role": "user", "content": format!("msg-{}", i)}),
            )
            .unwrap();
    }

    let cached: Vec<serde_json::Value> =
        manager.load_messages_recent_cached("s1", 10).await.unwrap();
    assert_eq!(cached.len(), 10);
    assert_eq!(cached[0]["content"], "msg-40");
    assert_eq!(cached[9]["content"], "msg-49");
}

#[tokio::test]
async fn test_session_manager_delete_clears_all_caches() {
    let (manager, _temp) = make_manager();

    create_persisted(&manager, "cached-session").await;

    for i in 0..10 {
        manager
            .append_message(
                "cached-session",
                &serde_json::json!({"role": "user", "content": format!("msg-{}", i)}),
            )
            .unwrap();
    }

    manager
        .load_messages_recent_cached::<serde_json::Value>("cached-session", 10)
        .await
        .unwrap();

    manager.delete("cached-session").await.unwrap();

    assert!(manager.get("cached-session").await.is_none());
    assert!(!manager.store().session_exists(TEST_USER, "cached-session"));
}

#[tokio::test]
async fn test_session_manager_save_after_tool_call_updates_metadata() {
    let (manager, _temp) = make_manager();
    create_persisted(&manager, "s1").await;

    for i in 0..5 {
        manager
            .append_message(
                "s1",
                &serde_json::json!({"role": "user", "content": format!("msg-{}", i)}),
            )
            .unwrap();
    }

    manager.save_after_tool_call("s1").await.unwrap();

    let metadata = manager.store().load_metadata(TEST_USER, "s1").unwrap();
    assert_eq!(metadata.message_count, 5);
}

#[tokio::test]
async fn test_session_manager_save_on_shutdown_persists_state() {
    let (manager, _temp) = make_manager();
    create_persisted(&manager, "s1").await;

    manager
        .update_session_state("s1", SessionState::WaitingForInput)
        .await
        .unwrap();
    manager
        .update_session_state("s1", SessionState::LlmCalling)
        .await
        .unwrap();

    for i in 0..3 {
        manager
            .append_message(
                "s1",
                &serde_json::json!({"role": "user", "content": format!("msg-{}", i)}),
            )
            .unwrap();
    }

    manager.save_on_shutdown("s1").await.unwrap();

    let metadata = manager.store().load_metadata(TEST_USER, "s1").unwrap();
    assert_eq!(metadata.message_count, 3);
    assert_eq!(metadata.state, SessionState::LlmCalling);
}

#[tokio::test]
async fn test_session_manager_save_on_pause_persists_state() {
    let (manager, _temp) = make_manager();
    create_persisted(&manager, "s1").await;

    manager
        .update_session_state("s1", SessionState::WaitingForInput)
        .await
        .unwrap();

    manager
        .append_message(
            "s1",
            &serde_json::json!({"role": "user", "content": "before pause"}),
        )
        .unwrap();

    manager
        .update_session_state("s1", SessionState::Paused)
        .await
        .unwrap();

    manager.save_on_pause("s1").await.unwrap();

    let metadata = manager.store().load_metadata(TEST_USER, "s1").unwrap();
    assert_eq!(metadata.message_count, 1);
    assert_eq!(metadata.state, SessionState::Paused);
}

#[test]
fn test_token_budget_monitor_integration() {
    let monitor = synthia_session::TokenBudgetMonitor::new();

    let usage_70 = synthia_session::TokenUsage {
        prompt_tokens: 70_000,
        completion_tokens: 0,
        total_tokens: 70_000,
        cached_prompt_tokens: None,
        cache_read_tokens: None,
        cache_write_tokens: None,
    };
    monitor.check_and_log("test", &usage_70, &TokenBudgetStatus::Notice);
    assert_eq!(monitor.get_last_status(), Some(TokenBudgetStatus::Notice));

    let usage_85 = synthia_session::TokenUsage {
        prompt_tokens: 85_000,
        completion_tokens: 0,
        total_tokens: 85_000,
        cached_prompt_tokens: None,
        cache_read_tokens: None,
        cache_write_tokens: None,
    };
    monitor.check_and_log("test", &usage_85, &TokenBudgetStatus::Warning);
    assert_eq!(monitor.get_last_status(), Some(TokenBudgetStatus::Warning));

    let usage_90 = synthia_session::TokenUsage {
        prompt_tokens: 90_000,
        completion_tokens: 0,
        total_tokens: 90_000,
        cached_prompt_tokens: None,
        cache_read_tokens: None,
        cache_write_tokens: None,
    };
    monitor.check_and_log("test", &usage_90, &TokenBudgetStatus::MustCompact);
    assert!(monitor.should_trigger_compaction(&TokenBudgetStatus::MustCompact));
}
