//! Integration tests for session persistence and recovery.
//!
//! These tests exercise the Store and Session types through their full
//! persistence lifecycle: save, load, incremental save, and recovery scenarios.
//!
//! Tests verify:
//! - Metadata save/load roundtrip
//! - Incremental message append and partial recovery
//! - Session directory isolation
//! - Message ordering after recovery
//! - Handling of corrupted data gracefully

use std::fs;

use serde::{Deserialize, Serialize};
use synthia_session::{
    SessionConfig,
    SessionState,
    Store,
    TokenBudget,
    types::Session,
};
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Test-scoped user identifier. The `Store` refuses to persist a session
/// whose `user_id` is empty, so every test must build its session under
/// this concrete namespace.
const TEST_USER: &str = "_legacy_";

/// A simple serializable message for testing persistence.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct TestMessage {
    role: String,
    content: String,
}

fn make_store() -> (Store, TempDir) {
    let temp = TempDir::new().unwrap();
    let root = temp.path().to_path_buf();
    (Store::new(root), temp)
}

fn make_session(id: &str) -> Session {
    // Build the session through the multi-tenant constructor so the
    // `Store` will accept persistence. `TEST_USER` is a real, non-empty
    // user_id — it just happens to be the placeholder for the legacy
    // single-tenant layout that older callers were using.
    Session::new_with_user(id.to_string(), TEST_USER.to_string())
        .expect("TEST_USER is non-empty")
}

