//! The default [`Spawner`](synthia_core::spawn::Spawner) for tool
//! dispatch.
//!
//! [`ToolRegistry::run_stream`](crate::ToolRegistry::run_stream) runs
//! one detached task per tool call so a call's progress items stream
//! out while its siblings are still running. That detach used to be a
//! hard `tokio::spawn`; it now goes through the registry's spawner,
//! which defaults to [`TokioSpawner`] and is replaceable via
//! [`ToolRegistry::with_spawner`](crate::ToolRegistry::with_spawner).
//!
//! Note this is about *dispatch*. The plugin crates that run
//! processes or touch the disk (`synthia-tool-read` / `-write` /
//! `-shell` / `-todo` / `-web` / `-task`) use `tokio::fs` and
//! `tokio::process` and are tokio-bound by construction — but they
//! are *separate crates*, so a consumer on another runtime simply
//! does not depend on them and supplies their own `Tool`
//! implementations with the same registry.

use synthia_core::spawn::{BoxFuture, Spawner};

/// Detached work on the ambient tokio runtime.
#[derive(Debug, Default)]
pub struct TokioSpawner;

impl Spawner for TokioSpawner {
    fn spawn(&self, task: BoxFuture<()>) {
        drop(tokio::spawn(task));
    }

    fn spawn_blocking(&self, f: Box<dyn FnOnce() + Send + 'static>) {
        drop(tokio::task::spawn_blocking(f));
    }
}
