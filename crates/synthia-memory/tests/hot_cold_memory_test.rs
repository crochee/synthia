//! Unit tests for Hot/Cold memory interface via [`MemoryStoreImpl`].
//!
//! Tests cover:
//! - Hot memory write/read roundtrip
//! - Cold memory persistence (append + search)
//! - Memory retrieval by key
//! - Importance scoring via [`ColdEntry`] mutators

use chrono::Utc;
use synthia_memory::{
    store::MemoryStoreImpl,
    types::{ColdEntry, HotEntry, MemoryStore},
};

fn make_store() -> (MemoryStoreImpl, tempfile::TempDir) {
    let temp = tempfile::tempdir().unwrap();
    let base = temp.path().to_path_buf();
    (MemoryStoreImpl::new(base), temp)
}

// ---------------------------------------------------------------------------
// Hot memory tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_hot_memory_write_and_read() {
    let (store, _temp) = make_store();

    store
        .write_hot("project_notes", "# Important Project\nKey decisions")
        .await
        .unwrap();

    let content = store.read_hot("project_notes").await.unwrap();
    assert!(content.is_some());
    assert!(content.unwrap().contains("Important Project"));
}

#[tokio::test]
async fn test_hot_memory_overwrite() {
    let (store, _temp) = make_store();

    store.write_hot("key1", "first value").await.unwrap();
    store.write_hot("key1", "second value").await.unwrap();

    let content = store.read_hot("key1").await.unwrap();
    assert_eq!(content.unwrap(), "second value");
}

#[tokio::test]
async fn test_hot_memory_read_nonexistent() {
    let (store, _temp) = make_store();

    let content = store.read_hot("nonexistent_key").await.unwrap();
    assert!(content.is_none());
}

#[tokio::test]
async fn test_hot_memory_multiple_keys() {
    let (store, _temp) = make_store();

    store.write_hot("key_a", "value a").await.unwrap();
    store.write_hot("key_b", "value b").await.unwrap();
    store.write_hot("key_c", "value c").await.unwrap();

    assert_eq!(store.read_hot("key_a").await.unwrap().unwrap(), "value a");
    assert_eq!(store.read_hot("key_b").await.unwrap().unwrap(), "value b");
    assert_eq!(store.read_hot("key_c").await.unwrap().unwrap(), "value c");
}

// ---------------------------------------------------------------------------
// Cold memory persistence tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_cold_memory_append_and_search() {
    let (store, _temp) = make_store();

    store
        .append_cold_fields(
            "Rust async programming",
            "sess-rust-1",
            vec!["read".to_string(), "write".to_string()],
            "completed successfully",
        )
        .await
        .unwrap();

    let results = store.search_cold("Rust", 10).await.unwrap();
    assert_eq!(results.len(), 1);
    assert!(results[0].content.contains("Rust async programming"));
}

#[tokio::test]
async fn test_cold_memory_multiple_entries() {
    let (store, _temp) = make_store();

    store
        .append_cold_fields(
            "Python tutorial",
            "sess-py-1",
            vec!["run".to_string()],
            "ok",
        )
        .await
        .unwrap();
    store
        .append_cold_fields(
            "Rust API design",
            "sess-rust-1",
            vec!["read".to_string()],
            "ok",
        )
        .await
        .unwrap();
    store
        .append_cold_fields(
            "JavaScript basics",
            "sess-js-1",
            vec!["edit".to_string()],
            "ok",
        )
        .await
        .unwrap();

    let rust_results = store.search_cold("Rust", 10).await.unwrap();
    assert_eq!(rust_results.len(), 1);

    let py_results = store.search_cold("Python", 10).await.unwrap();
    assert_eq!(py_results.len(), 1);

    let js_results = store.search_cold("JavaScript", 10).await.unwrap();
    assert_eq!(js_results.len(), 1);
}

#[tokio::test]
async fn test_cold_memory_search_limit() {
    let (store, _temp) = make_store();

    for i in 0..20 {
        store
            .append_cold_fields(
                &format!("Rust task number {}", i),
                &format!("sess-{}", i),
                vec!["tool".to_string()],
                "success",
            )
            .await
            .unwrap();
    }

    let results = store.search_cold("Rust", 5).await.unwrap();
    assert_eq!(results.len(), 5);
}

