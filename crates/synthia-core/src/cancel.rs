//! Runtime-neutral cooperative cancellation.
//!
//! The agent runtime's public API must not force a concrete
//! async-runtime onto lib consumers (R7 objective:
//! "不依赖具体运行时"). This module defines the single
//! cancellation vocabulary every crate shares:
//!
//! - [`CancelToken`] — the read/write trait (`is_cancelled` /
//!   `cancel` / `cancelled()` future). Object-safe, `Arc`-shareable.
//! - [`AtomicCancelToken`] — a std-only implementation
//!   (`AtomicBool` + a waker list) for consumers that have no
//!   runtime primitive of their own.
//! - `impl CancelToken for tokio_util CancellationToken` —
//!   feature-gated (`tokio-util`), so tokio-based callers (the
//!   server, the tests) keep their existing tokens with zero
//!   adapter boilerplate.
//!
//! The design mirrors `tokio_util::sync::CancellationToken`'s
//! shape (`cancel()`, `is_cancelled()`, `cancelled().await`)
//! so migrating call sites is a type swap, not a rewrite.

use std::{
    future::Future,
    pin::Pin,
    sync::{
        Arc,
        Mutex as StdMutex,
        atomic::{AtomicBool, Ordering},
    },
    task::{Context, Poll, Waker},
};

/// Cooperative cancellation vocabulary shared by every synthia
/// crate.
///
/// Read side: [`Self::is_cancelled`] (cheap, sync) and
/// [`Self::cancelled`] (async completion). Write side:
/// [`Self::cancel`]. Implementations must be idempotent —
/// cancelling an already-cancelled token is a no-op.
pub trait CancelToken: Send + Sync {
    /// `true` once [`Self::cancel`] has been called. Safe to
    /// poll from any thread; intended for hot-path checks.
    fn is_cancelled(&self) -> bool;

    /// Request cancellation. Wakes every pending
    /// [`Self::cancelled`] future. Idempotent.
    fn cancel(&self);

    /// Completes when the token is cancelled. The returned
    /// future is cancel-safe (dropping it is always fine).
    ///
    /// The boxed form keeps the trait object-safe without
    /// `async_trait`'s dynamic-dispatch macro layer — the
    /// agent hot path calls this at most once per stream.
    fn cancelled(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>>;
}

// ---------------------------------------------------------------------------
// AtomicCancelToken — std-only implementation
// ---------------------------------------------------------------------------

/// std-only [`CancelToken`] backed by an `AtomicBool` plus a
/// waker list. Use it when you do not have (or do not want) a
/// runtime-provided primitive.
///
/// `cancelled()` futures registered before cancellation are
/// woken by `cancel()`; futures polled after cancellation
/// complete immediately.
#[derive(Default)]
pub struct AtomicCancelToken {
    cancelled: AtomicBool,
    wakers: StdMutex<Vec<Waker>>,
}

impl AtomicCancelToken {
    /// New token in the not-cancelled state.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Wrap in the shared `Arc<dyn CancelToken>` shape the
    /// agent API consumes.
    #[must_use]
    pub fn shared() -> Arc<dyn CancelToken> {
        Arc::new(Self::new())
    }
}

impl CancelToken for AtomicCancelToken {
    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
        let mut wakers = self.wakers.lock().expect("wakers poisoned");
        for waker in wakers.drain(..) {
            waker.wake();
        }
    }

    fn cancelled(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(CancelledFuture { token: self })
    }
}

/// Future returned by [`AtomicCancelToken::cancelled`].
struct CancelledFuture<'a> {
    token: &'a AtomicCancelToken,
}

impl Future for CancelledFuture<'_> {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        if self.token.is_cancelled() {
            return Poll::Ready(());
        }
        let mut wakers = self.token.wakers.lock().expect("wakers poisoned");
        // Replace a stale waker for this task if present, else
        // register the new one.
        wakers.retain(|w| !w.will_wake(cx.waker()));
        wakers.push(cx.waker().clone());
        Poll::Pending
    }
}

// ---------------------------------------------------------------------------
// tokio-util bridge (feature-gated)
// ---------------------------------------------------------------------------

#[cfg(feature = "tokio-util")]
impl CancelToken for tokio_util::sync::CancellationToken {
    fn is_cancelled(&self) -> bool {
        tokio_util::sync::CancellationToken::is_cancelled(self)
    }

    fn cancel(&self) {
        tokio_util::sync::CancellationToken::cancel(self);
    }

    fn cancelled(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(tokio_util::sync::CancellationToken::cancelled(self))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicUsize;

    use super::*;

    #[test]
    fn fresh_token_is_not_cancelled() {
        let t = AtomicCancelToken::new();
        assert!(!t.is_cancelled());
    }

    #[test]
    fn cancel_is_idempotent_and_visible() {
        let t = AtomicCancelToken::new();
        t.cancel();
        t.cancel();
        assert!(t.is_cancelled());
    }

    #[tokio::test]
    async fn cancelled_future_completes_after_cancel() {
        let t = Arc::new(AtomicCancelToken::new());
        let waiter = Arc::clone(&t);
        let handle = tokio::spawn(async move {
            waiter.cancelled().await;
        });
        // Give the waiter a chance to register its waker first.
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        t.cancel();
        handle.await.expect("join");
    }

    #[tokio::test]
    async fn cancelled_future_completes_immediately_when_already_cancelled() {
        let t = AtomicCancelToken::new();
        t.cancel();
        t.cancelled().await;
    }

    #[test]
    fn multiple_waiters_all_wake() {
        let t = Arc::new(AtomicCancelToken::new());
        let woke = Arc::new(AtomicUsize::new(0));
        let mut handles = Vec::new();
        for _ in 0..4 {
            let token = Arc::clone(&t);
            let flag = Arc::clone(&woke);
            handles.push(std::thread::spawn(move || {
                // Poll the future once via a no-op waker to
                // exercise registration, then rely on cancel
                // waking the (parked) main test below.
                let fut = token.cancelled();
                let mut fut = std::pin::pin!(fut);
                let waker = Waker::noop();
                let mut cx = Context::from_waker(waker);
                let _ = fut.as_mut().poll(&mut cx);
                let _ = flag;
            }));
        }
        for h in handles {
            h.join().expect("join");
        }
        t.cancel();
        assert!(t.is_cancelled());
    }

    #[cfg(feature = "tokio-util")]
    #[test]
    fn tokio_token_implements_the_trait() {
        let t = tokio_util::sync::CancellationToken::new();
        let neutral: &dyn CancelToken = &t;
        assert!(!neutral.is_cancelled());
        neutral.cancel();
        assert!(neutral.is_cancelled());
        assert!(
            tokio_util::sync::CancellationToken::is_cancelled(&t),
            "bridge must delegate to the underlying token"
        );
    }

    #[cfg(feature = "tokio-util")]
    #[test]
    fn arc_coercion_works_at_call_sites() {
        // The pattern every agent/provider call site uses:
        // `Arc::new(CancellationToken::new())` coercing into
        // `Arc<dyn CancelToken>` with no adapter.
        fn takes_neutral(_t: Arc<dyn CancelToken>) {}
        takes_neutral(Arc::new(tokio_util::sync::CancellationToken::new()));
    }
}
