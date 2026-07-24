//! Unit tests for the [`admin`] module.
//!
//! Tests cover:
//! - entry_count, delete_entries, load_all_entries, flush_to_file

use chrono::Utc;

use crate::{cold::ColdMemory, types::ColdEntry};

fn make_entry(id: &str, content: &str) -> ColdEntry {
    ColdEntry {
        id: id.to_string(),
        content: content.to_string(),
        metadata: serde_json::json!({}),
        created_at: Utc::now(),
        importance_score: 0.5,
        access_count: 0,
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// entry_count tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_entry_count_empty() {
    let memory = ColdMemory::new_in_memory().await.unwrap();
    assert_eq!(memory.entry_count().await.unwrap(), 0);
}

#[tokio::test]
async fn test_entry_count_after_append() {
    let memory = ColdMemory::new_in_memory().await.unwrap();
    memory.append(make_entry("e1", "first")).await.unwrap();
    assert_eq!(memory.entry_count().await.unwrap(), 1);
    memory.append(make_entry("e2", "second")).await.unwrap();
    assert_eq!(memory.entry_count().await.unwrap(), 2);
}

// ---------------------------------------------------------------------------
// delete_entries tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_delete_single_entry() {
    let memory = ColdMemory::new_in_memory().await.unwrap();
    memory.append(make_entry("e1", "to delete")).await.unwrap();
    assert_eq!(memory.entry_count().await.unwrap(), 1);

    let deleted = memory.delete_entries(&["e1"]).await.unwrap();
    assert_eq!(deleted, 1);
    assert_eq!(memory.entry_count().await.unwrap(), 0);
}

#[tokio::test]
async fn test_delete_multiple_entries() {
    let memory = ColdMemory::new_in_memory().await.unwrap();
    memory.append(make_entry("a", "alpha")).await.unwrap();
    memory.append(make_entry("b", "beta")).await.unwrap();
    memory.append(make_entry("c", "charlie")).await.unwrap();

    let deleted = memory.delete_entries(&["a", "c"]).await.unwrap();
    assert_eq!(deleted, 2);
    assert_eq!(memory.entry_count().await.unwrap(), 1);
}

#[tokio::test]
async fn test_delete_empty_list_is_noop() {
    let memory = ColdMemory::new_in_memory().await.unwrap();
    memory.append(make_entry("e1", "content")).await.unwrap();
    let deleted = memory.delete_entries(&[]).await.unwrap();
    assert_eq!(deleted, 0);
    assert_eq!(memory.entry_count().await.unwrap(), 1);
}

#[tokio::test]
async fn test_delete_nonexistent_id() {
    let memory = ColdMemory::new_in_memory().await.unwrap();
    memory.append(make_entry("e1", "exists")).await.unwrap();
    let deleted = memory.delete_entries(&["nonexistent"]).await.unwrap();
    assert_eq!(deleted, 0);
    assert_eq!(memory.entry_count().await.unwrap(), 1);
}

// ---------------------------------------------------------------------------
// load_all_entries tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_load_all_entries_empty() {
    let memory = ColdMemory::new_in_memory().await.unwrap();
    let entries = memory.load_all_entries().await.unwrap();
    assert!(entries.is_empty());
}

#[tokio::test]
async fn test_load_all_entries_returns_all() {
    let memory = ColdMemory::new_in_memory().await.unwrap();
    memory.append(make_entry("a", "alpha")).await.unwrap();
    memory.append(make_entry("b", "beta")).await.unwrap();
    memory.append(make_entry("c", "charlie")).await.unwrap();

    let entries = memory.load_all_entries().await.unwrap();
    assert_eq!(entries.len(), 3);
    let ids: Vec<_> = entries.iter().map(|e| e.id.clone()).collect();
    assert!(ids.contains(&"a".to_string()));
    assert!(ids.contains(&"b".to_string()));
    assert!(ids.contains(&"c".to_string()));
}

#[tokio::test]
async fn test_load_all_entries_ordered_by_created_at_desc() {
    let memory = ColdMemory::new_in_memory().await.unwrap();
    memory.append(make_entry("first", "first")).await.unwrap();
    memory.append(make_entry("second", "second")).await.unwrap();
    memory.append(make_entry("third", "third")).await.unwrap();

    let entries = memory.load_all_entries().await.unwrap();
    assert_eq!(entries[0].id, "third");
    assert_eq!(entries[1].id, "second");
    assert_eq!(entries[2].id, "first");
}

#[tokio::test]
async fn test_load_all_entries_after_delete() {
    let memory = ColdMemory::new_in_memory().await.unwrap();
    memory.append(make_entry("1", "one")).await.unwrap();
    memory.append(make_entry("2", "two")).await.unwrap();
    memory.append(make_entry("3", "three")).await.unwrap();
    memory.delete_entries(&["2"]).await.unwrap();

    let entries = memory.load_all_entries().await.unwrap();
    assert_eq!(entries.len(), 2);
    let ids: Vec<_> = entries.iter().map(|e| e.id.clone()).collect();
    assert!(ids.contains(&"1".to_string()));
    assert!(ids.contains(&"3".to_string()));
    assert!(!ids.contains(&"2".to_string()));
}

// ---------------------------------------------------------------------------
// flush_to_file tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_flush_to_file_empty_is_error() {
    let temp = tempfile::tempdir().unwrap();
    let base = temp.path().to_path_buf();
    let memory = ColdMemory::new(base).await.unwrap();
    let result = memory.flush_to_file(temp.path()).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_flush_to_file_creates_markdown() {
    let temp = tempfile::tempdir().unwrap();
    let base = temp.path().to_path_buf();
    let memory = ColdMemory::new(base).await.unwrap();

    memory
        .append(make_entry("entry-1", "Memory content one"))
        .await
        .unwrap();
    memory
        .append(make_entry("entry-2", "Memory content two"))
        .await
        .unwrap();

    let path = memory.flush_to_file(temp.path()).await.unwrap();
    assert!(path.exists());

    let content = tokio::fs::read_to_string(&path).await.unwrap();
    assert!(content.contains("Memory Flush"));
    assert!(content.contains("Total entries: 2"));
    assert!(content.contains("entry-1"));
    assert!(content.contains("entry-2"));
    assert!(content.contains("Memory content one"));
    assert!(content.contains("Memory content two"));
}

#[tokio::test]
async fn test_flush_to_file_creates_memory_subdirectory() {
    let temp = tempfile::tempdir().unwrap();
    let base = temp.path().to_path_buf();
    let memory = ColdMemory::new(base).await.unwrap();
    memory.append(make_entry("x", "content")).await.unwrap();

    let path = memory.flush_to_file(temp.path()).await.unwrap();
    assert!(path.starts_with(temp.path().join("memory")));
}

#[tokio::test]
async fn test_flush_to_file_idempotent_per_day() {
    // Two stores opened sequentially on same temp dir same day
    let temp = tempfile::tempdir().unwrap();

    let base1 = temp.path().to_path_buf();
    let mem1 = ColdMemory::new(base1.clone()).await.unwrap();
    mem1.append(make_entry("day1", "first flush"))
        .await
        .unwrap();
    let path1 = mem1.flush_to_file(temp.path()).await.unwrap();

    let mem2 = ColdMemory::new(base1).await.unwrap();
    mem2.append(make_entry("day1-reloaded", "reloaded"))
        .await
        .unwrap();
    let path2 = mem2.flush_to_file(temp.path()).await.unwrap();

    // Same day = same filename (overwritten)
    assert_eq!(path1, path2);
}
