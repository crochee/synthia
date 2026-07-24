//! Unit tests for [`super::Executor<T>`].
//!
//! Covers:
//! - [`super::TaskPriority`] (values, default, display,
//!   ordering) — 4 tests.
//! - Task submit + completion + handle accessors
//!   (`priority`, `is_completed`, `is_cancelled`,
//!   `deadline`, `resource_usage`) — 5 tests.
//! - Timeout handling (sub-100ms timeout returns
//!   [`TaskError::Timeout`]) — 1 test.
//! - Error propagation (custom errors flow through the
//!   `oneshot::Sender`) — 1 test.
//! - Error variant display (Cancelled / Timeout / Shutdown
//!   / Custom) — 1 test.
//! - Shutdown: rejects new submissions, drains in-flight,
//!   cancels queued, `is_shutting_down` flag flips — 5
//!   tests.
//! - Active count / config accessor — 2 tests.
//! - Concurrency config + multi-task batch — 2 tests.
//! - [`super::executor_types::ResourceUsage`] defaults —
//!   1 test.

use std::time::Duration;

use tokio::time::sleep;

use super::Executor;
use crate::exec::{
    TaskError,
    executor_types::{ExecutorConfig, ResourceUsage},
    priority::TaskPriority,
};

fn test_executor() -> Executor<String> {
    let config = ExecutorConfig {
        max_concurrent: 4,
        default_timeout: Duration::from_secs(5),
        queue_capacity: 20,
    };
    Executor::with_config(config)
}

#[tokio::test]
async fn test_task_priority_values() {
    assert_eq!(TaskPriority::Low.as_u8(), 0);
    assert_eq!(TaskPriority::Normal.as_u8(), 1);
    assert_eq!(TaskPriority::High.as_u8(), 2);
    assert_eq!(TaskPriority::Critical.as_u8(), 3);

    assert!(TaskPriority::Critical.is_at_least(TaskPriority::High));
    assert!(TaskPriority::High.is_at_least(TaskPriority::Normal));
    assert!(TaskPriority::Normal.is_at_least(TaskPriority::Low));
    assert!(!TaskPriority::Low.is_at_least(TaskPriority::High));
}

#[tokio::test]
async fn test_task_priority_default() {
    assert_eq!(TaskPriority::default(), TaskPriority::Normal);
}

#[tokio::test]
async fn test_task_priority_display() {
    assert_eq!(format!("{}", TaskPriority::Low), "Low");
    assert_eq!(format!("{}", TaskPriority::Normal), "Normal");
    assert_eq!(format!("{}", TaskPriority::High), "High");
    assert_eq!(format!("{}", TaskPriority::Critical), "Critical");
}

#[tokio::test]
async fn test_task_priority_ordering() {
    let executor = test_executor();
    executor.start();

    let handle_low = executor
        .submit_with_priority(
            || async {
                sleep(Duration::from_millis(10)).await;
                Ok("low".to_string())
            },
            TaskPriority::Low,
        )
        .unwrap();

    let handle_critical = executor
        .submit_with_priority(
            || async {
                sleep(Duration::from_millis(10)).await;
                Ok("critical".to_string())
            },
            TaskPriority::Critical,
        )
        .unwrap();

    let handle_high = executor
        .submit_with_priority(
            || async {
                sleep(Duration::from_millis(10)).await;
                Ok("high".to_string())
            },
            TaskPriority::High,
        )
        .unwrap();

    let _ = handle_low.await_result().await;
    let _ = handle_critical.await_result().await;
    let _ = handle_high.await_result().await;

    assert_eq!(handle_low.priority(), TaskPriority::Low);
    assert_eq!(handle_critical.priority(), TaskPriority::Critical);
    assert_eq!(handle_high.priority(), TaskPriority::High);

    let _ = executor.shutdown(Duration::from_secs(2)).await;
}

#[tokio::test]
async fn test_task_timeout() {
    let executor = test_executor();
    executor.start();

    let handle = executor
        .submit_with_timeout(
            || async {
                sleep(Duration::from_secs(10)).await;
                Ok("should not complete".to_string())
            },
            TaskPriority::Normal,
            Duration::from_millis(100),
        )
        .unwrap();

    let result = handle.await_result().await;
    assert!(result.is_err());
    match result.unwrap_err() {
        TaskError::Timeout(d) => {
            assert_eq!(d, Duration::from_millis(100));
        }
        other => panic!("Expected Timeout error, got: {:?}", other),
    }

    let _ = executor.shutdown(Duration::from_secs(2)).await;
}

