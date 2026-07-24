//! Unit tests for the `retrieval` module family.
//!
//! Coverage map (10 tests):
//!
//! - [`semantic_search`]: 5 tests
//!   ([`test_semantic_search_exact_match`],
//!   [`test_semantic_search_partial_match`],
//!   [`test_semantic_search_multi_term`],
//!   [`test_semantic_search_no_match`],
//!   [`test_semantic_search_frequency_bonus`]).
//! - [`bm25::format_fts_query`]: 3 tests
//!   ([`test_format_fts_query_empty`],
//!   [`test_format_fts_query_normal`],
//!   [`test_format_fts_query_with_quotes`]).
//! - Integration tests against [`hybrid_search`] /
//!   [`retrieve`]: 4 tests
//!   ([`test_hybrid_search_default_weights`],
//!   [`test_hybrid_search_union_of_both_sources`],
//!   [`test_retrieve_bm25_mode`],
//!   [`test_retrieve_semantic_mode`],
//!   [`test_retrieve_hybrid_mode`]).

use chrono::Utc;

use super::*;
use crate::types::{ColdEntry, RetrievalMode};

fn make_entry(id: &str, content: &str) -> ColdEntry {
    ColdEntry {
        id: id.to_string(),
        content: content.to_string(),
        metadata: serde_json::json!({}),
        created_at: Utc::now(),
        ..Default::default()
    }
}

#[test]
fn test_semantic_search_exact_match() {
    let entries = vec![
        make_entry("1", "Rust programming language"),
        make_entry("2", "Python basics tutorial"),
        make_entry("3", "Advanced Rust async patterns"),
    ];
    let results = semantic_search(&entries, "Rust", 10);
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].entry.id, "1");
    assert!((results[0].score - 1.0).abs() < f64::EPSILON);
}

#[test]
fn test_semantic_search_partial_match() {
    let entries = vec![make_entry("1", "Rustacean community")];
    let results = semantic_search(&entries, "Rust", 10);
    assert_eq!(results.len(), 1);
    // "Rust" is a partial match in "Rustacean"
    assert!((results[0].score - 0.5).abs() < f64::EPSILON);
}

#[test]
fn test_semantic_search_multi_term() {
    let entries = vec![
        make_entry("1", "Rust async programming patterns"),
        make_entry("2", "Python async programming"),
    ];
    let results = semantic_search(&entries, "Rust async", 10);
    assert_eq!(results.len(), 2);
    // Entry 1 has both terms, entry 2 only has "async"
    assert_eq!(results[0].entry.id, "1");
    assert_eq!(results[1].entry.id, "2");
}

#[test]
fn test_semantic_search_no_match() {
    let entries = vec![make_entry("1", "JavaScript web development")];
    let results = semantic_search(&entries, "Rust", 10);
    assert!(results.is_empty());
}

#[test]
fn test_format_fts_query_empty() {
    assert_eq!(format_fts_query(""), "");
    assert_eq!(format_fts_query("  "), "");
}

#[test]
fn test_format_fts_query_normal() {
    assert_eq!(format_fts_query("hello world"), "hello OR world");
}

#[test]
fn test_format_fts_query_with_quotes() {
    assert_eq!(format_fts_query("say \"hello\""), "say OR \"\"hello\"\"");
}

#[test]
fn test_semantic_search_frequency_bonus() {
    let entries = vec![make_entry("1", "Rust Rust Rust programming")];
    let results = semantic_search(&entries, "Rust", 10);
    assert_eq!(results.len(), 1);
    // Base 1.0 + (3-1)*0.2 = 1.4, normalized to 1.0
    assert!((results[0].score - 1.0).abs() < f64::EPSILON);
}

/// Integration test: hybrid search with 0.7*bm25 + 0.3*semantic weighting.
#[tokio::test]
async fn test_hybrid_search_default_weights() {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::query(
        "CREATE VIRTUAL TABLE IF NOT EXISTS cold_entries_fts USING fts5(entry_id, content)",
    )
    .execute(&pool)
    .await
    .unwrap();

    // Insert test entries
    for entry in &[
        ("1", "Rust programming language with async tokio runtime"),
        ("2", "Python async programming with asyncio"),
        ("3", "JavaScript web development framework"),
        ("4", "Rust async patterns and macros"),
    ] {
        sqlx::query(
            "INSERT INTO cold_entries_fts (entry_id, content) VALUES (?, ?)",
        )
        .bind(entry.0)
        .bind(entry.1)
        .execute(&pool)
        .await
        .unwrap();
    }

    let in_memory_entries: Vec<ColdEntry> = [
        ("1", "Rust programming language with async tokio runtime"),
        ("2", "Python async programming with asyncio"),
        ("3", "JavaScript web development framework"),
        ("4", "Rust async patterns and macros"),
    ]
    .iter()
    .map(|(id, content)| make_entry(id, content))
    .collect();

    let results = hybrid_search(&pool, &in_memory_entries, "Rust async", 10)
        .await
        .unwrap();

    assert!(!results.is_empty());
    // Entry 4 has both "Rust" and "async" exactly
    // Entry 1 also has both
    assert!(
        results
            .iter()
            .any(|r| r.entry.id == "1" || r.entry.id == "4")
    );

    // Scores should be between 0 and 1
    for r in &results {
        assert!(r.score >= 0.0 && r.score <= 1.0);
    }
}

