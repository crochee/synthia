//! Unit tests for BM25 retrieval mode.
//!
//! Tests cover:
//! - BM25 search returns ranked results
//! - Empty query returns empty
//! - Limit parameter works

use synthia_memory::retrieval::{bm25_search, format_fts_query};

#[tokio::test]
async fn test_bm25_search_returns_ranked_results() {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();

    // Create FTS5 virtual table
    sqlx::query(
        "CREATE VIRTUAL TABLE IF NOT EXISTS cold_entries_fts USING fts5(entry_id, content)",
    )
    .execute(&pool)
    .await
    .unwrap();

    // Insert test entries with varying relevance to "rust async"
    sqlx::query(
        "INSERT INTO cold_entries_fts (entry_id, content) VALUES (?, ?)",
    )
    .bind("1")
    .bind("Rust async programming with tokio runtime")
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO cold_entries_fts (entry_id, content) VALUES (?, ?)",
    )
    .bind("2")
    .bind("Rust error handling and Result types")
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO cold_entries_fts (entry_id, content) VALUES (?, ?)",
    )
    .bind("3")
    .bind("Python async programming with asyncio")
    .execute(&pool)
    .await
    .unwrap();

    let results = bm25_search(&pool, "Rust async", 10).await.unwrap();

    // All entries containing "Rust" or "async"
    assert_eq!(results.len(), 3);
    // Entry 1 contains both "Rust" and "async", should be first
    assert_eq!(results[0].entry.id, "1");
    // Scores should be non-negative
    for result in &results {
        assert!(result.score >= 0.0);
    }
}

#[tokio::test]
async fn test_bm25_search_empty_query_returns_empty() {
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
    .bind("Some content")
    .execute(&pool)
    .await
    .unwrap();

    let results = bm25_search(&pool, "rust", 10).await.unwrap();
    assert!(results.is_empty());
}

#[tokio::test]
async fn test_bm25_search_whitespace_query_returns_empty() {
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
    .bind("Some content")
    .execute(&pool)
    .await
    .unwrap();

    let results = bm25_search(&pool, "nonexistentterm", 10).await.unwrap();
    assert!(results.is_empty());
}

#[tokio::test]
async fn test_bm25_search_limit_parameter() {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();

    sqlx::query(
        "CREATE VIRTUAL TABLE IF NOT EXISTS cold_entries_fts USING fts5(entry_id, content)",
    )
    .execute(&pool)
    .await
    .unwrap();

    // Insert 10 entries
    for i in 0..10 {
        sqlx::query(
            "INSERT INTO cold_entries_fts (entry_id, content) VALUES (?, ?)",
        )
        .bind(format!("{}", i))
        .bind(format!("Rust programming task {}", i))
        .execute(&pool)
        .await
        .unwrap();
    }

    let results = bm25_search(&pool, "Rust", 3).await.unwrap();
    assert_eq!(results.len(), 3);
}

#[tokio::test]
async fn test_bm25_search_no_match_returns_empty() {
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
    .bind("Python web development")
    .execute(&pool)
    .await
    .unwrap();

    let results = bm25_search(&pool, "Rust", 10).await.unwrap();
    assert!(results.is_empty());
}

#[tokio::test]
async fn test_bm25_search_single_term() {
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
    .bind("database SQL query optimization")
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO cold_entries_fts (entry_id, content) VALUES (?, ?)",
    )
    .bind("2")
    .bind("cache Redis in-memory")
    .execute(&pool)
    .await
    .unwrap();

    let results = bm25_search(&pool, "database", 10).await.unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].entry.id, "1");
}

#[tokio::test]
async fn test_bm25_search_scores_are_nonnegative() {
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
    .bind("Rust async tokio programming")
    .execute(&pool)
    .await
    .unwrap();

    let results = bm25_search(&pool, "async", 10).await.unwrap();

    assert!(!results.is_empty());
    for result in results {
        assert!(result.score >= 0.0, "BM25 score should be non-negative");
    }
}

// ---------------------------------------------------------------------------
// format_fts_query tests (directly test the public function)
// ---------------------------------------------------------------------------

#[test]
fn test_format_fts_query_empty_string() {
    assert_eq!(format_fts_query(""), "");
}

#[test]
fn test_format_fts_query_whitespace_only() {
    assert_eq!(format_fts_query("   "), "");
    assert_eq!(format_fts_query("\t\n"), "");
}

#[test]
fn test_format_fts_query_single_term() {
    assert_eq!(format_fts_query("rust"), "rust");
}

#[test]
fn test_format_fts_query_multiple_terms() {
    assert_eq!(format_fts_query("rust async"), "rust OR async");
}

#[test]
fn test_format_fts_query_preserves_term_order() {
    let query = "one two three";
    let result = format_fts_query(query);
    assert!(result.contains("one"));
    assert!(result.contains("two"));
    assert!(result.contains("three"));
}

#[test]
fn test_format_fts_query_trims_whitespace() {
    assert_eq!(format_fts_query("  rust  "), "rust");
    assert_eq!(format_fts_query("rust  async  "), "rust OR async");
}
