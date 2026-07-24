//! Task submission — the public API for enqueuing work.
//!
//! The 3 entry points form a layered stack:
//!
//! - [`super::submit::Executor::submit`] — the
//!   simplest form (just a closure, default priority +
//!   timeout).
//! - [`super::submit::Executor::submit_with_priority`] —
//!   caller controls priority but uses the default
//!   timeout.
//! - [`super::submit::Executor::submit_with_timeout`] —
//!   the full form: caller controls both priority and
//!   per-task timeout. This is the 90-line core of the
//!   public API (queue capacity check, ID generation, store
//!   insert, queue push, scheduler notify).

use std::{
    future::Future,
    sync::Arc,
    time::{Duration, Instant},
};

use parking_lot::Mutex;
use tokio::sync::oneshot;
use tracing::{debug, error, warn};

use super::{construct::Executor, types::TaskItem};
use crate::exec::{
    TaskError,
    TaskHandle,
    executor_types::ResourceUsage,
    priority::TaskPriority,
};

impl<T: Send + 'static> Executor<T> {
    pub fn submit<F, Fut>(&self, task_fn: F) -> Result<TaskHandle<T>, TaskError>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = Result<T, TaskError>> + Send + 'static,
    {
        self.submit_with_priority(task_fn, TaskPriority::Normal)
    }

    pub fn submit_with_priority<F, Fut>(
        &self,
        task_fn: F,
        priority: TaskPriority,
    ) -> Result<TaskHandle<T>, TaskError>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = Result<T, TaskError>> + Send + 'static,
    {
        self.submit_with_timeout(task_fn, priority, self.config.default_timeout)
    }

    pub fn submit_with_timeout<F, Fut>(
        &self,
        task_fn: F,
        priority: TaskPriority,
        task_timeout: Duration,
    ) -> Result<TaskHandle<T>, TaskError>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = Result<T, TaskError>> + Send + 'static,
    {
        if *self.shutting_down.lock() {
            warn!("Task submission rejected: executor is shutting down");
            return Err(TaskError::Shutdown);
        }

        {
            let queue = self.queue.lock();
            if queue.len() >= self.config.queue_capacity {
                error!(
                    "Task queue is full (capacity: {})",
                    self.config.queue_capacity
                );
                return Err(TaskError::Custom(
                    "Task queue is full".to_string(),
                ));
            }
        }

        let task_id = {
            let mut counter = self.task_counter.lock();
            *counter += 1;
            format!("task-{}", counter)
        };

        let (result_tx, result_rx) = oneshot::channel();
        let resource_usage = Arc::new(Mutex::new(ResourceUsage::new()));
        let cancelled = Arc::new(Mutex::new(false));

        let deadline = Some(Instant::now() + task_timeout);

        let handle = TaskHandle::new(
            result_rx,
            resource_usage.clone(),
            cancelled.clone(),
            priority,
            deadline,
        );

        let task_item = TaskItem {
            task_fn: Box::new(move || Box::pin(task_fn())),
            timeout: task_timeout,
            result_tx,
            resource_usage,
            cancelled,
        };

        {
            let mut store = self.task_store.lock();
            store.insert(task_id.clone(), task_item);
        }

        {
            let mut queue = self.queue.lock();
            queue.push(task_id.clone(), priority.as_u8());
        }

        debug!("Task {} queued with priority={}", task_id, priority.as_u8());

        self.notify.notify_one();

        Ok(handle)
    }
}
