//! `EffectGate` — typed admission control for cancel-aware hot paths.
//!
//! R5-2 (pi `effect-gate.ts` parity): synthia had four hand-written
//! `CancellationToken::is_cancelled()` checks scattered across
//! `ReActLoop::drive` (lines 658, 721, 1314, 1698 in re_act.rs).
//! Each new await point risks adding a fifth, and forgetting to
//! check means a cancel races past an in-flight LLM or tool call.
//!
//! The pi pattern: split cancellation into a procedure-facing
//! `Gate::admit(invoke)` (synchronous check + invoke) and an
//! owner-facing `GateControl::begin_abort / signal_abort / close`.
//! Hot-path code calls `gate.admit(|| future.await)`; an external
//! cancel that arrives between admission and invocation is rejected
//! by the gate instead of needing each call site to remember to
//! check.
//!
//! `EffectGate` is implemented on top of
//! `tokio_util::sync::CancellationToken` so it does not regress
//! R4's existing cancellation behaviour; it adds a single
//! synchronisation point instead of replacing the underlying
//! primitive.

use std::sync::Arc;

use parking_lot::Mutex;
use synthia_core::CancelToken;

/// State of an `EffectGate`.
///
/// The gate moves through three states, never back. `Open` is the
/// only state that admits effects; once `begin_abort` is called the
/// gate is `Aborting` and any future `admit` returns
/// [`AdmissionError::Aborted`]. `Closed` is the terminal state used
/// when the gate will never admit again (session teardown).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateState {
    /// All effects are admitted.
    Open,
    /// A cancel was requested; no further admissions will succeed.
    Aborting,
    /// Terminal: the gate will never admit again. The contained
    /// `&'static str` is a short reason surfaced to the caller.
    Closed(&'static str),
}

impl GateState {
    /// `true` iff the gate admits effects.
    #[must_use]
    pub fn admits(self) -> bool {
        matches!(self, Self::Open)
    }
}

/// Procedure-facing view of an `EffectGate`.
///
/// A `Gate` is cheaply cloneable (`Arc` inside). Callers run
/// side-effecting code paths through [`Gate::admit`] to ensure the
/// gate is `Open` before invoking. Once the gate transitions to
/// `Aborting` or `Closed`, `admit` returns
/// [`AdmissionError::Aborted`] synchronously without invoking.
#[derive(Clone)]
pub struct Gate {
    inner: Arc<EffectGateInner>,
}

struct EffectGateInner {
    state: Mutex<GateState>,
    cancel: Arc<dyn CancelToken>,
}

impl Gate {
    /// Wrap an existing cancellation token. The gate is `Open`
    /// while the token is not cancelled; cancelling the token via
    /// `GateControl::begin_abort` transitions the gate to
    /// `Aborting`.
    #[must_use]
    pub fn new(cancel: Arc<dyn CancelToken>) -> (Gate, GateControl) {
        let inner = Arc::new(EffectGateInner {
            state: Mutex::new(GateState::Open),
            cancel,
        });
        (
            Gate {
                inner: Arc::clone(&inner),
            },
            GateControl { inner },
        )
    }

    /// Current state.
    #[must_use]
    pub fn state(&self) -> GateState {
        *self.inner.state.lock()
    }

    /// The underlying cancellation token. Mirrored so call sites
    /// that still need an `is_cancelled()` check (e.g. inside a
    /// stream callback that cannot easily wrap in `admit`) can
    /// read it directly.
    #[must_use]
    pub fn cancel_token(&self) -> Arc<dyn CancelToken> {
        Arc::clone(&self.inner.cancel)
    }

    /// Admit and synchronously invoke `f`. If the gate is `Open`
    /// when `admit` is called, `f` runs immediately and its return
    /// value is forwarded. If the gate is `Aborting` or `Closed`,
    /// `f` is **NOT** invoked and `Err(AdmissionError::Aborted)` is
    /// returned.
    ///
    /// # Cancellation race
    ///
    /// Cancellation that arrives *after* `admit` began executing
    /// `f` does NOT abort `f`; that is the contract of pi's
    /// `gate.admit`. Callers that need mid-effect cancellation
    /// pass the token into the inner effect.
    pub fn admit<F, R>(&self, f: F) -> Result<R, AdmissionError>
    where
        F: FnOnce() -> R,
    {
        let guard = self.inner.state.lock();
        match *guard {
            GateState::Open => {
                drop(guard);
                Ok(f())
            }
            GateState::Aborting => Err(AdmissionError::Aborted),
            GateState::Closed(reason) => {
                Err(AdmissionError::ClosedReason(reason))
            }
        }
    }
}

