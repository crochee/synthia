//! The default [`Spawner`](synthia_core::spawn::Spawner) for the agent
//! loop.
//!
//! [`ReActAgent::run`](crate::ReActAgent) detaches one task per run so
//! the loop keeps running to completion even if the caller stops
//! polling the event stream (a dropped SSE connection must not cancel
//! a turn mid-write). That detach used to be a hard `tokio::spawn`,
//! which meant "your executor must be tokio or this panics".
//! [`ReActAgent::with_spawner`](crate::agent::ReActAgent::with_spawner) lets a

use synthia_core::spawn::{BoxFuture, Spawner};

/// Detached work on the ambient tokio runtime.
///
/// `synthia-harness` already depends on tokio for its own internals
/// (channels, timers), so this is the zero-cost default. A consumer on
/// `smol` / `async-std` / a bare `futures` executor installs their own
/// spawner and never enters this type.
#[derive(Debug, Default)]
pub struct TokioSpawner;

impl Spawner for TokioSpawner {
    fn spawn(&self, task: BoxFuture<()>) {
        // The `JoinHandle` is dropped on purpose: the caller has no way
        // to observe it, and dropping it does not cancel the task.
        drop(tokio::spawn(task));
    }

    fn spawn_blocking(&self, f: Box<dyn FnOnce() + Send + 'static>) {
        drop(tokio::task::spawn_blocking(f));
    }
}
