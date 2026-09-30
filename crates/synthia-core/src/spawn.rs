//! Runtime-neutral task spawning.
//!
//! The agent loop, the tool dispatch, and the session bookkeeping all
//! need to *detach* work: run one future to completion on an executor
//! the caller never polls again. Doing that with `tokio::spawn`
//! compiles only against tokio and panics at runtime ("there is no
//! reactor running") under any other executor — the coupling that
//! stops this framework from being a library for consumers who did not
//! choose tokio.
//!
//! [`Spawner`] is that one operation, with no runtime in its
//! signature: box a future in, nothing comes out. `synthia-harness`'s
//! default spawner is tokio-backed (it already depends on tokio for
//! its own internals), and a consumer on `smol`, `async-std`, or a
//! bare `futures::executor::ThreadPool` injects their own through
//! `ReActAgent::with_spawner`. Nothing in the loop knows which one it got.
//!
//! ## What is *not* runtime-neutral
//!
//! Being honest about the boundary is part of the contract:
//!
//! - the builtin tools (`read` / `write` / `shell`) use `tokio::fs`
//!   and `tokio::process`;
//! - provider HTTP adapters use `reqwest` + a tokio reactor;
//! - the JSONL session sink uses `tokio::fs` and `spawn_blocking`.
//!
//! Those are *plugins* — a consumer on another runtime implements
//! [`Tool`](https://docs.rs/synthia-tool) / `ModelProvider` /
//! `SessionSink` themselves and keeps the loop. The loop itself, the
//! event stream, the context managers, and the steering layer use
//! nothing but `futures` plus this trait.

use std::{future::Future, pin::Pin, sync::Arc};

/// A boxed, detached, `Send` future — the unit [`Spawner::spawn`]
/// accepts.
pub type BoxFuture<T = ()> = Pin<Box<dyn Future<Output = T> + Send + 'static>>;

/// Spawn detached work without naming a runtime.
///
/// Implementations own an executor (a tokio runtime, a
/// `futures::executor::ThreadPool`, a queue drained by the caller, …)
/// and must run the future to completion on it.
///
/// Object-safe on purpose: the crates that need it hold
/// `Arc<dyn Spawner>` so a consumer can swap the executor without
/// recompiling the framework.
pub trait Spawner: Send + Sync + 'static {
    /// Run `task` to completion somewhere this spawner owns.
    ///
    /// The caller never observes the result; a panicking task must not
    /// take the process down (implementations decide how — tokio's
    /// `spawn` catches it into the `JoinHandle` it drops).
    fn spawn(&self, task: BoxFuture<()>);

    /// Run a blocking closure off the async worker threads.
    ///
    /// Used by file-mutation paths where the critical section is
    /// synchronous. The default implementation runs `f` on the async
    /// executor via [`Spawner::spawn`] — correct but impolite for a
    /// real blocking call, so an implementation with a blocking pool
    /// (tokio's, `smol`'s `unblock`, a dedicated thread) SHOULD
    /// override it.
    ///
    /// The closure is erased to `FnOnce()` (no generics) to keep the
    /// trait object-safe; a caller that needs the return value hands
    /// it out through a channel of its own.
    fn spawn_blocking(&self, f: Box<dyn FnOnce() + Send + 'static>) {
        self.spawn(Box::pin(async move { f() }));
    }
}

/// A shared spawner handle — what the crates actually store.
pub type SharedSpawner = Arc<dyn Spawner>;

#[cfg(test)]
mod tests {
    use std::{
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
        task::{Context, Waker},
    };

    use super::*;

    /// A spawner that polls the future once, inline. It is the smallest
    /// possible implementation and it proves the trait needs no
    /// runtime: this test compiles in a crate with no async executor.
    struct InlineSpawner;

    impl Spawner for InlineSpawner {
        fn spawn(&self, mut task: BoxFuture<()>) {
            let mut cx = Context::from_waker(Waker::noop());
            // One poll is enough for a future that never awaits; a
            // real executor would loop until `Poll::Ready`.
            let _ = task.as_mut().poll(&mut cx);
        }
    }

    #[test]
    fn spawn_runs_the_future_without_a_runtime() {
        let payload = Arc::new(AtomicUsize::new(0));
        let seen = Arc::clone(&payload);
        InlineSpawner.spawn(Box::pin(async move {
            seen.store(7, Ordering::SeqCst);
        }));
        assert_eq!(payload.load(Ordering::SeqCst), 7);
    }

    #[test]
    fn default_spawn_blocking_runs_the_closure() {
        let cell = Arc::new(AtomicUsize::new(0));
        let seen = Arc::clone(&cell);
        InlineSpawner.spawn_blocking(Box::new(move || {
            seen.store(11, Ordering::SeqCst);
        }));
        assert_eq!(cell.load(Ordering::SeqCst), 11);
    }

    #[test]
    fn a_spawner_is_object_safe() {
        let spawner: Arc<dyn Spawner> = Arc::new(InlineSpawner);
        let ran = Arc::new(AtomicUsize::new(0));
        let seen = Arc::clone(&ran);
        spawner.spawn(Box::pin(async move {
            seen.fetch_add(1, Ordering::SeqCst);
        }));
        assert_eq!(ran.load(Ordering::SeqCst), 1);
    }
}
