//! [`Guard`] — synchronous, veto-capable pre-execution policy.
//!
//! A guard inspects an outgoing [`Action`] and the run's
//! [`AgentState`] and returns a [`GuardResult`]. Guards are
//! deliberately **synchronous**: they must complete in
//! microseconds (regex + counter checks, no I/O) so the tool
//! dispatch path never grows an await per policy check. A guard
//! that needs remote state should pre-load it behind the trait.
//!
//! ## Pipeline semantics ([`run_guards`])
//!
//! - Guards run in registration order; the **first denial wins**
//!   and short-circuits the pipeline.
//! - A sanitize result rewrites the action and the remaining
//!   guards see the sanitized form (folding semantics).
//! - A panicking guard is treated as a denial (fail-closed) —
//!   policy code must never crash the agent loop.
//!
//! ## Deny semantics in the loop
//!
//! A denial is surfaced to the model as the tool result (an error
//! string carrying guard name + severity + reason) so the model
//! can adapt, never as a run abort. This mirrors the
//! errors-as-results contract used for tool failures.
//!
//! [`Action`]: crate::Action

use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::Arc,
};

use synthia_context::AgentState;

use crate::Action;

/// How serious a denial is. Surfaced in the model-facing denial
/// message and in the `WarningKind::Guard` event so operators can
/// filter critical policy hits from advisory ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum GuardSeverity {
    /// Advisory: the action was denied for hygiene reasons
    /// (e.g. loop detection).
    Medium,
    /// Dangerous action class (destructive command, workspace
    /// escape, sensitive data).
    High,
    /// Security boundary (prompt injection, exhausted hard
    /// budget). Operators should page on these.
    Critical,
}

impl GuardSeverity {
    /// Wire name (snake_case), used in denial messages.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Medium => "medium",
            Self::High => "high",
            Self::Critical => "critical",
        }
    }
}

/// One guard's verdict on an [`Action`].
#[derive(Debug, Clone)]
pub enum GuardResult {
    /// Proceed unchanged.
    Allow,
    /// Veto. `reason` MUST be actionable — it is fed back to the
    /// model as the tool result.
    Deny {
        reason: String,
        severity: GuardSeverity,
    },
    /// Rewrite the action and continue. Unlike traitclaw — where
    /// the sanitized action is silently discarded — the synthia
    /// loop adopts the rewritten action before dispatch.
    Sanitize { action: Action, warning: String },
}

/// Outcome of the whole [`run_guards`] pipeline.
#[derive(Debug, Clone)]
pub enum GuardOutcome {
    /// No guard objected; the (possibly folded) action may run.
    Allowed { action: Action },
    /// A guard vetoed; nothing executes.
    Denied {
        guard: String,
        reason: String,
        severity: GuardSeverity,
    },
}

/// Synchronous pre-execution policy check.
pub trait Guard: Send + Sync {
    /// Stable guard name for logs / denial messages.
    fn name(&self) -> &str;

    /// Inspect one outgoing action. MUST NOT mutate the state
    /// (the agent loop is the single writer of [`AgentState`]);
    /// MUST NOT panic (a panic is converted to a fail-closed
    /// denial by [`run_guards`]).
    fn check(&self, action: &Action, state: &AgentState) -> GuardResult;
}

/// Permissive guard — always allows. Used as the neutral element
/// so callers never hold an `Option<Guard>`.
pub struct NoopGuard;

impl Guard for NoopGuard {
    fn name(&self) -> &str {
        "noop"
    }

    fn check(&self, _action: &Action, _state: &AgentState) -> GuardResult {
        GuardResult::Allow
    }
}