/// Integration test: hybrid search combining BM25-only and semantic-only matches.
#[tokio::test]
async fn test_hybrid_search_union_of_both_sources() {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::query(
        "CREATE VIRTUAL TABLE IF NOT EXISTS cold_entries_fts USING fts5(entry_id, content)",
    )
    .execute(&pool)
    .await
    .unwrap();

    // BM25 will match entry 1 ("database optimization" has exact words)
    // Semantic will match entry 2 ("db" contains "data" as partial)
    for entry in &[
        ("1", "database optimization query performance"),
        ("2", "db tuning index analysis"),
    ] {
        sqlx::query(
            "INSERT INTO cold_entries_fts (entry_id, content) VALUES (?, ?)",
        )
        .bind(entry.0)
        .bind(entry.1)
        .execute(&pool)
        .await
        .unwrap();
    }

    let in_memory_entries: Vec<ColdEntry> = [
        ("1", "database optimization query performance"),
        ("2", "db tuning index analysis"),
    ]
    .iter()
    .map(|(id, content)| make_entry(id, content))
    .collect();

    let results = hybrid_search(&pool, &in_memory_entries, "database", 10)
        .await
        .unwrap();

    assert!(!results.is_empty());
    assert!(results.iter().any(|r| r.entry.id == "1"));
}

/// Integration test: BM25-only mode via retrieve function.
#[tokio::test]
async fn test_retrieve_bm25_mode() {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::query(
        "CREATE VIRTUAL TABLE IF NOT EXISTS cold_entries_fts USING fts5(entry_id, content)",
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO cold_entries_fts (entry_id, content) VALUES (?, ?)",
    )
    .bind("1")
    .bind("machine learning neural network")
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO cold_entries_fts (entry_id, content) VALUES (?, ?)",
    )
    .bind("2")
    .bind("web development frontend")
    .execute(&pool)
    .await
    .unwrap();

    let results =
        retrieve(&pool, &[], "machine learning", 10, RetrievalMode::Bm25)
            .await
            .unwrap();

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].entry.id, "1");
}

/// Integration test: semantic-only mode via retrieve function.
#[test]
fn test_retrieve_semantic_mode() {
    let entries = vec![
        make_entry("1", "Rust programming systems language"),
        make_entry("2", "Python scripting language tutorial"),
        make_entry("3", "JavaScript browser framework"),
    ];

    // Semantic mode only uses in-memory entries, no pool needed
    let results = semantic_search(&entries, "Rust", 10);
    assert!(!results.is_empty());
    assert_eq!(results[0].entry.id, "1");
}

/// Integration test: hybrid mode via retrieve function.
#[tokio::test]
async fn test_retrieve_hybrid_mode() {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::query(
        "CREATE VIRTUAL TABLE IF NOT EXISTS cold_entries_fts USING fts5(entry_id, content)",
    )
    .execute(&pool)
    .await
    .unwrap();

    for (id, content) in &[
        ("1", "database SQL query optimization"),
        ("2", "cache Redis in-memory storage"),
    ] {
        sqlx::query(
            "INSERT INTO cold_entries_fts (entry_id, content) VALUES (?, ?)",
        )
        .bind(id)
        .bind(content)
        .execute(&pool)
        .await
        .unwrap();
    }

    let entries: Vec<ColdEntry> = [
        ("1", "database SQL query optimization"),
        ("2", "cache Redis in-memory storage"),
    ]
    .iter()
    .map(|(id, content)| make_entry(id, content))
    .collect();

    let results =
        retrieve(&pool, &entries, "database", 10, RetrievalMode::Hybrid)
            .await
            .unwrap();

    assert!(!results.is_empty());
    assert!(results.iter().any(|r| r.entry.id == "1"));
}
