use synthia_core::registry::Registry;

use super::*;
use crate::types::{Task, TaskStatus};

#[tokio::test]
async fn test_register_and_get() {
    let registry = TaskRegistry::new();
    let task = Task::new("t1".to_string(), 5);
    registry.register(task.clone()).await.unwrap();
    let got = registry.get("t1").await.unwrap();
    assert!(got.is_some());
    assert_eq!(got.unwrap().id, "t1");
}

#[tokio::test]
async fn test_register_duplicate() {
    let registry = TaskRegistry::new();
    let task = Task::new("t1".to_string(), 5);
    registry.register(task.clone()).await.unwrap();
    let result = registry.register(task).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_unregister() {
    let registry = TaskRegistry::new();
    let task = Task::new("t1".to_string(), 5);
    registry.register(task).await.unwrap();
    let result = registry.unregister("t1").await;
    assert!(result.is_ok());
    assert!(!registry.contains("t1"));
}

#[tokio::test]
async fn test_unregister_not_found() {
    let registry = TaskRegistry::new();
    let result = registry.unregister("nonexistent").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_get_not_found() {
    let registry = TaskRegistry::new();
    let result = registry.get("nonexistent").await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn test_list_all() {
    let registry = TaskRegistry::new();
    registry
        .register(Task::new("t1".to_string(), 5))
        .await
        .unwrap();
    registry
        .register(Task::new("t2".to_string(), 3))
        .await
        .unwrap();
    let items = registry.list(None).await.unwrap();
    assert_eq!(items.len(), 2);
}

#[tokio::test]
async fn test_list_with_filter() {
    let registry = TaskRegistry::new();
    let mut task = Task::new("t1".to_string(), 5);
    task.start();
    registry.register(task).await.unwrap();
    registry
        .register(Task::new("t2".to_string(), 3))
        .await
        .unwrap();

    let filter = TaskFilter {
        status: Some(TaskStatus::Running),
    };
    let items = registry.list(Some(filter)).await.unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].id, "t1");
}

#[test]
fn test_contains() {
    let registry = TaskRegistry::new();
    assert!(!registry.contains("t1"));
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        registry
            .register(Task::new("t1".to_string(), 5))
            .await
            .unwrap();
    });
    assert!(registry.contains("t1"));
}

#[test]
fn test_len_and_is_empty() {
    let registry = TaskRegistry::new();
    assert!(registry.is_empty());
    assert_eq!(registry.len(), 0);
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        registry
            .register(Task::new("t1".to_string(), 5))
            .await
            .unwrap();
    });
    assert!(!registry.is_empty());
    assert_eq!(registry.len(), 1);
}

#[tokio::test]
async fn test_add_dependency() {
    let registry = TaskRegistry::new();
    registry
        .register(Task::new("a".to_string(), 1))
        .await
        .unwrap();
    registry
        .register(Task::new("b".to_string(), 1))
        .await
        .unwrap();
    registry
        .add_dependency("a".to_string(), "b".to_string())
        .unwrap();
    assert_eq!(registry.get_dependencies("b"), vec!["a"]);
    assert_eq!(registry.get_dependents("a"), vec!["b"]);
}

#[tokio::test]
async fn test_add_dependency_cycle_prevented() {
    let registry = TaskRegistry::new();
    registry
        .register(Task::new("a".to_string(), 1))
        .await
        .unwrap();
    registry
        .register(Task::new("b".to_string(), 1))
        .await
        .unwrap();
    registry
        .register(Task::new("c".to_string(), 1))
        .await
        .unwrap();
    registry
        .add_dependency("a".to_string(), "b".to_string())
        .unwrap();
    registry
        .add_dependency("b".to_string(), "c".to_string())
        .unwrap();
    let result = registry.add_dependency("c".to_string(), "a".to_string());
    assert!(result.is_err());
}

#[tokio::test]
async fn test_topological_sort() {
    let registry = TaskRegistry::new();
    registry
        .register(Task::new("a".to_string(), 1))
        .await
        .unwrap();
    registry
        .register(Task::new("b".to_string(), 1))
        .await
        .unwrap();
    registry
        .register(Task::new("c".to_string(), 1))
        .await
        .unwrap();
    registry
        .add_dependency("a".to_string(), "b".to_string())
        .unwrap();
    registry
        .add_dependency("b".to_string(), "c".to_string())
        .unwrap();
    let sorted = registry.topological_sort().unwrap();
    let pos_a = sorted.iter().position(|x| x == "a").unwrap();
    let pos_b = sorted.iter().position(|x| x == "b").unwrap();
    let pos_c = sorted.iter().position(|x| x == "c").unwrap();
    assert!(pos_a < pos_b);
    assert!(pos_b < pos_c);
}

#[tokio::test]
async fn test_unregister_removes_topology() {
    let registry = TaskRegistry::new();
    registry
        .register(Task::new("a".to_string(), 1))
        .await
        .unwrap();
    registry
        .register(Task::new("b".to_string(), 1))
        .await
        .unwrap();
    registry
        .add_dependency("a".to_string(), "b".to_string())
        .unwrap();
    registry.unregister("b").await.unwrap();
    assert!(registry.get_dependents("a").is_empty());
}
