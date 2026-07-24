use std::{path::PathBuf, sync::Arc, time::Duration};

use synthia_agent::task::{
    scheduler::{DispatchableTask, PriorityScheduler},
    types::{TaskContext, TaskPriority, TaskResult},
};
use tokio::sync::Mutex;

#[tokio::test]
async fn test_dispatch_returns_tasks_in_submission_order() {
    let scheduler = PriorityScheduler::new(PathBuf::from("/tmp"));

    for task_id in ["task-a", "task-b", "task-c"] {
        let task = DispatchableTask::new(
            task_id.to_string(),
            TaskContext::new(format!("Task {}", task_id)),
            PathBuf::from("/tmp"),
        );
        scheduler.submit(task).await;
    }

    let execution_order = Arc::new(Mutex::new(Vec::new()));

    let order = Arc::clone(&execution_order);
    let rx_a = scheduler
        .dispatch_next(async move { TaskResult::success("A done".to_string()) })
        .unwrap();
    let _ = rx_a.await;
    order.lock().await.push("task-a".to_string());

    let order = Arc::clone(&execution_order);
    let rx_b = scheduler
        .dispatch_next(async move { TaskResult::success("B done".to_string()) })
        .unwrap();
    let _ = rx_b.await;
    order.lock().await.push("task-b".to_string());

    let order = Arc::clone(&execution_order);
    let rx_c = scheduler
        .dispatch_next(async move { TaskResult::success("C done".to_string()) })
        .unwrap();
    let _ = rx_c.await;
    order.lock().await.push("task-c".to_string());

    let order = execution_order.lock().await;
    assert_eq!(order.len(), 3);
    assert_eq!(order[0], "task-a");
    assert_eq!(order[1], "task-b");
    assert_eq!(order[2], "task-c");
}

#[tokio::test]
async fn test_sequential_dependency_respects_predecessors() {
    let scheduler = PriorityScheduler::new(PathBuf::from("/tmp"));

    let task1 = DispatchableTask::new(
        "build".to_string(),
        TaskContext::new("Build step".to_string()),
        PathBuf::from("/tmp"),
    )
    .with_priority(TaskPriority::High);

    let task2 = DispatchableTask::new(
        "test".to_string(),
        TaskContext::new("Test step".to_string()),
        PathBuf::from("/tmp"),
    )
    .with_priority(TaskPriority::High);

    scheduler.submit(task1).await;
    scheduler.submit(task2).await;

    let build_result = scheduler
        .dispatch_next(async move {
            TaskResult::success("build output".to_string())
        })
        .unwrap();

    let build_output = build_result.await.unwrap();
    assert!(build_output.is_success());
    assert!(build_output.output.contains("build"));

    let test_result = scheduler
        .dispatch_next(
            async move { TaskResult::success("test output".to_string()) },
        )
        .unwrap();

    let test_output = test_result.await.unwrap();
    assert!(test_output.is_success());
    assert!(test_output.output.contains("test"));
}

#[tokio::test]
async fn test_sequential_chain_with_failure() {
    let scheduler = PriorityScheduler::new(PathBuf::from("/tmp"));

    for i in 0..3 {
        let task = DispatchableTask::new(
            format!("chain-{}", i),
            TaskContext::new(format!("Chain step {}", i)),
            PathBuf::from("/tmp"),
        );
        scheduler.submit(task).await;
    }

    let first = scheduler
        .dispatch_next(
            async move { TaskResult::error("step 0 failed".to_string()) },
        )
        .unwrap();

    let result = first.await.unwrap();
    assert!(!result.is_success());
    assert_eq!(result.output, "step 0 failed");
}

#[tokio::test]
async fn test_sequential_dependency_with_timeout() {
    let scheduler = PriorityScheduler::new(PathBuf::from("/tmp"));

    let task = DispatchableTask::new(
        "slow-task".to_string(),
        TaskContext::new("A slow task".to_string()),
        PathBuf::from("/tmp"),
    )
    .with_timeout(Duration::from_millis(50))
    .with_priority(TaskPriority::Medium);

    scheduler.submit(task).await;

    let rx = scheduler
        .dispatch_next(async {
            tokio::time::sleep(Duration::from_secs(10)).await;
            TaskResult::success("should not reach here".to_string())
        })
        .unwrap();

    let result = rx.await.unwrap();
    assert!(!result.is_success());
}