fn make_session_with_config(
    id: &str,
    model: &str,
    max_tokens: usize,
) -> Session {
    let config = SessionConfig {
        model: model.to_string(),
        max_tokens,
    };
    let budget = TokenBudget::new(200_000);
    let mut session = Session::with_config(id.to_string(), config, budget);
    session.assign_user(TEST_USER.to_string());
    session
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Verify that saving and loading session metadata works end-to-end.
#[test]
fn test_session_persistence_save_and_load_metadata() {
    let (store, _temp) = make_store();
    let session = make_session("persist-1");

    store.save_metadata(&session).unwrap();

    let metadata = store.load_metadata(TEST_USER, "persist-1").unwrap();
    assert_eq!(metadata.id, "persist-1");
    assert_eq!(metadata.state, SessionState::Initializing);
    assert_eq!(metadata.version, 1);
    assert!(metadata.message_count == 0);
}

/// Verify that metadata reflects custom configuration.
#[test]
fn test_session_persistence_metadata_with_custom_config() {
    let (store, _temp) = make_store();
    let session = make_session_with_config("persist-2", "claude-3-opus", 8192);

    store.save_metadata(&session).unwrap();

    let metadata = store.load_metadata(TEST_USER, "persist-2").unwrap();
    assert_eq!(metadata.config.model, "claude-3-opus");
    assert_eq!(metadata.config.max_tokens, 8192);
}

/// Test incremental save: append multiple messages and verify they all persist.
#[test]
fn test_session_persistence_incremental_save() {
    let (store, _temp) = make_store();
    let session = make_session("incremental-1");
    store.save_metadata(&session).unwrap();

    // Simulate a conversation: user -> assistant -> user -> assistant
    let messages = vec![
        TestMessage {
            role: "user".to_string(),
            content: "What is Rust?".to_string(),
        },
        TestMessage {
            role: "assistant".to_string(),
            content: "A systems programming language.".to_string(),
        },
        TestMessage {
            role: "user".to_string(),
            content: "What about Go?".to_string(),
        },
        TestMessage {
            role: "assistant".to_string(),
            content: "Another systems programming language from Google."
                .to_string(),
        },
    ];

    for msg in &messages {
        store
            .append_message(TEST_USER, "incremental-1", msg)
            .unwrap();
    }

    let loaded: Vec<TestMessage> =
        store.load_messages_all(TEST_USER, "incremental-1").unwrap();
    assert_eq!(loaded.len(), 4);
    assert_eq!(loaded[0].content, "What is Rust?");
    assert_eq!(
        loaded[3].content,
        "Another systems programming language from Google."
    );
}

/// Test that partial recovery works: load only the most recent N messages.
#[test]
fn test_session_persistence_partial_recovery() {
    let (store, _temp) = make_store();
    let session = make_session("recovery-1");
    store.save_metadata(&session).unwrap();

    // Write 20 messages
    for i in 0..20 {
        store
            .append_message(
                TEST_USER,
                "recovery-1",
                &TestMessage {
                    role: "user".to_string(),
                    content: format!("message-{}", i),
                },
            )
            .unwrap();
    }

    // Recover only the last 5 messages
    let recent: Vec<TestMessage> = store
        .load_messages_recent(TEST_USER, "recovery-1", 5)
        .unwrap();
    assert_eq!(recent.len(), 5);
    assert_eq!(recent[0].content, "message-15");
    assert_eq!(recent[4].content, "message-19");
}

/// Test that session directories are properly isolated.
#[test]
fn test_session_persistence_directory_isolation() {
    let (store, _temp) = make_store();

    // Create two sessions
    let s1 = make_session("isolation-a");
    let s2 = make_session("isolation-b");
    store.save_metadata(&s1).unwrap();
    store.save_metadata(&s2).unwrap();

    // Append messages only to session a
    store
        .append_message(
            TEST_USER,
            "isolation-a",
            &TestMessage {
                role: "user".to_string(),
                content: "only in session a".to_string(),
            },
        )
        .unwrap();

    // Session b should have no messages
    let msgs_b: Vec<TestMessage> =
        store.load_messages_all(TEST_USER, "isolation-b").unwrap();
    assert!(msgs_b.is_empty());

    // Session a should have exactly 1 message
    let msgs_a: Vec<TestMessage> =
        store.load_messages_all(TEST_USER, "isolation-a").unwrap();
    assert_eq!(msgs_a.len(), 1);
    assert_eq!(msgs_a[0].content, "only in session a");
}

/// Test recovery after simulating a crash: metadata is saved, but messages
/// are partially written. Verify we can still recover what exists.
#[test]
fn test_session_persistence_recovery_after_partial_write() {
    let (store, _temp) = make_store();
    let mut session = make_session("partial-recovery");

    // Simulate: save metadata, write some messages, update metadata with
    // token usage, and "crash" (session is saved but some state is incomplete)
    store.save_metadata(&session).unwrap();

    // Write 3 messages before "crash"
    for i in 0..3 {
        store
            .append_message(
                TEST_USER,
                "partial-recovery",
                &TestMessage {
                    role: "assistant".to_string(),
                    content: format!("before-crash-{}", i),
                },
            )
            .unwrap();
    }

    // Simulate crash: update token usage and save metadata
    session.add_token_usage(500, 300, Some(100));
    session.state = SessionState::Error;
    store.save_metadata(&session).unwrap();

    // Recovery: load metadata and messages
    let metadata = store.load_metadata(TEST_USER, "partial-recovery").unwrap();
    assert_eq!(metadata.state, SessionState::Error);
    assert_eq!(metadata.token_usage.total_tokens, 800);

    let messages: Vec<TestMessage> = store
        .load_messages_all(TEST_USER, "partial-recovery")
        .unwrap();
    assert_eq!(messages.len(), 3);
    assert_eq!(messages[0].content, "before-crash-0");
    assert_eq!(messages[2].content, "before-crash-2");
}

/// Test that a session can be deleted and no longer appears in listings.
#[test]
fn test_session_persistence_delete_and_list() {
    let (store, _temp) = make_store();

    let s1 = make_session("delete-me");
    let s2 = make_session("keep-me");
    store.save_metadata(&s1).unwrap();
    store.save_metadata(&s2).unwrap();

    assert!(store.session_exists(TEST_USER, "delete-me"));
    assert!(store.session_exists(TEST_USER, "keep-me"));

    store.delete_session(TEST_USER, "delete-me").unwrap();

    assert!(!store.session_exists(TEST_USER, "delete-me"));
    assert!(store.session_exists(TEST_USER, "keep-me"));

    let ids = store.list_session_ids(TEST_USER).unwrap();
    assert_eq!(ids.len(), 1);
    assert_eq!(ids[0], "keep-me");
}

/// Test listing all sessions with their metadata.
#[test]
fn test_session_persistence_list_with_metadata() {
    let (store, _temp) = make_store();

    let mut s1 = make_session("list-meta-1");
    s1.add_token_usage(100, 50, None);
    store.save_metadata(&s1).unwrap();

    let mut s2 = make_session_with_config("list-meta-2", "gpt-4-turbo", 16384);
    s2.add_token_usage(200, 100, Some(50));
    store.save_metadata(&s2).unwrap();

    let metadata_list = store.list_sessions_with_metadata(TEST_USER).unwrap();
    assert_eq!(metadata_list.len(), 2);

    // Find each session by id
    let meta1 = metadata_list
        .iter()
        .find(|m| m.id == "list-meta-1")
        .unwrap();
    assert_eq!(meta1.token_usage.total_tokens, 150);

    let meta2 = metadata_list
        .iter()
        .find(|m| m.id == "list-meta-2")
        .unwrap();
    assert_eq!(meta2.config.model, "gpt-4-turbo");
    assert_eq!(meta2.token_usage.total_tokens, 300);
    assert_eq!(meta2.token_usage.cached_prompt_tokens, Some(50));
}

/// Test message count tracking in metadata.
#[test]
fn test_session_persistence_message_count_tracking() {
    let (store, _temp) = make_store();
    let session = make_session("count-tracking");
    store.save_metadata(&session).unwrap();

    // Write 7 messages
    for i in 0..7 {
        store
            .append_message(
                TEST_USER,
                "count-tracking",
                &TestMessage {
                    role: "user".to_string(),
                    content: format!("msg-{}", i),
                },
            )
            .unwrap();
    }

    // Reload metadata and check count
    let metadata = store.load_metadata(TEST_USER, "count-tracking").unwrap();
    // Note: count is only updated when save_metadata is called
    // The append operation doesn't update metadata automatically
    // But count_messages should still work correctly
    assert_eq!(metadata.message_count, 0); // Was 0 when we saved

    // Save metadata again to get updated count
    let mut session = make_session("count-tracking");
    session.add_token_usage(1, 1, None);
    store.save_metadata(&session).unwrap();

    let metadata2 = store.load_metadata(TEST_USER, "count-tracking").unwrap();
    assert_eq!(metadata2.message_count, 7);
}

/// Test that loading messages from a nonexistent session returns empty.
#[test]
fn test_session_persistence_load_nonexistent_session() {
    let (store, _temp) = make_store();

    let msgs_all: Vec<TestMessage> = store
        .load_messages_all(TEST_USER, "no-such-session")
        .unwrap();
    assert!(msgs_all.is_empty());

    let msgs_recent: Vec<TestMessage> = store
        .load_messages_recent(TEST_USER, "no-such-session", 10)
        .unwrap();
    assert!(msgs_recent.is_empty());
}

/// Test that the messages file can handle empty lines gracefully.
#[test]
fn test_session_persistence_empty_lines_handling() {
    let (store, _temp) = make_store();
    let session = make_session("empty-lines");
    store.save_metadata(&session).unwrap();

    // Manually append messages with empty lines interspersed
    let dir = store.session_dir(TEST_USER, "empty-lines");
    let path = dir.join("messages.jsonl");
    fs::write(
        &path,
        r#"{"role":"user","content":"hello"}
{"role":"assistant","content":"hi"}

{"role":"user","content":"bye"}
"#,
    )
    .unwrap();

    let messages: Vec<TestMessage> =
        store.load_messages_all(TEST_USER, "empty-lines").unwrap();
    assert_eq!(messages.len(), 3);
    assert_eq!(messages[0].content, "hello");
    assert_eq!(messages[2].content, "bye");
}

/// Test session state transitions through persistence.
#[test]
fn test_session_persistence_state_transition_history() {
    let (store, _temp) = make_store();
    let mut session = make_session("state-transitions");

    // Save initial state
    store.save_metadata(&session).unwrap();

    // Simulate state changes
    session.state = SessionState::WaitingForInput;
    store.save_metadata(&session).unwrap();

    let meta = store.load_metadata(TEST_USER, "state-transitions").unwrap();
    assert_eq!(meta.state, SessionState::WaitingForInput);

    // Update to completed
    let mut session2 = make_session("state-transitions");
    session2.state = SessionState::Completed;
    store.save_metadata(&session2).unwrap();

    let meta2 = store.load_metadata(TEST_USER, "state-transitions").unwrap();
    assert_eq!(meta2.state, SessionState::Completed);
}

/// Test that token usage accumulates across save→load→add→save cycles.
///
/// Original version of this test created a fresh `Session` for the
/// second turn, so it never actually exercised accumulation. The
/// real persistence contract is: a session that has been saved,
/// reloaded, had more usage added, and saved again must reflect
/// the cumulative total on next load.
#[test]
fn test_session_persistence_token_usage_accumulation() {
    let (store, _temp) = make_store();

    // Turn 1: create a new session, record first usage, save.
    let mut session = make_session("token-usage");
    session.add_token_usage(1000, 500, Some(200));
    store.save_metadata(&session).unwrap();

    // Reload from disk; confirm first turn persisted.
    let loaded = store.load_metadata(TEST_USER, "token-usage").unwrap();
    assert_eq!(loaded.token_usage.total_tokens, 1500);

    // Turn 2: simulate resuming the session by loading it back into
    // a `Session` object, then adding more usage.
    let mut resumed = make_session("token-usage");
    resumed.token_usage = loaded.token_usage.clone();
    resumed.add_token_usage(800, 400, Some(100));
    store.save_metadata(&resumed).unwrap();

    // Final reload must reflect the cumulative total (1500 + 1200 = 2700).
    // `total_tokens` only sums input + output; `cached_tokens` is a
    // separate counter, not part of `total_tokens`.
    let final_meta = store.load_metadata(TEST_USER, "token-usage").unwrap();
    assert_eq!(
        final_meta.token_usage.total_tokens, 2700,
        "token usage should accumulate across save→load→add→save; got {}",
        final_meta.token_usage.total_tokens
    );
}

/// Test that the session directory path is correctly structured.
#[test]
fn test_session_persistence_directory_structure() {
    let (store, _temp) = make_store();
    let session = make_session("dir-structure");
    store.save_metadata(&session).unwrap();

    let session_dir = store.session_dir(TEST_USER, "dir-structure");
    assert!(session_dir.exists());
    assert!(session_dir.join("metadata.json").exists());

    // Append a message and verify messages.jsonl is created
    store
        .append_message(
            TEST_USER,
            "dir-structure",
            &TestMessage {
                role: "user".to_string(),
                content: "test".to_string(),
            },
        )
        .unwrap();

    assert!(session_dir.join("messages.jsonl").exists());
}

/// Test loading metadata from corrupted JSON fails gracefully.
#[test]
fn test_session_persistence_corrupted_metadata() {
    let (store, _temp) = make_store();
    let session = make_session("corrupted");
    store.save_metadata(&session).unwrap();

    // Corrupt the metadata file
    let dir = store.session_dir(TEST_USER, "corrupted");
    fs::write(dir.join("metadata.json"), "not valid json{{{").unwrap();

    let result = store.load_metadata(TEST_USER, "corrupted");
    assert!(result.is_err());
}

/// Test concurrent-style: interleaved messages from different sessions.
#[test]
fn test_session_persistence_interleaved_messages() {
    let (store, _temp) = make_store();
    let s1 = make_session("interleaved-1");
    let s2 = make_session("interleaved-2");
    store.save_metadata(&s1).unwrap();
    store.save_metadata(&s2).unwrap();

    // Interleave messages
    store
        .append_message(
            TEST_USER,
            "interleaved-1",
            &TestMessage {
                role: "user".to_string(),
                content: "s1-msg-1".to_string(),
            },
        )
        .unwrap();
    store
        .append_message(
            TEST_USER,
            "interleaved-2",
            &TestMessage {
                role: "user".to_string(),
                content: "s2-msg-1".to_string(),
            },
        )
        .unwrap();
    store
        .append_message(
            TEST_USER,
            "interleaved-1",
            &TestMessage {
                role: "user".to_string(),
                content: "s1-msg-2".to_string(),
            },
        )
        .unwrap();

    let msgs1: Vec<TestMessage> =
        store.load_messages_all(TEST_USER, "interleaved-1").unwrap();
    assert_eq!(msgs1.len(), 2);
    assert_eq!(msgs1[0].content, "s1-msg-1");
    assert_eq!(msgs1[1].content, "s1-msg-2");

    let msgs2: Vec<TestMessage> =
        store.load_messages_all(TEST_USER, "interleaved-2").unwrap();
    assert_eq!(msgs2.len(), 1);
    assert_eq!(msgs2[0].content, "s2-msg-1");
}

/// Test that raw message append works with arbitrary JSON strings.
#[test]
fn test_session_persistence_raw_message_append() {
    let (store, _temp) = make_store();
    let session = make_session("raw-append");
    store.save_metadata(&session).unwrap();

    store
        .append_message_raw(
            TEST_USER,
            "raw-append",
            r#"{"role":"system","content":"You are a test assistant"}"#,
        )
        .unwrap();

    let messages: Vec<TestMessage> =
        store.load_messages_all(TEST_USER, "raw-append").unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].role, "system");
    assert_eq!(messages[0].content, "You are a test assistant");
}

