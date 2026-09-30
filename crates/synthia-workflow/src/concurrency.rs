//! Admission control for agent calls.
//!
//! One counting semaphore bounds everything a run does. It is
//! runtime-neutral on purpose — no timers, no task handles, no runtime
//! types — because the crate's public surface must be usable from any
//! executor: an `Acquire` is an ordinary [`Future`], and
//! `Semaphore::drain` is how an abort wakes whatever is parked behind
//! the limit so it can observe the abort and give up.

use std::{
    collections::VecDeque,
    future::Future,
    pin::Pin,
    task::{Context, Poll, Waker},
};

use parking_lot::{Mutex, MutexGuard};

/// Concurrent agent calls allowed by default.
///
/// Two cores are left for the host and its own work; the ceiling of 16
/// keeps a large machine from opening more model streams than any
/// provider will serve.
pub fn default_max_concurrency() -> usize {
    let cpus = std::thread::available_parallelism()
        .map(|count| count.get())
        .unwrap_or(1);
    cpus.saturating_sub(2).clamp(1, 16)
}

/// A counting semaphore whose waiters are served in FIFO order.
///
/// A permit is never handed to a specific waiter: [`Semaphore::release`]
/// makes a permit available and wakes the queue head, which takes it on
/// its next poll. A waiter that is dropped — an aborted or cancelled
/// call — leaves the queue and passes the wake-up on, so no permit is
/// ever stranded on a future nobody polls again.
pub(crate) struct Semaphore {
    state: Mutex<State>,
}

struct State {
    limit: usize,
    active: usize,
    draining: bool,
    next_waiter: u64,
    waiters: VecDeque<Waiter>,
}

struct Waiter {
    id: u64,
    waker: Waker,
}

/// The semaphore was drained while a call waited: the run is over and
/// the call must not start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SemaphoreDrained;

impl Semaphore {
    pub(crate) fn new(limit: usize) -> Self {
        Self {
            state: Mutex::new(State {
                limit: limit.max(1),
                active: 0,
                draining: false,
                next_waiter: 0,
                waiters: VecDeque::new(),
            }),
        }
    }

    /// Ask for a permit, waiting until one is free.
    pub(crate) fn acquire(&self) -> Acquire<'_> {
        Acquire {
            semaphore: self,
            waiter: None,
        }
    }

    /// Stop admitting and wake every queued waiter.
    ///
    /// Permits already handed out are untouched: the run drains
    /// admission and then awaits the work it already started.
    pub(crate) fn drain(&self) {
        let mut state = self.lock();
        state.draining = true;
        for waiter in state.waiters.drain(..) {
            waiter.waker.wake();
        }
    }

    fn release(&self) {
        let mut state = self.lock();
        state.active = state.active.saturating_sub(1);
        // Wake, but do not pop: the head takes the permit when it is
        // polled, and a head that is dropped passes the wake-up on.
        if let Some(head) = state.waiters.front() {
            head.waker.wake_by_ref();
        }
    }

    fn leave_queue(&self, id: u64) {
        let mut state = self.lock();
        let Some(index) =
            state.waiters.iter().position(|waiter| waiter.id == id)
        else {
            return;
        };
        state.waiters.remove(index);
        if index == 0
            && let Some(head) = state.waiters.front()
        {
            head.waker.wake_by_ref();
        }
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock()
    }

    #[cfg(test)]
    fn counts(&self) -> (usize, usize) {
        let state = self.lock();
        (state.active, state.waiters.len())
    }
}

/// One queued request for a permit. Dropping it cancels the request.
pub(crate) struct Acquire<'a> {
    semaphore: &'a Semaphore,
    waiter: Option<u64>,
}