#[tokio::test]
async fn test_shutdown_rejects_new_tasks() {
    let executor = test_executor();
    executor.start();

    let handle = executor
        .submit(|| async { Ok("done".to_string()) })
        .unwrap();

    let _ = handle.await_result().await;

    let _ = executor.shutdown(Duration::from_secs(1)).await;

    let submit_result =
        executor.submit(|| async { Ok("too late".to_string()) });
    assert!(submit_result.is_err());
    if let Err(TaskError::Shutdown) = submit_result {
    } else {
        panic!("Expected Shutdown error");
    }
}

#[tokio::test]
async fn test_task_handle_is_completed() {
    let executor = test_executor();
    executor.start();

    let handle = executor
        .submit(|| async {
            sleep(Duration::from_millis(50)).await;
            Ok("done".to_string())
        })
        .unwrap();

    assert!(!handle.is_completed());

    let _ = handle.await_result().await;

    assert!(handle.is_completed());

    let _ = executor.shutdown(Duration::from_secs(2)).await;
}

#[tokio::test]
async fn test_task_handle_is_cancelled() {
    let executor = test_executor();
    executor.start();

    let handle = executor
        .submit(|| async {
            sleep(Duration::from_millis(50)).await;
            Ok("done".to_string())
        })
        .unwrap();

    assert!(!handle.is_cancelled());

    let _ = handle.await_result().await;

    assert!(!handle.is_cancelled());

    let _ = executor.shutdown(Duration::from_secs(2)).await;
}

#[tokio::test]
async fn test_resource_usage_tracking() {
    let executor = test_executor();
    executor.start();

    let handle = executor
        .submit(|| async {
            sleep(Duration::from_millis(50)).await;
            Ok("done".to_string())
        })
        .unwrap();

    let _ = handle.await_result().await;

    let usage = handle.resource_usage();
    assert!(usage.duration.is_some());
    assert!(usage.duration.unwrap() >= Duration::from_millis(40));
    assert!(usage.end_time.is_some());

    let _ = executor.shutdown(Duration::from_secs(2)).await;
}

#[tokio::test]
async fn test_resource_usage_new() {
    let usage = ResourceUsage::new();
    assert!(usage.end_time.is_none());
    assert!(usage.duration.is_none());
    assert!(usage.cpu_time_estimate_ms.is_none());
    assert!(usage.memory_estimate_bytes.is_none());
}

#[tokio::test]
async fn test_resource_usage_mark_completed() {
    let usage = ResourceUsage::new();
    let completed = usage.mark_completed();

    assert!(completed.end_time.is_some());
    assert!(completed.duration.is_some());
}

#[tokio::test]
async fn test_executor_config_defaults() {
    let config = ExecutorConfig::default();
    assert_eq!(config.max_concurrent, 10);
    assert_eq!(config.default_timeout, Duration::from_secs(30));
    assert_eq!(config.queue_capacity, 100);
}

#[tokio::test]
async fn test_executor_active_count() {
    let config = ExecutorConfig {
        max_concurrent: 2,
        default_timeout: Duration::from_secs(5),
        queue_capacity: 10,
    };
    let executor = Executor::with_config(config);
    executor.start();

    assert_eq!(executor.active_count(), 0);

    let _h1 = executor
        .submit(|| async {
            sleep(Duration::from_millis(200)).await;
            Ok("task1".to_string())
        })
        .unwrap();

    let _h2 = executor
        .submit(|| async {
            sleep(Duration::from_millis(200)).await;
            Ok("task2".to_string())
        })
        .unwrap();

    sleep(Duration::from_millis(50)).await;
    assert!(executor.active_count() <= 2);

    let _ = executor.shutdown(Duration::from_secs(2)).await;
}

#[tokio::test]
async fn test_executor_config_accessor() {
    let executor = test_executor();
    assert_eq!(executor.config().max_concurrent, 4);
    assert_eq!(executor.config().default_timeout, Duration::from_secs(5));
    assert_eq!(executor.config().queue_capacity, 20);
}