#[tokio::test]
async fn test_cold_memory_search_no_match() {
    let (store, _temp) = make_store();

    store
        .append_cold_fields("Python data science", "sess-1", vec![], "ok")
        .await
        .unwrap();

    let results = store.search_cold("Rust", 10).await.unwrap();
    assert!(results.is_empty());
}

#[tokio::test]
async fn test_cold_memory_empty_query_returns_empty() {
    let (store, _temp) = make_store();

    store
        .append_cold_fields("Some content", "sess-1", vec![], "ok")
        .await
        .unwrap();

    let results = store.search_cold("", 10).await.unwrap();
    assert!(results.is_empty());
}

#[tokio::test]
async fn test_cold_memory_persistence_via_reload() {
    let temp = tempfile::tempdir().unwrap();
    let base = temp.path().to_path_buf();

    // First store: write entries
    {
        let store = MemoryStoreImpl::new(base.clone());
        store
            .append_cold_fields(
                "Persisted content",
                "sess-1",
                vec![],
                "success",
            )
            .await
            .unwrap();
        store
            .write_hot("hot_key", "hot persisted value")
            .await
            .unwrap();
    }

    // Second store: reload from same directory
    {
        let store = MemoryStoreImpl::new(base);
        let cold_results = store.search_cold("Persisted", 10).await.unwrap();
        assert_eq!(cold_results.len(), 1);

        let hot_value = store.read_hot("hot_key").await.unwrap();
        assert_eq!(hot_value.unwrap(), "hot persisted value");
    }
}

// ---------------------------------------------------------------------------
// Memory retrieval by key
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_memory_retrieval_by_key_hot() {
    let (store, _temp) = make_store();

    store
        .write_hot("memory", "# System Memory\nAgent prefers async")
        .await
        .unwrap();

    let retrieved = store.read_hot("memory").await.unwrap();
    assert!(retrieved.unwrap().contains("Agent prefers async"));
}

#[tokio::test]
async fn test_memory_retrieval_by_key_cold() {
    let (store, _temp) = make_store();

    store
        .append_cold_fields(
            "Agent uses ReAct pattern",
            "sess-1",
            vec!["think".to_string(), "act".to_string()],
            "success",
        )
        .await
        .unwrap();

    let results = store.search_cold("ReAct", 10).await.unwrap();
    assert!(!results.is_empty());
    assert!(results[0].content.contains("ReAct"));
}

// ---------------------------------------------------------------------------
// Importance scoring tests
// ---------------------------------------------------------------------------

#[test]
fn test_cold_entry_importance_decay() {
    let mut entry = ColdEntry {
        id: "test-1".to_string(),
        content: "Test content".to_string(),
        metadata: serde_json::json!({}),
        created_at: Utc::now(),
        importance_score: 0.9,
        access_count: 5,
        ..Default::default()
    };

    // Apply decay
    entry.update_importance(0.5);
    assert!((entry.importance_score - 0.45).abs() < f64::EPSILON);
}

#[test]
fn test_cold_entry_increment_access() {
    let mut entry = ColdEntry {
        id: "test-1".to_string(),
        content: "Test content".to_string(),
        metadata: serde_json::json!({}),
        created_at: Utc::now(),
        importance_score: 0.5,
        access_count: 0,
        ..Default::default()
    };

    entry.increment_access();
    assert_eq!(entry.access_count, 1);
    // Importance should increase after access
    assert!(entry.importance_score > 0.5);
}

#[test]
fn test_hot_entry_default_importance() {
    let entry = HotEntry {
        key: "test".to_string(),
        value: "value".to_string(),
        updated_at: Utc::now(),
        importance_score: 0.5,
    };
    assert!((entry.importance_score - 0.5).abs() < f64::EPSILON);
}

#[test]
fn test_cold_entry_importance_full_decay() {
    let mut entry = ColdEntry {
        id: "test-1".to_string(),
        content: "Test content".to_string(),
        metadata: serde_json::json!({}),
        created_at: Utc::now(),
        importance_score: 1.0,
        access_count: 0,
        ..Default::default()
    };

    // Multiple decays
    entry.update_importance(0.8);
    entry.update_importance(0.8);
    entry.update_importance(0.8);
    // 1.0 * 0.8 * 0.8 * 0.8 = 0.512
    assert!((entry.importance_score - 0.512).abs() < 0.001);
}