/// Test lazy loading: load_messages_recent with default 100 limit.
#[test]
fn test_session_persistence_lazy_loading_default_limit() {
    let (store, _temp) = make_store();
    let session = make_session("lazy-loading");
    store.save_metadata(&session).unwrap();

    for i in 0..250 {
        store
            .append_message(
                TEST_USER,
                "lazy-loading",
                &TestMessage {
                    role: "user".to_string(),
                    content: format!("msg-{}", i),
                },
            )
            .unwrap();
    }

    let recent: Vec<TestMessage> = store
        .load_messages_recent(TEST_USER, "lazy-loading", 100)
        .unwrap();
    assert_eq!(recent.len(), 100);
    assert_eq!(recent[0].content, "msg-150");
    assert_eq!(recent[99].content, "msg-249");
}

/// Test on-demand loading of older messages beyond the initial batch.
#[test]
fn test_session_persistence_load_older_messages_on_demand() {
    let (store, _temp) = make_store();
    let session = make_session("older-messages");
    store.save_metadata(&session).unwrap();

    for i in 0..200 {
        store
            .append_message(
                TEST_USER,
                "older-messages",
                &TestMessage {
                    role: "user".to_string(),
                    content: format!("msg-{}", i),
                },
            )
            .unwrap();
    }

    let recent: Vec<TestMessage> = store
        .load_messages_recent(TEST_USER, "older-messages", 100)
        .unwrap();
    assert_eq!(recent.len(), 100);
    assert_eq!(recent[0].content, "msg-100");

    let older: Vec<TestMessage> = store
        .load_messages_older_than(TEST_USER, "older-messages", 100, 100)
        .unwrap();
    assert_eq!(older.len(), 100);
    assert_eq!(older[0].content, "msg-0");
    assert_eq!(older[99].content, "msg-99");
}