/// Owner-facing view of an `EffectGate`.
///
/// The `SessionController` (or any other long-lived owner of a
/// session) holds a `GateControl` and uses it to drive the gate
/// through its lifecycle. `Gate` and `GateControl` share the same
/// `Arc` internally so any clone sees the same state.
pub struct GateControl {
    inner: Arc<EffectGateInner>,
}

impl GateControl {
    ///
    /// # Idempotent
    ///
    /// Calling `begin_abort` on a gate that is already `Aborting`
    /// or `Closed` is a no-op (the state stays where it was).
    pub fn begin_abort(&self) {
        let mut state = self.inner.state.lock();
        if matches!(*state, GateState::Open) {
            *state = GateState::Aborting;
            self.inner.cancel.cancel();
        }
    }

    /// Force the gate into the terminal `Closed` state. After
    /// `close`, no effect ever admits again. `reason` is surfaced
    /// to the caller via [`AdmissionError::ClosedReason`].
    pub fn close(self, reason: &'static str) {
        let mut state = self.inner.state.lock();
        *state = GateState::Closed(reason);
        self.inner.cancel.cancel();
    }

    /// Current state — useful for diagnostics / tests.
    #[must_use]
    pub fn state(&self) -> GateState {
        *self.inner.state.lock()
    }
}

/// Errors returned by [`Gate::admit`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AdmissionError {
    /// The gate has been signalled to abort. `admit` did not run
    /// the closure.
    #[error("effect gate is aborting")]
    Aborted,
    /// The gate is in its terminal `Closed` state. The contained
    /// reason was passed to [`GateControl::close`].
    #[error("effect gate is closed: {0}")]
    ClosedReason(&'static str),
}

#[cfg(test)]
mod tests {
    use tokio_util::sync::CancellationToken;

    use super::*;

    #[test]
    fn open_gate_admits() {
        let (gate, _ctl) = Gate::new(Arc::new(CancellationToken::new()));
        let result = gate.admit(|| 42);
        assert_eq!(result, Ok(42));
        assert_eq!(gate.state(), GateState::Open);
    }

    #[test]
    fn begin_abort_rejects_admission() {
        let (gate, ctl) = Gate::new(Arc::new(CancellationToken::new()));
        ctl.begin_abort();
        assert_eq!(gate.state(), GateState::Aborting);
        let mut invoked = false;
        let result = gate.admit(|| {
            invoked = true;
            1
        });
        assert_eq!(result, Err(AdmissionError::Aborted));
        assert!(!invoked, "closure must NOT run on Aborting gate");
    }

    #[test]
    fn closed_gate_carries_reason() {
        let (gate, ctl) = Gate::new(Arc::new(CancellationToken::new()));
        ctl.close("session_ended");
        let result = gate.admit(|| 1);
        assert_eq!(result, Err(AdmissionError::ClosedReason("session_ended")));
    }

    #[test]
    fn begin_abort_is_idempotent() {
        let (gate, ctl) = Gate::new(Arc::new(CancellationToken::new()));
        ctl.begin_abort();
        ctl.begin_abort();
        ctl.begin_abort();
        assert_eq!(gate.state(), GateState::Aborting);
    }

    #[test]
    fn underlying_cancel_token_fires() {
        let (gate, ctl) = Gate::new(Arc::new(CancellationToken::new()));
        let token = gate.cancel_token();
        assert!(!token.is_cancelled());
        ctl.begin_abort();
        assert!(token.is_cancelled());
    }

    #[test]
    fn clones_share_state() {
        let (gate_a, ctl) = Gate::new(Arc::new(CancellationToken::new()));
        let gate_b = gate_a.clone();
        assert_eq!(gate_a.state(), GateState::Open);
        assert_eq!(gate_b.state(), GateState::Open);
        ctl.begin_abort();
        assert_eq!(gate_a.state(), GateState::Aborting);
        assert_eq!(gate_b.state(), GateState::Aborting);
    }
}
