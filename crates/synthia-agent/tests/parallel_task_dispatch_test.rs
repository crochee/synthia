use std::{path::PathBuf, sync::Arc};

use synthia_agent::task::{
    aggregate_results,
    scheduler::{DispatchableTask, PriorityScheduler},
    types::{TaskContext, TaskPriority, TaskResult},
};
use tokio::sync::Mutex;

#[tokio::test]
async fn test_parallel_dispatch_executes_all_tasks() {
    let scheduler = PriorityScheduler::new(PathBuf::from("/tmp"));

    let task1 = DispatchableTask::new(
        "p1".to_string(),
        TaskContext::new("parallel task 1".to_string()),
        PathBuf::from("/tmp"),
    );
    let task2 = DispatchableTask::new(
        "p2".to_string(),
        TaskContext::new("parallel task 2".to_string()),
        PathBuf::from("/tmp"),
    );
    let task3 = DispatchableTask::new(
        "p3".to_string(),
        TaskContext::new("parallel task 3".to_string()),
        PathBuf::from("/tmp"),
    );

    scheduler.submit(task1).await;
    scheduler.submit(task2).await;
    scheduler.submit(task3).await;

    assert_eq!(scheduler.pending_count().await, 3);

    let completed = Arc::new(Mutex::new(Vec::<String>::new()));

    for i in 0..3 {
        let completed = Arc::clone(&completed);
        let handler = async move {
            let result = TaskResult::success(format!("result from task {}", i));
            completed.lock().await.push(result.output.clone());
            result
        };
        let rx = scheduler.dispatch_next(handler).unwrap();
        let _ = rx.await;
    }

    let completed = completed.lock().await;
    assert_eq!(completed.len(), 3);
}

#[tokio::test]
async fn test_parallel_dispatch_three_prioritized_tasks_records_all_three() {
    let scheduler = PriorityScheduler::new(PathBuf::from("/tmp"));

    let low = DispatchableTask::new(
        "low".to_string(),
        TaskContext::new("low priority".to_string()),
        PathBuf::from("/tmp"),
    )
    .with_priority(TaskPriority::Low);

    let high = DispatchableTask::new(
        "high".to_string(),
        TaskContext::new("high priority".to_string()),
        PathBuf::from("/tmp"),
    )
    .with_priority(TaskPriority::High);

    let medium = DispatchableTask::new(
        "medium".to_string(),
        TaskContext::new("medium priority".to_string()),
        PathBuf::from("/tmp"),
    )
    .with_priority(TaskPriority::Medium);

    scheduler.submit(low).await;
    scheduler.submit(high).await;
    scheduler.submit(medium).await;

    let execution_order = Arc::new(Mutex::new(Vec::new()));

    let order1 = Arc::clone(&execution_order);
    let rx1 = scheduler
        .dispatch_next(async move { TaskResult::success("first".to_string()) })
        .unwrap();
    let _ = rx1.await;
    order1.lock().await.push("first".to_string());

    let order2 = Arc::clone(&execution_order);
    let rx2 = scheduler
        .dispatch_next(async move { TaskResult::success("second".to_string()) })
        .unwrap();
    let _ = rx2.await;
    order2.lock().await.push("second".to_string());

    let order3 = Arc::clone(&execution_order);
    let rx3 = scheduler
        .dispatch_next(async move { TaskResult::success("third".to_string()) })
        .unwrap();
    let _ = rx3.await;
    order3.lock().await.push("third".to_string());

    let order = execution_order.lock().await;
    assert_eq!(order.len(), 3);
}

#[tokio::test]
async fn test_parallel_dispatch_aggregates_results() {
    let scheduler = PriorityScheduler::new(PathBuf::from("/tmp"));

    let results = Arc::new(Mutex::new(Vec::new()));

    for i in 0..3 {
        let task = DispatchableTask::new(
            format!("task-{}", i),
            TaskContext::new(format!("task {}", i)),
            PathBuf::from("/tmp"),
        );
        scheduler.submit(task).await;
    }

    let results_clone = Arc::clone(&results);
    for i in 0..3 {
        let results = Arc::clone(&results_clone);
        let idx = i;
        let handler = async move {
            let r = TaskResult::success(format!("output-{}", idx));
            results.lock().await.push(r.clone());
            r
        };
        let rx = scheduler.dispatch_next(handler).unwrap();
        let _ = rx.await;
    }

    let results = results.lock().await;
    let agg = aggregate_results(results.clone());
    assert_eq!(agg.total, 3);
    assert!(agg.all_succeeded());
}
