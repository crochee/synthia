//! Constructors for [`Executor<T>`] and the struct itself.
//!
//! [`Executor<T>`] is the public, parameterised bounded-
//! concurrency task runner. Its 11 fields are all
//! `pub(super)` so the action submodules ([`super::submit`],
//! [`super::lifecycle`], [`super::query`]) can share state
//! directly. The struct + both constructors + the `Default`
//! impl all live here; the struct is small (no methods
//! defined on it directly) and the constructors are the
//! only places that produce an `Executor<T>`, so keeping
//! the data + the constructors in one file makes the
//! "what does an Executor look like and how do I get one"
//! question answerable by reading a single file.

use std::{collections::HashMap, sync::Arc};

use priority_queue::PriorityQueue;
use tokio::{sync::Semaphore, task::JoinHandle};

use super::{scheduler::Scheduler, types::TaskItem};
use crate::exec::executor_types::ExecutorConfig;

pub struct Executor<T: Send + 'static> {
    pub(super) config: ExecutorConfig,
    pub(super) semaphore: Arc<Semaphore>,
    pub(super) queue: Arc<parking_lot::Mutex<PriorityQueue<String, u8>>>,
    pub(super) shutting_down: Arc<parking_lot::Mutex<bool>>,
    pub(super) task_counter: Arc<parking_lot::Mutex<u64>>,
    pub(super) active_count: Arc<parking_lot::Mutex<usize>>,
    pub(super) task_store:
        Arc<parking_lot::Mutex<HashMap<String, TaskItem<T>>>>,
    pub(super) worker: Arc<parking_lot::Mutex<Option<JoinHandle<()>>>>,
    pub(super) notify: Arc<tokio::sync::Notify>,
    pub(super) scheduler: Arc<parking_lot::Mutex<Option<Arc<Scheduler<T>>>>>,
}

impl<T: Send + 'static> Executor<T> {
    /// Create a new executor with the default
    /// [`ExecutorConfig`].
    pub fn new() -> Self {
        Self::with_config(ExecutorConfig::default())
    }

    /// Create a new executor with a custom
    /// [`ExecutorConfig`].
    pub fn with_config(config: ExecutorConfig) -> Self {
        let max_concurrent = config.max_concurrent;

        tracing::info!(
            "Creating executor with max_concurrent={}, default_timeout={:?}, queue_capacity={}",
            max_concurrent,
            config.default_timeout,
            config.queue_capacity
        );

        Self {
            config,
            semaphore: Arc::new(Semaphore::new(max_concurrent)),
            queue: Arc::new(parking_lot::Mutex::new(PriorityQueue::new())),
            shutting_down: Arc::new(parking_lot::Mutex::new(false)),
            task_counter: Arc::new(parking_lot::Mutex::new(0)),
            active_count: Arc::new(parking_lot::Mutex::new(0)),
            task_store: Arc::new(parking_lot::Mutex::new(HashMap::new())),
            worker: Arc::new(parking_lot::Mutex::new(None)),
            notify: Arc::new(tokio::sync::Notify::new()),
            scheduler: Arc::new(parking_lot::Mutex::new(None)),
        }
    }
}

impl<T: Send + 'static> Default for Executor<T> {
    fn default() -> Self {
        Self::new()
    }
}
