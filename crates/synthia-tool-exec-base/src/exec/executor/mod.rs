//! `Executor<T>` — bounded-concurrency task runner with
//! priority queue, per-task timeout, drain-on-shutdown, and
//! per-task [`ResourceUsage`] tracking.
//!
//! Submodule layout:
//!
//! - [`types`]: the [`TaskItem`] struct (internal storage
//!   unit; never escapes the executor) + the `TaskFn<T>`
//!   closure type alias.
//! - [`scheduler`]: the private [`Scheduler<T>`] that drives
//!   the queue forward. Owns the spawned worker task; the
//!   `Executor` only holds a clone of the
//!   `Arc<Scheduler<T>>` after [`super::lifecycle::Executor::start`]
//!   returns. Three methods: `new` + `start` +
//!   `process_queue`.
//! - [`executor`]: the public [`Executor<T>`] struct (11
//!   fields, all `pub(super)` so the action submodules can
//!   share state).
//! - [`construct`]: the 3 constructors —
//!   [`super::construct::Executor::new`],
//!   [`super::construct::Executor::with_config`], and the
//!   `Default` impl.
//! - [`submit`]: the 3 task-submission entry points —
//!   [`super::submit::Executor::submit`],
//!   [`super::submit::Executor::submit_with_priority`], and
//!   [`super::submit::Executor::submit_with_timeout`]
//!   (the 90-line core of the public API).
//! - [`lifecycle`]: the 2 lifecycle methods —
//!   [`super::lifecycle::Executor::start`] (spawns the
//!   scheduler worker) and
//!   [`super::lifecycle::Executor::shutdown`] (drains
//!   in-flight tasks then cancels the rest).
//! - [`query`]: the 4 read-only accessors —
//!   [`super::query::Executor::queue_len`],
//!   [`super::query::Executor::active_count`],
//!   [`super::query::Executor::is_shutting_down`], and
//!   [`super::query::Executor::config`].
//!
//! Unit tests live in [`tests`].

mod construct;
mod lifecycle;
mod query;
mod scheduler;
mod submit;
#[cfg(test)]
mod tests;
mod types;

pub use construct::Executor;
