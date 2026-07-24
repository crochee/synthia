//! Tests for the memory background task.

use std::{sync::Arc, time::Duration};

use synthia_memory::{store::MemoryStoreImpl, types::MemoryEvent};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::*;

fn create_test_store() -> MemoryStoreImpl {
    let temp_dir = tempfile::tempdir().unwrap();
    MemoryStoreImpl::new(temp_dir.path().to_path_buf())
}

#[tokio::test]
async fn test_memory_event_creation() {
    let event = MemoryEvent::session_end(
        "sess-1".to_string(),
        "Test summary".to_string(),
        vec!["tool1".to_string()],
        "success".to_string(),
    );
    assert!(matches!(event, MemoryEvent::SessionEnd { .. }));

    let event = MemoryEvent::tool_executed(
        "sess-1".to_string(),
        "read".to_string(),
        true,
    );
    assert!(matches!(event, MemoryEvent::ToolExecuted { .. }));

    let event =
        MemoryEvent::memory_flush("key".to_string(), "content".to_string());
    assert!(matches!(event, MemoryEvent::MemoryFlush { .. }));
}

#[tokio::test]
async fn test_background_task_handles_session_end() {
    let store = Arc::new(create_test_store());
    let shutdown_token = CancellationToken::new();

    let (handle, tx) = spawn(store.clone(), shutdown_token.clone(), 10);

    tx.send(MemoryEvent::session_end(
        "sess-1".to_string(),
        "Test session".to_string(),
        vec!["tool1".to_string(), "tool2".to_string()],
        "success".to_string(),
    ))
    .await
    .unwrap();

    tokio::time::sleep(Duration::from_millis(100)).await;
    shutdown_token.cancel();
    let _ = handle.await;
}

#[tokio::test]
async fn test_background_task_handles_tool_executed() {
    let store = Arc::new(create_test_store());
    let shutdown_token = CancellationToken::new();

    let (handle, tx) = spawn(store.clone(), shutdown_token.clone(), 10);

    tx.send(MemoryEvent::tool_executed(
        "sess-1".to_string(),
        "read_file".to_string(),
        true,
    ))
    .await
    .unwrap();

    tokio::time::sleep(Duration::from_millis(100)).await;
    shutdown_token.cancel();
    let _ = handle.await;
}

#[tokio::test]
async fn test_background_task_handles_memory_flush() {
    let store = Arc::new(create_test_store());
    let shutdown_token = CancellationToken::new();

    let (handle, tx) = spawn(store.clone(), shutdown_token.clone(), 10);

    tx.send(MemoryEvent::memory_flush(
        "test_key".to_string(),
        "test content".to_string(),
    ))
    .await
    .unwrap();

    tokio::time::sleep(Duration::from_millis(100)).await;
    shutdown_token.cancel();
    let _ = handle.await;

    let content = store.read_hot("test_key").await.unwrap();
    assert!(content.unwrap().contains("test content"));
}

#[tokio::test]
async fn test_graceful_shutdown() {
    let store = Arc::new(create_test_store());
    let shutdown_token = CancellationToken::new();

    let (handle, tx) = spawn(store, shutdown_token.clone(), 10);
    drop(tx);

    let result =
        graceful_shutdown(handle, shutdown_token, Duration::from_secs(1)).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_compaction_interval_configuration() {
    let store = Arc::new(create_test_store());
    let (_tx, rx) = mpsc::channel(10);
    let shutdown_token = CancellationToken::new();

    let task = MemoryBackgroundTask::new(store, rx, shutdown_token)
        .with_compaction_interval(Duration::from_secs(60));

    assert_eq!(task.compaction_interval, Duration::from_secs(60));
}

#[tokio::test]
async fn test_compact_specific_session() {
    let store = Arc::new(create_test_store());
    let (_tx, rx) = mpsc::channel(10);
    let shutdown_token = CancellationToken::new();

    let task = MemoryBackgroundTask::new(store, rx, shutdown_token);
    let result = task.compact_specific_session("test-session").await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_background_task_processes_multiple_events() {
    let store = Arc::new(create_test_store());
    let shutdown_token = CancellationToken::new();

    let (handle, tx) = spawn(store.clone(), shutdown_token.clone(), 10);

    tx.send(MemoryEvent::memory_flush(
        "key1".to_string(),
        "value1".to_string(),
    ))
    .await
    .unwrap();

    tx.send(MemoryEvent::memory_flush(
        "key2".to_string(),
        "value2".to_string(),
    ))
    .await
    .unwrap();

    tx.send(MemoryEvent::session_end(
        "sess-1".to_string(),
        "Multi-event session".to_string(),
        vec!["tool1".to_string()],
        "success".to_string(),
    ))
    .await
    .unwrap();

    tokio::time::sleep(Duration::from_millis(200)).await;
    shutdown_token.cancel();
    let _ = handle.await;

    let key1 = store.read_hot("key1").await.unwrap();
    assert!(key1.unwrap().contains("value1"));

    let key2 = store.read_hot("key2").await.unwrap();
    assert!(key2.unwrap().contains("value2"));
}

#[tokio::test]
async fn test_background_task_ignores_failed_tool_execution() {
    let store = Arc::new(create_test_store());
    let shutdown_token = CancellationToken::new();

    let (handle, tx) = spawn(store.clone(), shutdown_token.clone(), 10);

    tx.send(MemoryEvent::tool_executed(
        "sess-1".to_string(),
        "failing_tool".to_string(),
        false,
    ))
    .await
    .unwrap();

    tokio::time::sleep(Duration::from_millis(100)).await;
    shutdown_token.cancel();
    let _ = handle.await;

    let skills = store.load_episodic_jsonl("failing_tool").await.unwrap();
    assert!(skills.is_empty());
}

#[tokio::test]
async fn test_graceful_shutdown_with_timeout() {
    let store = Arc::new(create_test_store());
    let shutdown_token = CancellationToken::new();

    let (handle, tx) = spawn(store, shutdown_token.clone(), 10);
    drop(tx);

    let result =
        graceful_shutdown(handle, shutdown_token, Duration::from_millis(50))
            .await;
    assert!(result.is_ok());
}

#[test]
fn test_default_shutdown_timeout() {
    assert_eq!(default_shutdown_timeout(), Duration::from_secs(5));
}