/// Test loading older messages when fewer exist than requested.
#[test]
fn test_session_persistence_load_older_fewer_than_requested() {
    let (store, _temp) = make_store();
    let session = make_session("fewer-messages");
    store.save_metadata(&session).unwrap();

    for i in 0..50 {
        store
            .append_message(
                TEST_USER,
                "fewer-messages",
                &TestMessage {
                    role: "user".to_string(),
                    content: format!("msg-{}", i),
                },
            )
            .unwrap();
    }

    let recent: Vec<TestMessage> = store
        .load_messages_recent(TEST_USER, "fewer-messages", 100)
        .unwrap();
    assert_eq!(recent.len(), 50);

    let older: Vec<TestMessage> = store
        .load_messages_older_than(TEST_USER, "fewer-messages", 50, 100)
        .unwrap();
    assert!(older.is_empty());
}

/// Test token budget with 90% MustCompact threshold.
#[test]
fn test_session_persistence_token_budget_90_percent_threshold() {
    let budget = TokenBudget::new(10_000);

    assert_eq!(budget.check(6_999), synthia_session::TokenBudgetStatus::Ok);
    assert_eq!(
        budget.check(7_000),
        synthia_session::TokenBudgetStatus::Notice
    );
    assert_eq!(
        budget.check(8_499),
        synthia_session::TokenBudgetStatus::Notice
    );
    assert_eq!(
        budget.check(8_500),
        synthia_session::TokenBudgetStatus::Warning
    );
    assert_eq!(
        budget.check(8_999),
        synthia_session::TokenBudgetStatus::Warning
    );
    assert_eq!(
        budget.check(9_000),
        synthia_session::TokenBudgetStatus::MustCompact
    );
    assert_eq!(
        budget.check(10_000),
        synthia_session::TokenBudgetStatus::MustCompact
    );
}

/// Test that must_compact_at field exists and is 90% of hard_limit.
#[test]
fn test_session_persistence_must_compact_at_field() {
    let budget = TokenBudget::new(1_000);
    assert_eq!(budget.soft_limit, 700);
    assert_eq!(budget.compaction_at, 850);
    assert_eq!(budget.must_compact_at, 900);
    assert_eq!(budget.hard_limit, 1_000);
}
