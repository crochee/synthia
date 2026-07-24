use chrono::Utc;

use super::*;
use crate::{
    cold::store::{MemoryStore, SearchMode, SearchQuery},
    types::ColdEntry,
};

fn test_entry(id: &str, content: &str) -> ColdEntry {
    ColdEntry {
        id: id.to_string(),
        content: content.to_string(),
        metadata: serde_json::Value::Null,
        created_at: Utc::now(),
        timestamp: None,
        summary: None,
        session_id: None,
        tools_used: None,
        outcome: None,
        importance_score: 0.5,
        access_count: 0,
    }
}

#[tokio::test]
async fn test_sqlite_store_insert_and_get() {
    let store = SqliteStore::new("sqlite::memory:").await.unwrap();
    let entry = test_entry("1", "hello world");
    store.insert(&entry).await.unwrap();
    let result = store.get("1").await.unwrap();
    assert!(result.is_some());
    assert_eq!(result.unwrap().id, "1");
}

#[tokio::test]
async fn test_sqlite_store_batch_insert() {
    let store = SqliteStore::new("sqlite::memory:").await.unwrap();
    let entries: Vec<_> = (1..=5)
        .map(|i| test_entry(&i.to_string(), &format!("content {}", i)))
        .collect();
    let refs: Vec<_> = entries.iter().collect();
    store.insert_batch(&refs).await.unwrap();
    for i in 1..=5 {
        assert!(store.get(&i.to_string()).await.unwrap().is_some());
    }
}

#[tokio::test]
async fn test_sqlite_store_search() {
    let store = SqliteStore::new("sqlite::memory:").await.unwrap();
    store
        .insert(&test_entry("1", "apple banana"))
        .await
        .unwrap();
    store
        .insert(&test_entry("2", "banana cherry"))
        .await
        .unwrap();
    let hits = store
        .search(&SearchQuery {
            query: "banana".to_string(),
            limit: 10,
            mode: SearchMode::Similarity,
        })
        .await
        .unwrap();
    assert_eq!(hits.len(), 2);
}
