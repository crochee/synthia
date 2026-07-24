//! Private [`Scheduler<T>`] — the queue worker that pulls
//! [`super::types::TaskItem`] entries out of the priority
//! queue, acquires a semaphore permit, spawns the task, and
//! forwards the result back via the
//! `oneshot::Sender`.
//!
//! The scheduler is a single-actor concurrency primitive: the
//! worker task loops on a `Notify`, and the executor
//! notifies it whenever a new task is enqueued (see
//! [`super::submit`]) or shutdown is requested (see
//! [`super::lifecycle::Executor::shutdown`]).
//!
//! `Scheduler` is `pub(super)` — callers only ever interact
//! with the public [`super::Executor<T>`] which holds a
//! clone of `Arc<Scheduler<T>>` after
//! [`super::lifecycle::Executor::start`] returns.

use std::{collections::HashMap, sync::Arc};

use parking_lot::Mutex;
use priority_queue::PriorityQueue;
use tokio::{
    sync::{Notify, Semaphore},
    task::JoinHandle,
    time::timeout,
};
use tracing::warn;

use super::types::TaskItem;
use crate::exec::TaskError;

pub(super) struct Scheduler<T: Send + 'static> {
    pub(super) queue: Arc<Mutex<PriorityQueue<String, u8>>>,
    pub(super) task_store: Arc<Mutex<HashMap<String, TaskItem<T>>>>,
    pub(super) semaphore: Arc<Semaphore>,
    pub(super) shutting_down: Arc<Mutex<bool>>,
    pub(super) active_count: Arc<Mutex<usize>>,
    pub(super) notify: Arc<Notify>,
}

impl<T: Send + 'static> Scheduler<T> {
    pub(super) fn new(
        queue: Arc<Mutex<PriorityQueue<String, u8>>>,
        task_store: Arc<Mutex<HashMap<String, TaskItem<T>>>>,
        semaphore: Arc<Semaphore>,
        shutting_down: Arc<Mutex<bool>>,
        active_count: Arc<Mutex<usize>>,
        notify: Arc<Notify>,
    ) -> Self {
        Self {
            queue,
            task_store,
            semaphore,
            shutting_down,
            active_count,
            notify,
        }
    }

    pub(super) fn start(self: Arc<Self>) -> JoinHandle<()> {
        tokio::spawn(async move {
            loop {
                self.notify.notified().await;

                if *self.shutting_down.lock() {
                    break;
                }

                self.process_queue().await;
            }
        })
    }

    pub(super) async fn process_queue(&self) {
        loop {
            if *self.shutting_down.lock() {
                break;
            }

            let (task_id, priority) = {
                let mut q = self.queue.lock();
                match q.pop() {
                    Some((id, p)) => (id, p),
                    None => return,
                }
            };

            let permit = match self.semaphore.clone().try_acquire_owned() {
                Ok(p) => p,
                Err(_) => {
                    let mut q = self.queue.lock();
                    q.push(task_id, priority);
                    return;
                }
            };

            let task_item = {
                let mut store = self.task_store.lock();
                store.remove(&task_id)
            };

            if let Some(task_item) = task_item {
                {
                    let mut count = self.active_count.lock();
                    *count += 1;
                }

                let active_count = self.active_count.clone();
                let notify = self.notify.clone();
                let task_id_clone = task_id.clone();

                tokio::spawn(async move {
                    let task_timeout = task_item.timeout;
                    let result_tx = task_item.result_tx;
                    let resource_usage = task_item.resource_usage;

                    let task_fut = (task_item.task_fn)();

                    let result = match timeout(task_timeout, task_fut).await {
                        Ok(task_result) => task_result,
                        Err(_) => {
                            warn!(
                                "Task {} timed out after {:?}",
                                task_id_clone, task_timeout
                            );
                            Err(TaskError::Timeout(task_timeout))
                        }
                    };

                    {
                        let mut usage = resource_usage.lock();
                        *usage = std::mem::take(&mut *usage).mark_completed();
                    }

                    let _ = result_tx.send(result);

                    {
                        let mut count = active_count.lock();
                        *count = count.saturating_sub(1);
                    }

                    drop(permit);

                    notify.notify_one();
                });
            }
        }
    }
}
