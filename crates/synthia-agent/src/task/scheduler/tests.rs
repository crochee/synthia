use std::{path::PathBuf, time::Duration};

use super::*;
use crate::task::types::{
    DEFAULT_TASK_TIMEOUT,
    TaskContext,
    TaskPriority,
    TaskResult,
    TaskStatus,
};

#[test]
fn test_dispatchable_task_default_priority() {
    let task = DispatchableTask::new(
        "t1".to_string(),
        TaskContext::new("test".to_string()),
        PathBuf::from("/tmp"),
    );
    assert_eq!(task.priority, TaskPriority::Medium);
    assert_eq!(task.timeout, DEFAULT_TASK_TIMEOUT);
}

#[test]
fn test_dispatchable_task_custom_priority_and_timeout() {
    let task = DispatchableTask::new(
        "t1".to_string(),
        TaskContext::new("test".to_string()),
        PathBuf::from("/tmp"),
    )
    .with_priority(TaskPriority::High)
    .with_timeout(Duration::from_secs(60));

    assert_eq!(task.priority, TaskPriority::High);
    assert_eq!(task.timeout, Duration::from_secs(60));
}

#[tokio::test]
async fn test_priority_scheduler_submit_and_count() {
    let scheduler = PriorityScheduler::new(PathBuf::from("/tmp"));

    assert_eq!(scheduler.pending_count().await, 0);

    let task1 = DispatchableTask::new(
        "t1".to_string(),
        TaskContext::new("low".to_string()),
        PathBuf::from("/tmp"),
    )
    .with_priority(TaskPriority::Low);

    let task2 = DispatchableTask::new(
        "t2".to_string(),
        TaskContext::new("high".to_string()),
        PathBuf::from("/tmp"),
    )
    .with_priority(TaskPriority::High);

    scheduler.submit(task1).await;
    scheduler.submit(task2).await;

    assert_eq!(scheduler.pending_count().await, 2);
}

#[tokio::test]
async fn test_execute_with_timeout_success() {
    let result = execute_with_timeout(
        async { TaskResult::success("done".to_string()) },
        Duration::from_secs(5),
    )
    .await;

    assert!(result.is_success());
}

#[tokio::test]
async fn test_execute_with_timeout_timeout() {
    let result = execute_with_timeout(
        async {
            tokio::time::sleep(Duration::from_secs(10)).await;
            TaskResult::success("never".to_string())
        },
        Duration::from_millis(50),
    )
    .await;

    assert_eq!(result.status, TaskStatus::Timeout);
}

#[test]
fn test_aggregate_results_all_success() {
    let results = vec![
        TaskResult::success("a".to_string()),
        TaskResult::success("b".to_string()),
    ];
    let agg = aggregate_results(results);
    assert_eq!(agg.total, 2);
    assert_eq!(agg.success_count, 2);
    assert!(agg.all_succeeded());
    assert!(!agg.any_failed());
}

#[test]
fn test_aggregate_results_mixed() {
    let results = vec![
        TaskResult::success("a".to_string()),
        TaskResult::error("fail".to_string()),
        TaskResult::timeout(),
    ];
    let agg = aggregate_results(results);
    assert_eq!(agg.total, 3);
    assert_eq!(agg.success_count, 1);
    assert_eq!(agg.error_count, 1);
    assert_eq!(agg.timeout_count, 1);
    assert!(!agg.all_succeeded());
    assert!(agg.any_failed());
}

#[test]
fn test_aggregate_results_artifacts() {
    let results = vec![
        TaskResult::success("a".to_string())
            .with_artifacts(vec!["out1.txt".to_string()]),
        TaskResult::success("b".to_string())
            .with_artifacts(vec!["out2.txt".to_string()]),
    ];
    let agg = aggregate_results(results);
    assert_eq!(agg.artifacts.len(), 2);
    assert!(agg.combined_output.contains("---"));
}
