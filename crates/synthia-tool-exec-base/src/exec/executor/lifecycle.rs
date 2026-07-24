//! Lifecycle — [`super::lifecycle::Executor::start`] and
//! [`super::lifecycle::Executor::shutdown`].
//!
//! `start` constructs the [`super::scheduler::Scheduler`]
//! (a clone of every shared state field on the executor),
//! spawns its worker task, and stores the join handle +
//! scheduler clone on the executor.
//!
//! `shutdown` flips the `shutting_down` flag, notifies the
//! scheduler to drain, waits up to `drain_timeout` for
//! in-flight tasks to finish, and finally cancels anything
//! still queued (sending [`TaskError::Shutdown`] to the
//! user's `oneshot` receiver).

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use tracing::{info, warn};

use super::{construct::Executor, scheduler::Scheduler};
use crate::exec::TaskError;

impl<T: Send + 'static> Executor<T> {
    /// Start the scheduler worker. Idempotent: re-calling
    /// on an already-started executor is a no-op (the
    /// `worker: Arc<Mutex<Option<JoinHandle<()>>>>` stays
    /// populated).
    pub fn start(&self) {
        let scheduler = Arc::new(Scheduler::new(
            self.queue.clone(),
            self.task_store.clone(),
            self.semaphore.clone(),
            self.shutting_down.clone(),
            self.active_count.clone(),
            self.notify.clone(),
        ));

        let worker = scheduler.clone().start();

        *self.worker.lock() = Some(worker);
        *self.scheduler.lock() = Some(scheduler);
    }

    /// Drain in-flight tasks, then cancel anything still
    /// queued. Returns `Ok(())` if all in-flight tasks
    /// drained before `drain_timeout` elapsed, `Err(())` if
    /// the drain timed out and queued tasks were cancelled.
    pub async fn shutdown(&self, drain_timeout: Duration) -> Result<(), ()> {
        info!(
            "Initiating executor shutdown with drain_timeout={:?}",
            drain_timeout
        );

        {
            let mut shutting_down = self.shutting_down.lock();
            *shutting_down = true;
        }

        self.notify.notify_one();

        let start = Instant::now();

        loop {
            let active = *self.active_count.lock();

            if active == 0 {
                info!("All tasks completed during shutdown");
                return Ok(());
            }

            if start.elapsed() >= drain_timeout {
                warn!("Drain timeout reached, {} tasks still running", active);
                break;
            }

            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        {
            let mut queue = self.queue.lock();
            let mut store = self.task_store.lock();

            while let Some((task_id, _priority)) = queue.pop() {
                if let Some(task) = store.remove(&task_id) {
                    *task.cancelled.lock() = true;
                    let _ = task.result_tx.send(Err(TaskError::Shutdown));
                }
            }
        }

        info!("Executor shutdown complete");
        Err(())
    }
}
