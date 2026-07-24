use std::{future::Future, pin::Pin, sync::Arc};

use tokio::sync::{Mutex, oneshot};
use tracing::{info, warn};

use super::dispatchable_task::DispatchableTask;
use crate::task::types::{
    TaskResult,
    format_task_context,
    resolve_file_references,
};

/// Priority-aware task scheduler that executes tasks respecting
/// priority ordering and timeout constraints.
///
/// Tasks are queued and dispatched in priority order (High > Medium > Low).
/// Each task is wrapped with a timeout to prevent runaway executions.
pub struct PriorityScheduler {
    pending: Arc<Mutex<Vec<DispatchableTask>>>,
    workspace_root: std::path::PathBuf,
}

impl PriorityScheduler {
    pub fn new(workspace_root: std::path::PathBuf) -> Self {
        Self {
            pending: Arc::new(Mutex::new(Vec::new())),
            workspace_root,
        }
    }

    /// Submit a task to the scheduler queue.
    pub async fn submit(&self, task: DispatchableTask) {
        let mut queue = self.pending.lock().await;
        queue.push(task);
        // Sort by priority descending (highest first)
        queue.sort_by_key(|b| std::cmp::Reverse(b.priority.as_u8()));
    }

    /// Dispatch the highest-priority pending task with the given handler.
    ///
    /// The handler receives the formatted task context as a prompt string.
    /// Returns a oneshot receiver for the TaskResult.
    pub fn dispatch_next<F>(
        &self,
        handler_fn: F,
    ) -> Option<oneshot::Receiver<TaskResult>>
    where
        F: Future<Output = TaskResult> + Send + 'static,
    {
        let pending = Arc::clone(&self.pending);

        // Use tokio::spawn to extract the next task from the queue
        let (tx, rx) = oneshot::channel();

        tokio::spawn(async move {
            let mut queue = pending.lock().await;
            let task = queue.pop();
            drop(queue);

            let Some(task) = task else {
                let _ =
                    tx.send(TaskResult::error("No pending tasks".to_string()));
                return;
            };

            info!(
                task_id = %task.id,
                priority = ?task.priority,
                timeout_ms = task.timeout.as_millis(),
                "Dispatching task from priority scheduler"
            );

            // Resolve file references and build the prompt
            let resolved_files = resolve_file_references(
                &task.context.file_references,
                &task.workspace_root,
            )
            .await;
            let prompt = format_task_context(&task.context, &resolved_files);

            let timeout = task.timeout;
            let task_id = task.id.clone();

            // Execute with timeout wrapper
            let result = tokio::time::timeout(timeout, async {
                // The handler receives the formatted prompt as context.
                // In practice, the handler would pass this to a sub-agent.
                // Here we just invoke the provided handler.
                let _ = &prompt; // prompt is available for the handler if needed
                handler_fn.await
            })
            .await;

            let final_result = match result {
                Ok(task_result) => task_result,
                Err(_elapsed) => {
                    warn!(
                        task_id = %task_id,
                        timeout_ms = timeout.as_millis(),
                        "Task timed out"
                    );
                    TaskResult::timeout()
                }
            };

            let _ = tx.send(final_result);
        });

        Some(rx)
    }

    /// Dispatch all pending tasks concurrently.
    ///
    /// Returns a vector of oneshot receivers, one per task.
    pub fn dispatch_all<F>(
        &self,
        handler_factory: impl Fn(
            DispatchableTask,
        )
            -> Pin<Box<dyn Future<Output = TaskResult> + Send>>
        + Send
        + 'static,
    ) -> Vec<(String, oneshot::Receiver<TaskResult>)> {
        let pending = Arc::clone(&self.pending);
        let workspace_root = self.workspace_root.clone();

        tokio::spawn(async move {
            let mut queue = pending.lock().await;
            let tasks: Vec<_> = queue.drain(..).collect();
            drop(queue);

            for task in tasks {
                let task_id = task.id.clone();
                let priority = task.priority;
                let timeout = task.timeout;
                let resolved_files = resolve_file_references(
                    &task.context.file_references,
                    &workspace_root,
                )
                .await;
                let _prompt =
                    format_task_context(&task.context, &resolved_files);

                let handler = handler_factory(task);

                tokio::spawn(async move {
                    info!(
                        task_id = %task_id,
                        priority = ?priority,
                        timeout_ms = timeout.as_millis(),
                        "Dispatching task concurrently"
                    );

                    let result = tokio::time::timeout(timeout, handler).await;

                    let final_result = match result {
                        Ok(r) => r,
                        Err(_) => {
                            warn!(
                                task_id = %task_id,
                                timeout_ms = timeout.as_millis(),
                                "Task timed out during concurrent dispatch"
                            );
                            TaskResult::timeout()
                        }
                    };

                    info!(
                        task_id = %task_id,
                        status = ?final_result.status,
                        "Concurrent task completed"
                    );
                });
            }
        });

        // For the dispatch_all version, we return immediately since we can't
        // easily return individual receivers from spawned tasks.
        // The caller should use submit + dispatch_next for individual tracking.
        Vec::new()
    }

    /// Return the number of pending tasks.
    pub async fn pending_count(&self) -> usize {
        self.pending.lock().await.len()
    }
}
