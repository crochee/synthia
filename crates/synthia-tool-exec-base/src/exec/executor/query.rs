//! Read-only accessors for [`super::Executor<T>`].
//!
//! All 4 methods are pure read operations on internal
//! state — they take `&self`, never mutate, and never
//! fail. Used by callers (and tests) for observability
//! and for [`super::lifecycle::Executor::shutdown`]'s
//! drain loop.

use super::construct::Executor;
use crate::exec::executor_types::ExecutorConfig;

impl<T: Send + 'static> Executor<T> {
    /// Number of tasks currently in the priority queue
    /// (not yet picked up by the scheduler).
    pub fn queue_len(&self) -> usize {
        self.queue.lock().len()
    }

    /// Number of tasks currently executing (holding a
    /// semaphore permit).
    pub fn active_count(&self) -> usize {
        *self.active_count.lock()
    }

    /// Whether the executor has begun shutting down.
    /// Once `true`, new submissions are rejected with
    /// [`super::TaskError::Shutdown`].
    pub fn is_shutting_down(&self) -> bool {
        *self.shutting_down.lock()
    }

    /// Borrow the executor config (no clone — the config
    /// is held by value inside the executor, so this is
    /// just a reference).
    pub fn config(&self) -> &ExecutorConfig {
        &self.config
    }
}