/// Run the guard pipeline over one action.
///
/// Folds sanitizes through the remaining guards, short-circuits on
/// the first denial, and converts guard panics into critical
/// denials (fail-closed).
pub fn run_guards(
    guards: &[Arc<dyn Guard>],
    action: &Action,
    state: &AgentState,
) -> GuardOutcome {
    let mut current = action.clone();
    for guard in guards {
        let verdict = match catch_unwind(AssertUnwindSafe(|| {
            guard.check(&current, state)
        })) {
            Ok(verdict) => verdict,
            Err(panic) => {
                let reason = synthia_core::panic_message(panic);
                tracing::warn!(
                    guard = guard.name(),
                    reason = %reason,
                    "guard panicked; failing closed"
                );
                return GuardOutcome::Denied {
                    guard: guard.name().to_string(),
                    reason: format!(
                        "guard panicked and failed closed: {reason}"
                    ),
                    severity: GuardSeverity::Critical,
                };
            }
        };
        match verdict {
            GuardResult::Allow => {}
            GuardResult::Deny { reason, severity } => {
                return GuardOutcome::Denied {
                    guard: guard.name().to_string(),
                    reason,
                    severity,
                };
            }
            GuardResult::Sanitize {
                action: rewritten,
                warning,
            } => {
                tracing::debug!(
                    guard = guard.name(),
                    warning = %warning,
                    sanitized = %rewritten.summary(),
                    "guard sanitized action"
                );
                current = rewritten;
            }
        }
    }
    GuardOutcome::Allowed { action: current }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    struct DenyAll;
    impl Guard for DenyAll {
        fn name(&self) -> &str {
            "deny_all"
        }

        fn check(&self, _a: &Action, _s: &AgentState) -> GuardResult {
            GuardResult::Deny {
                reason: "nope".to_string(),
                severity: GuardSeverity::High,
            }
        }
    }

    struct UpperCaseShell;
    impl Guard for UpperCaseShell {
        fn name(&self) -> &str {
            "uppercase"
        }

        fn check(&self, a: &Action, _s: &AgentState) -> GuardResult {
            if let Action::ShellCommand {
                command,
                timeout_secs,
            } = a
                && command.chars().any(char::is_lowercase)
            {
                return GuardResult::Sanitize {
                    action: Action::ShellCommand {
                        command: command.to_uppercase(),
                        timeout_secs: *timeout_secs,
                    },
                    warning: "uppercased".to_string(),
                };
            }
            GuardResult::Allow
        }
    }

    struct Panicking;
    impl Guard for Panicking {
        fn name(&self) -> &str {
            "boom"
        }

        fn check(&self, _a: &Action, _s: &AgentState) -> GuardResult {
            panic!("kaboom");
        }
    }

    /// NoopGuard MUST always allow.
    #[test]
    fn noop_guard_always_allows() {
        let state = AgentState::with_window(100);
        let out = run_guards(
            &[Arc::new(NoopGuard)],
            &Action::RawOutput {
                content: "x".into(),
            },
            &state,
        );
        assert!(matches!(out, GuardOutcome::Allowed { .. }));
    }

    /// First denial wins and carries guard name + severity.
    #[test]
    fn first_denial_short_circuits() {
        let state = AgentState::with_window(100);
        let out = run_guards(
            &[Arc::new(DenyAll), Arc::new(NoopGuard)],
            &Action::RawOutput {
                content: "x".into(),
            },
            &state,
        );
        match out {
            GuardOutcome::Denied {
                guard, severity, ..
            } => {
                assert_eq!(guard, "deny_all");
                assert_eq!(severity, GuardSeverity::High);
            }
            other => panic!("expected denial, got {other:?}"),
        }
    }

    /// A sanitize MUST fold: later guards see the rewritten
    /// action, and the allowed outcome carries it.
    #[test]
    fn sanitize_folds_through_pipeline() {
        let state = AgentState::with_window(100);
        let out = run_guards(
            &[Arc::new(UpperCaseShell), Arc::new(NoopGuard)],
            &Action::ShellCommand {
                command: "ls".to_string(),
                timeout_secs: None,
            },
            &state,
        );
        match out {
            GuardOutcome::Allowed { action } => {
                assert_eq!(
                    action,
                    Action::ShellCommand {
                        command: "LS".to_string(),
                        timeout_secs: None,
                    }
                );
            }
            other => panic!("expected allowed, got {other:?}"),
        }
    }

    /// A panic MUST convert to a fail-closed critical denial.
    #[test]
    fn panicking_guard_fails_closed() {
        let state = AgentState::with_window(100);
        let out = run_guards(
            &[Arc::new(Panicking)],
            &Action::RawOutput {
                content: "x".into(),
            },
            &state,
        );
        match out {
            GuardOutcome::Denied {
                guard,
                severity,
                reason,
            } => {
                assert_eq!(guard, "boom");
                assert_eq!(severity, GuardSeverity::Critical);
                assert!(reason.contains("kaboom"));
            }
            other => panic!("expected denial, got {other:?}"),
        }
    }

    /// Shared-state guards behind Arc verify object safety.
    #[test]
    fn guards_are_object_safe() {
        struct Counting(AtomicUsize);
        impl Guard for Counting {
            fn name(&self) -> &str {
                "counting"
            }

            fn check(&self, _a: &Action, _s: &AgentState) -> GuardResult {
                self.0.fetch_add(1, Ordering::SeqCst);
                GuardResult::Allow
            }
        }

        let state = AgentState::with_window(100);
        let counting = Arc::new(Counting(AtomicUsize::new(0)));
        let guard: Arc<dyn Guard> = Arc::clone(&counting) as Arc<dyn Guard>;
        let _ = run_guards(
            &[guard],
            &Action::RawOutput {
                content: String::new(),
            },
            &state,
        );
        assert_eq!(counting.0.load(Ordering::SeqCst), 1);
    }
}
