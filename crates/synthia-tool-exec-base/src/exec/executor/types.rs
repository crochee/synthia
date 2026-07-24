//! Internal storage types for [`super::Executor`].
//!
//! [`TaskItem`] is the "parked" form of a task in the
//! executor — the user provides a future-producing closure
//! via [`super::submit`], and the executor wraps it in a
//! `TaskItem` to keep in the parking lot. The scheduler
//! pops it out, calls the closure, and forwards the
//! result to the user via the `oneshot::Sender`.
//!
//! [`TaskFn`] is the boxed closure type used inside
//! [`TaskItem`].

use std::{future::Future, pin::Pin, sync::Arc, time::Duration};

use tokio::sync::oneshot;

use crate::exec::executor_types::ResourceUsage;

pub(super) type TaskFn<T> = Box<
    dyn FnOnce() -> Pin<
            Box<dyn Future<Output = Result<T, crate::exec::TaskError>> + Send>,
        > + Send,
>;

pub(super) struct TaskItem<T: Send + 'static> {
    pub(super) task_fn: TaskFn<T>,
    pub(super) timeout: Duration,
    pub(super) result_tx: oneshot::Sender<Result<T, crate::exec::TaskError>>,
    pub(super) resource_usage: Arc<parking_lot::Mutex<ResourceUsage>>,
    pub(super) cancelled: Arc<parking_lot::Mutex<bool>>,
}
