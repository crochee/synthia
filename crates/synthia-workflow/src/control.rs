//! Live control over a run.
//!
//! A [`WorkflowControl`] is a handle, not a thread: the caller owns the
//! run future and this type only records intent. The runtime reads that
//! intent at the points where it changes what happens — before a call is
//! admitted, before it starts, and after it settles — so control is a
//! state machine with defined semantics rather than an interruption.
//!
//! | Command | Effect |
//! |---|---|
//! | [`pause`](WorkflowControl::pause) | No new call starts. In-flight calls keep running; each waiter parks *before* taking a slot, so a paused run holds no concurrency it is not using. |
//! | [`resume`](WorkflowControl::resume) | Releases every parked call at once. |
//! | [`skip`](WorkflowControl::skip) | A named step's calls are reported `Skipped`, never spawned, and journaled as failures so a resume re-runs them. |
//! | [`retry`](WorkflowControl::retry) | One more attempt for a named step: a failed settle with a pending request re-runs instead of recording a failure, and a request pending before the step starts forces it to run live rather than replay. |
//! | [`abort`](WorkflowControl::abort) | Admits nothing further, drains the queue and lets in-flight work finish. |
//!
//! Control applies to the runtime it is installed on. A run that is
//! already over ignores it.

use std::{
    collections::BTreeSet,
    future::Future,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll, Waker},
};

use parking_lot::{Mutex, MutexGuard};

/// What a run can be told to do while it is going.
///
/// Cloning shares the same state: hand a clone to whoever needs to steer
/// and keep one for the runtime.
#[derive(Debug, Clone, Default)]
pub struct WorkflowControl {
    state: Arc<Mutex<ControlState>>,
}

#[derive(Debug, Default)]
struct ControlState {
    paused: bool,
    aborted: bool,
    skipped: BTreeSet<String>,
    retries: BTreeSet<String>,
    wakers: Vec<Waker>,
}

impl WorkflowControl {
    /// A control handle with nothing pending: not paused, not aborted.
    pub fn new() -> Self {
        Self::default()
    }

    /// Stop admitting calls until [`Self::resume`].
    pub fn pause(&self) {
        self.lock().paused = true;
    }

    /// Release everything [`Self::pause`] parked.
    pub fn resume(&self) {
        let wakers = {
            let mut state = self.lock();
            state.paused = false;
            std::mem::take(&mut state.wakers)
        };
        for waker in wakers {
            waker.wake();
        }
    }

    /// Skip a step's calls: they are reported `Skipped` and never
    /// spawned. A call that is already running is not interrupted.
    pub fn skip(&self, step_id: impl Into<String>) {
        self.lock().skipped.insert(step_id.into());
    }

    /// Request one more attempt for a step.
    ///
    /// The request is consumed at the step's next settle: a failure then
    /// re-runs instead of being recorded, and a success uses the request
    /// up doing nothing. A request pending before the step starts also
    /// keeps it from being replayed from the journal.
    pub fn retry(&self, step_id: impl Into<String>) {
        self.lock().retries.insert(step_id.into());
    }

    /// Stop admitting calls, drain the queue, and let in-flight work
    /// finish before the run reports.
    pub fn abort(&self) {
        let wakers = {
            let mut state = self.lock();
            state.aborted = true;
            std::mem::take(&mut state.wakers)
        };
        for waker in wakers {
            waker.wake();
        }
    }

    /// Whether the run is paused.
    pub fn is_paused(&self) -> bool {
        self.lock().paused
    }

    /// Whether the run has been aborted.
    pub fn is_aborted(&self) -> bool {
        self.lock().aborted
    }

    /// Whether a step's calls were asked to be skipped.
    pub fn is_skipped(&self, step_id: &str) -> bool {
        self.lock().skipped.contains(step_id)
    }

    /// Whether a step has a retry request waiting.
    pub fn has_retry(&self, step_id: &str) -> bool {
        self.lock().retries.contains(step_id)
    }

    /// Take a step's retry request, if it has one.
    pub fn take_retry(&self, step_id: &str) -> bool {
        self.lock().retries.remove(step_id)
    }

    /// Wait until the run is either running again or aborted.
    pub(crate) async fn wait_until_open(&self) {
        Wait {
            control: self,
            until: Until::Open,
        }
        .await
    }

    /// Wait until the run is aborted.
    pub(crate) async fn wait_for_abort(&self) {
        Wait {
            control: self,
            until: Until::Abort,
        }
        .await
    }

    fn lock(&self) -> MutexGuard<'_, ControlState> {
        self.state.lock()
    }
}

/// Which condition a [`Wait`] is parked on.
enum Until {
    /// Not paused, or aborted — whichever comes first.
    Open,
    /// Aborted.
    Abort,
}

impl Until {
    fn reached(&self, state: &ControlState) -> bool {
        match self {
            Self::Open => !state.paused || state.aborted,
            Self::Abort => state.aborted,
        }
    }
}

/// Parks until a [`WorkflowControl`] condition holds.
struct Wait<'a> {
    control: &'a WorkflowControl,
    until: Until,
}

impl Future for Wait<'_> {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        let this = self.get_mut();
        let mut state = this.control.lock();
        if this.until.reached(&state) {
            return Poll::Ready(());
        }
        state.wakers.push(cx.waker().clone());
        Poll::Pending
    }
}

#[cfg(test)]
mod tests {
    use futures::FutureExt;
    use tokio::test;

    use super::*;

    #[test]
    async fn pause_parks_until_resume() {
        let control = WorkflowControl::new();
        control.pause();

        let waiting = control.wait_until_open();
        futures::pin_mut!(waiting);
        assert!(waiting.as_mut().now_or_never().is_none());

        control.resume();
        assert!(matches!(waiting.as_mut().now_or_never(), Some(())));
        assert!(!control.is_paused());
    }

    #[test]
    async fn abort_releases_a_paused_waiter() {
        let control = WorkflowControl::new();
        control.pause();

        let waiting = control.wait_until_open();
        futures::pin_mut!(waiting);
        assert!(waiting.as_mut().now_or_never().is_none());

        control.abort();
        assert!(matches!(waiting.as_mut().now_or_never(), Some(())));
        assert!(control.is_aborted());
    }

    #[test]
    async fn abort_wakes_an_abort_watcher() {
        let control = WorkflowControl::new();
        let watching = control.wait_for_abort();
        futures::pin_mut!(watching);
        assert!(watching.as_mut().now_or_never().is_none());

        control.abort();
        assert!(matches!(watching.as_mut().now_or_never(), Some(())));
    }

    #[test]
    async fn skips_persist_and_retries_are_consumed() {
        let control = WorkflowControl::new();
        control.skip("slow");
        control.retry("flaky");

        assert!(control.is_skipped("slow"));
        assert!(!control.is_skipped("other"));
        assert!(control.has_retry("flaky"));
        assert!(control.take_retry("flaky"));
        assert!(!control.has_retry("flaky"));
        assert!(!control.take_retry("flaky"));
    }
}