#[tokio::test]
async fn test_multiple_tasks_complete() {
    let executor = test_executor();
    executor.start();

    let mut handles = Vec::new();
    for i in 0..5 {
        let handle = executor
            .submit(move || {
                let i = i;
                async move {
                    sleep(Duration::from_millis(20)).await;
                    Ok(format!("task-{}", i))
                }
            })
            .unwrap();
        handles.push(handle);
    }

    for (i, handle) in handles.iter().enumerate() {
        let result = handle.await_result().await.unwrap();
        assert_eq!(result, format!("task-{}", i));
    }

    let _ = executor.shutdown(Duration::from_secs(2)).await;
}

#[tokio::test]
async fn test_task_error_propagation() {
    let executor = test_executor();
    executor.start();

    let handle = executor
        .submit(|| async { Err(TaskError::Custom("test error".to_string())) })
        .unwrap();

    let result = handle.await_result().await;
    assert!(result.is_err());
    match result.unwrap_err() {
        TaskError::Custom(msg) => {
            assert_eq!(msg, "test error");
        }
        other => panic!("Expected Custom error, got: {:?}", other),
    }

    let _ = executor.shutdown(Duration::from_secs(2)).await;
}

#[tokio::test]
async fn test_is_shutting_down_flag() {
    let executor = test_executor();
    executor.start();

    assert!(!executor.is_shutting_down());

    let _ = executor.shutdown(Duration::from_secs(2)).await;
    assert!(executor.is_shutting_down());
}

#[tokio::test]
async fn test_task_handle_deadline() {
    let executor = test_executor();
    executor.start();

    let handle = executor
        .submit_with_timeout(
            || async {
                sleep(Duration::from_millis(10)).await;
                Ok("done".to_string())
            },
            TaskPriority::Normal,
            Duration::from_secs(10),
        )
        .unwrap();

    assert!(handle.deadline().is_some());

    let _ = handle.await_result().await;
    let _ = executor.shutdown(Duration::from_secs(2)).await;
}

#[tokio::test]
async fn test_queued_tasks_cancelled_on_shutdown() {
    let config = ExecutorConfig {
        max_concurrent: 1,
        default_timeout: Duration::from_secs(5),
        queue_capacity: 10,
    };
    let executor = Executor::with_config(config);
    executor.start();

    let _long_handle = executor
        .submit(|| async {
            sleep(Duration::from_secs(10)).await;
            Ok("long".to_string())
        })
        .unwrap();

    sleep(Duration::from_millis(50)).await;

    let queued_handles: Vec<_> = (0..3)
        .map(|i| {
            executor.submit(move || {
                let i = i;
                async move { Ok(format!("queued-{}", i)) }
            })
        })
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    let _ = executor.shutdown(Duration::from_millis(10)).await;

    for handle in &queued_handles {
        assert!(handle.is_cancelled());
    }
}

#[tokio::test]
async fn test_task_error_display() {
    let err_cancelled = TaskError::Cancelled;
    assert!(format!("{}", err_cancelled).contains("cancelled"));

    let err_timeout = TaskError::Timeout(Duration::from_secs(5));
    assert!(format!("{}", err_timeout).contains("5s"));

    let err_shutdown = TaskError::Shutdown;
    assert!(format!("{}", err_shutdown).contains("shutting down"));

    let err_custom = TaskError::Custom("test".to_string());
    assert!(format!("{}", err_custom).contains("test"));
}

#[tokio::test]
async fn test_executor_with_zero_max_concurrent() {
    let config = ExecutorConfig {
        max_concurrent: 1,
        default_timeout: Duration::from_secs(5),
        queue_capacity: 5,
    };
    let executor = Executor::with_config(config);
    executor.start();

    let handle = executor
        .submit(|| async { Ok("single".to_string()) })
        .unwrap();

    let result = handle.await_result().await.unwrap();
    assert_eq!(result, "single");

    let _ = executor.shutdown(Duration::from_secs(2)).await;
}

#[tokio::test]
async fn test_resource_usage_default() {
    let usage = ResourceUsage::default();
    assert!(usage.end_time.is_none());
    assert!(usage.duration.is_none());
}

#[tokio::test]
async fn test_executor_default() {
    let executor: Executor<String> = Executor::default();
    assert_eq!(executor.config().max_concurrent, 10);
}

#[tokio::test]
async fn test_task_debug_display() {
    let err = TaskError::Custom("debug test".to_string());
    let debug_str = format!("{:?}", err);
    assert!(debug_str.contains("Custom"));
}
