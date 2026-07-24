//! 11 unit tests for the `store` module family.
//!
//! Coverage map:
//!
//! - Hot memory: 2 tests (write_and_read_hot /
//!   read_nonexistent_hot).
//! - Cold JSONL: 2 tests (append_and_search_cold /
//!   search_cold_limit).
//! - Episodic JSONL: 2 tests (write_and_load_episodic /
//!   load_episodic_no_match).
//! - Context: 1 test (set_and_compact_context).
//! - Trait impl: 2 tests (write_cold_from_cold_entry /
//!   save_episodic_from_skill).
//! - Retrieval fallback: 1 test
//!   (search_cold_with_mode_jsonl_fallback).

use chrono::Utc;

use super::*;
use crate::{
    context::ContextMessage,
    types::{ColdEntry, EpisodicSkill, MemoryStore, RetrievalMode},
};

fn make_store() -> MemoryStoreImpl {
    let temp_dir = tempfile::tempdir().unwrap();
    let base = temp_dir.path().to_path_buf();
    MemoryStoreImpl::new(base)
}

#[tokio::test]
async fn test_write_and_read_hot() {
    let store = make_store();
    store.write_hot("memory", "# Project notes").await.unwrap();
    let content = store.read_hot("memory").await.unwrap();
    assert!(content.unwrap().contains("# Project notes"));
}

#[tokio::test]
async fn test_read_nonexistent_hot() {
    let store = make_store();
    let result = store.read_hot("missing").await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn test_append_and_search_cold() {
    let store = make_store();
    store
        .append_cold_fields(
            "Rust API build",
            "sess-1",
            vec!["read".to_string()],
            "success",
        )
        .await
        .unwrap();
    store
        .append_cold_fields(
            "Python data analysis",
            "sess-2",
            vec!["run".to_string()],
            "success",
        )
        .await
        .unwrap();

    let results = store.search_cold("Rust", 10).await.unwrap();
    assert_eq!(results.len(), 1);
    assert!(results[0].content.contains("Rust API build"));
}

#[tokio::test]
async fn test_search_cold_limit() {
    let store = make_store();
    for i in 0..10 {
        store
            .append_cold_fields(
                &format!("Rust task {}", i),
                &format!("sess-{}", i),
                vec![],
                "ok",
            )
            .await
            .unwrap();
    }

    let results = store.search_cold("Rust", 3).await.unwrap();
    assert_eq!(results.len(), 3);
}

#[tokio::test]
async fn test_write_and_load_episodic() {
    let store = make_store();
    store
        .write_episodic_fields(
            "Build REST API",
            vec!["read".to_string(), "write".to_string()],
            5,
            1200.0,
        )
        .await
        .unwrap();

    let entries = store.load_episodic_jsonl("Build").await.unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].task_hint, "Build REST API");
}

#[tokio::test]
async fn test_load_episodic_no_match() {
    let store = make_store();
    store
        .write_episodic_fields("Rust async", vec![], 1, 500.0)
        .await
        .unwrap();

    let entries = store.load_episodic_jsonl("Python").await.unwrap();
    assert!(entries.is_empty());
}

#[tokio::test]
async fn test_set_and_compact_context() {
    let store = make_store();
    let messages = vec![
        ContextMessage {
            role: "user".to_string(),
            content: "Hello".to_string(),
        },
        ContextMessage {
            role: "assistant".to_string(),
            content: "Hi there".to_string(),
        },
    ];
    store.set_context("sess-1", messages).await.unwrap();

    let report = store.compact_context("sess-1").await.unwrap();
    assert!(report.tokens_before > 0);
}

#[tokio::test]
async fn test_write_cold_from_cold_entry() {
    let store = make_store();
    let entry = ColdEntry {
        id: "sess-1".to_string(),
        content: "Summary of session".to_string(),
        metadata: serde_json::json!({
            "tools_used": ["read", "write"],
            "outcome": "success"
        }),
        created_at: Utc::now(),
        ..Default::default()
    };
    store.write_cold(entry).await.unwrap();

    let results = store.search_cold("Summary", 10).await.unwrap();
    assert_eq!(results.len(), 1);
}

#[tokio::test]
async fn test_save_episodic_from_skill() {
    let store = make_store();
    let skill = EpisodicSkill {
        task_hint: "data analysis".to_string(),
        skill_content: "Analysis workflow".to_string(),
        success_rate: 0.8,
        used_at: Utc::now(),
    };
    store.save_episodic(skill).await.unwrap();

    let skills = store.load_episodic("data").await.unwrap();
    assert_eq!(skills.len(), 1);
}

#[tokio::test]
async fn test_search_cold_with_mode_jsonl_fallback() {
    let store = make_store();
    store
        .append_cold_fields(
            "Rust API build",
            "sess-1",
            vec!["read".to_string()],
            "success",
        )
        .await
        .unwrap();

    // Without SQLite backend, falls back to keyword matching
    let result = store
        .search_cold_with_mode("Rust", 10, RetrievalMode::Hybrid)
        .await;
    assert!(result.is_ok());
    let results = result.unwrap();
    assert!(!results.is_empty());
    assert!(results[0].entry.content.contains("Rust API build"));
}