impl<'a> Future for Acquire<'a> {
    type Output = Result<Permit<'a>, SemaphoreDrained>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        let mut state = this.semaphore.lock();
        if state.draining {
            return Poll::Ready(Err(SemaphoreDrained));
        }
        match this.waiter {
            None => {
                if state.waiters.is_empty() && state.active < state.limit {
                    state.active += 1;
                    return Poll::Ready(Ok(Permit {
                        semaphore: this.semaphore,
                    }));
                }
                let id = state.next_waiter;
                state.next_waiter += 1;
                state.waiters.push_back(Waiter {
                    id,
                    waker: cx.waker().clone(),
                });
                this.waiter = Some(id);
                Poll::Pending
            }
            Some(id) => {
                let at_head =
                    state.waiters.front().map(|waiter| waiter.id) == Some(id);
                if at_head && state.active < state.limit {
                    state.waiters.pop_front();
                    state.active += 1;
                    return Poll::Ready(Ok(Permit {
                        semaphore: this.semaphore,
                    }));
                }
                if let Some(waiter) =
                    state.waiters.iter_mut().find(|waiter| waiter.id == id)
                {
                    waiter.waker = cx.waker().clone();
                }
                Poll::Pending
            }
        }
    }
}

impl Drop for Acquire<'_> {
    fn drop(&mut self) {
        if let Some(id) = self.waiter {
            self.semaphore.leave_queue(id);
        }
    }
}

/// Proof that a call holds a permit; dropping it hands the slot on.
pub(crate) struct Permit<'a> {
    semaphore: &'a Semaphore,
}

impl Drop for Permit<'_> {
    fn drop(&mut self) {
        self.semaphore.release();
    }
}

#[cfg(test)]
mod tests {
    use futures::FutureExt;
    use tokio::test;

    use super::*;

    #[test]
    async fn permits_are_served_in_arrival_order() {
        let semaphore = Semaphore::new(1);
        let trace = Mutex::new(Vec::new());

        let hold = |tag: u32| {
            let semaphore = &semaphore;
            let trace = &trace;
            async move {
                let permit = semaphore.acquire().await.unwrap();
                trace.lock().push(tag);
                drop(permit);
            }
        };
        futures::future::join_all([hold(1), hold(2), hold(3)]).await;

        assert_eq!(*trace.lock(), vec![1, 2, 3]);
        assert_eq!(semaphore.counts(), (0, 0));
    }

    #[test]
    async fn the_limit_bounds_how_many_hold_at_once() {
        let semaphore = Semaphore::new(2);
        let in_flight = Mutex::new(0usize);
        let peak = Mutex::new(0usize);

        let hold = || async {
            let permit = semaphore.acquire().await.unwrap();
            {
                let mut now = in_flight.lock();
                *now += 1;
                let mut peak = peak.lock();
                *peak = (*peak).max(*now);
            }
            // Yield while holding the permit: without a suspension
            // point every future would run its whole critical section
            // inside one poll, and the bound could never be observed.
            tokio::task::yield_now().await;
            *in_flight.lock() -= 1;
            drop(permit);
        };
        futures::future::join_all((0..6).map(|_| hold())).await;

        assert_eq!(*peak.lock(), 2, "the limit is reached, never exceeded");
        assert_eq!(semaphore.counts(), (0, 0));
    }

    #[test]
    async fn a_dropped_waiter_hands_its_slot_on() {
        let semaphore = Semaphore::new(1);
        let permit = semaphore.acquire().await.unwrap();

        // Queued, then dropped before it can be served.
        let abandoned = semaphore.acquire();
        assert!(abandoned.now_or_never().is_none());
        // Queued behind it, and kept alive across polls.
        let waiting = semaphore.acquire();
        futures::pin_mut!(waiting);
        assert!(waiting.as_mut().now_or_never().is_none());

        drop(permit);
        assert!(matches!(waiting.as_mut().now_or_never(), Some(Ok(_))));
    }

    #[test]
    async fn draining_refuses_new_and_queued_waiters() {
        let semaphore = Semaphore::new(1);
        let permit = semaphore.acquire().await.unwrap();
        let waiting = semaphore.acquire();
        futures::pin_mut!(waiting);
        assert!(waiting.as_mut().now_or_never().is_none());

        semaphore.drain();
        assert!(matches!(
            waiting.as_mut().now_or_never(),
            Some(Err(SemaphoreDrained))
        ));
        let fresh = semaphore.acquire();
        assert!(matches!(fresh.now_or_never(), Some(Err(SemaphoreDrained))));
        drop(permit);
    }

    #[test]
    async fn the_default_bound_leaves_room_for_the_host() {
        let cpus = std::thread::available_parallelism()
            .map(|count| count.get())
            .unwrap_or(1);
        let expected = cpus.saturating_sub(2).clamp(1, 16);
        assert_eq!(default_max_concurrency(), expected);
    }
}
