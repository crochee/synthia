//! Defect channel for turn transitions.
//!
//! Distinguishes recoverable ([`ControlFlow::Continue`]) vs unrecoverable
//! ([`ControlFlow::Break`]) defects and applies a bounded retry cap before
//! terminating the run.
//!
//! This module is intentionally parallel to [`crate::error_recovery`]: the
//! legacy five-layer cascade (Truncate → Retry → Fallback → Auto-compact →
//! Reset → Fail-fast) remains the long-form recovery pipeline. The defect
//! channel is the *short-form* decision gate used inside the turn loop: each
//! turn returns a [`TurnResult`], and [`handle_defect_with_count`] decides
//! whether to retry the turn (`Continue`, up to [`MAX_DEFECT_RETRIES`]) or
//! terminate the run (`Break`, or cap exceeded).

use std::{future::Future, ops::ControlFlow};

/// Maximum number of consecutive `Continue` defects before the defect channel
/// terminates the run.
pub const MAX_DEFECT_RETRIES: u32 = 3;

/// A defect observed while transitioning between turns.
///
/// - [`TurnTransition::ContextOverflow`] / [`TurnTransition::ToolExecutionFailure`]
///   are *recoverable*: when wrapped in [`ControlFlow::Continue`] they trigger
///   compaction + a retried turn (subject to [`MAX_DEFECT_RETRIES`]).
/// - [`TurnTransition::FatalError`] is *unrecoverable*: when wrapped in
///   [`ControlFlow::Break`] it terminates the run immediately.
#[derive(Debug, Clone)]
pub enum TurnTransition {
    /// The turn's context exceeded the model window; recoverable via compaction.
    ContextOverflow,
    /// A tool call failed during the turn; recoverable via retry.
    ToolExecutionFailure(String),
    /// A fatal, non-recoverable error; terminates the run.
    FatalError(String),
}

/// Result of a single turn.
///
/// - `Ok(T)` — the turn produced a value.
/// - `Err(Continue(t))` — recoverable defect (retry after compaction).
/// - `Err(Break(t))` — unrecoverable defect (terminate + propagate).
///
/// Both variants carry the defect descriptor so [`handle_defect_with_count`]
/// can discriminate on the [`TurnTransition`] kind.
pub type TurnResult<T> = Result<T, ControlFlow<TurnTransition, TurnTransition>>;

/// Action decided by the defect channel for a given [`TurnTransition`].
#[derive(Debug)]
pub enum DefectAction {
    /// Retry the turn (recoverable defect, under the retry cap).
    Retry,
    /// Terminate the run with a human-readable reason.
    Terminate(String),
}

/// Handles a defect using a fresh retry counter (starts at 0).
///
/// Convenience wrapper around [`handle_defect_with_count`] for callers that do
/// not track retries across calls.
pub async fn handle_defect(
    defect: ControlFlow<TurnTransition, TurnTransition>,
) -> DefectAction {
    handle_defect_with_count(defect, &mut 0).await
}

/// Handles a defect against a caller-managed retry counter.
///
/// - `Continue(_)` — recoverable: increments `retry_count` and returns
///   [`DefectAction::Retry`], unless `retry_count` has already reached
///   [`MAX_DEFECT_RETRIES`], in which case it returns [`DefectAction::Terminate`]
///   with `"max defect retries (3) exceeded"`.
/// - `Break(t)` — unrecoverable: returns [`DefectAction::Terminate`] with the
///   debug representation of `t`.
///
/// The counter is shared across all [`TurnTransition`] variants so that
/// mixed-type defect storms (e.g. an overflow followed by a tool failure) are
/// bounded by a single budget. Callers that observe a successful turn may reset
/// `retry_count` to `0`.
pub async fn handle_defect_with_count(
    defect: ControlFlow<TurnTransition, TurnTransition>,
    retry_count: &mut u32,
) -> DefectAction {
    match defect {
        ControlFlow::Continue(_) => {
            if *retry_count >= MAX_DEFECT_RETRIES {
                DefectAction::Terminate(
                    "max defect retries (3) exceeded".into(),
                )
            } else {
                *retry_count += 1;
                DefectAction::Retry
            }
        }
        ControlFlow::Break(t) => DefectAction::Terminate(format!("{t:?}")),
    }
}

/// Executes a turn with defect handling.
///
/// Repeatedly invokes `turn_fn` until it either returns `Ok(value)` or the
/// defect channel decides to terminate. Each `Err(Continue(_))` consumes one
/// retry from `retry_count`; once the cap is exceeded the run terminates with
/// the channel's reason string.
///
/// Integration point for the main turn loop — see
/// `stream_builder/builder/run/main_loop.rs`:
/// ```text
/// // TODO: wire TurnTransition here once main_loop refactor lands
/// ```
pub async fn execute_turn_with_defect_handling<T, F, Fut>(
    turn_fn: F,
    retry_count: &mut u32,
) -> Result<T, String>
where
    F: Fn() -> Fut,
    Fut: Future<Output = TurnResult<T>>,
{
    loop {
        match turn_fn().await {
            Ok(value) => return Ok(value),
            Err(defect) => {
                match handle_defect_with_count(defect, retry_count).await {
                    DefectAction::Retry => continue,
                    DefectAction::Terminate(reason) => return Err(reason),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn continue_defect_triggers_retry() {
        let action = handle_defect(ControlFlow::Continue(
            TurnTransition::ContextOverflow,
        ))
        .await;
        assert!(matches!(action, DefectAction::Retry));
    }

    #[tokio::test]
    async fn break_defect_terminates() {
        let action = handle_defect(ControlFlow::Break(
            TurnTransition::FatalError("test".into()),
        ))
        .await;
        match action {
            DefectAction::Terminate(reason) => {
                assert!(reason.contains("FatalError"), "reason: {reason}");
            }
            other => panic!("expected Terminate, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn fourth_retry_rejected() {
        let mut count = 3u32;
        let action = handle_defect_with_count(
            ControlFlow::Continue(TurnTransition::ContextOverflow),
            &mut count,
        )
        .await;
        match action {
            DefectAction::Terminate(reason) => {
                assert!(
                    reason.contains("max defect retries (3) exceeded"),
                    "reason: {reason}"
                );
            }
            other => panic!("expected Terminate, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn cross_type_shared_count() {
        let mut count = 0u32;

        // Continue(ContextOverflow) twice: 0 -> 1, 1 -> 2
        assert!(matches!(
            handle_defect_with_count(
                ControlFlow::Continue(TurnTransition::ContextOverflow),
                &mut count,
            )
            .await,
            DefectAction::Retry
        ));
        assert_eq!(count, 1);
        assert!(matches!(
            handle_defect_with_count(
                ControlFlow::Continue(TurnTransition::ContextOverflow),
                &mut count,
            )
            .await,
            DefectAction::Retry
        ));
        assert_eq!(count, 2);

        // Continue(ToolExecutionFailure) once: 2 -> 3 (shared count, different variant)
        assert!(matches!(
            handle_defect_with_count(
                ControlFlow::Continue(TurnTransition::ToolExecutionFailure(
                    "boom".into()
                )),
                &mut count,
            )
            .await,
            DefectAction::Retry
        ));
        assert_eq!(count, 3);

        // 4th call (regardless of variant) is rejected.
        let action = handle_defect_with_count(
            ControlFlow::Continue(TurnTransition::ContextOverflow),
            &mut count,
        )
        .await;
        let reason = match action {
            DefectAction::Terminate(r) => r,
            other => panic!("expected Terminate, got {other:?}"),
        };
        assert!(
            reason.contains("max defect retries (3) exceeded"),
            "reason: {reason}"
        );
    }

    #[tokio::test]
    async fn third_success_resets_count() {
        let mut count = 3u32;

        // At the cap, a Continue defect is rejected.
        assert!(matches!(
            handle_defect_with_count(
                ControlFlow::Continue(TurnTransition::ContextOverflow),
                &mut count,
            )
            .await,
            DefectAction::Terminate(_)
        ));

        // After a successful turn the caller resets the shared counter to 0.
        count = 0;

        // A subsequent Continue defect must be retried (not rejected).
        assert!(matches!(
            handle_defect_with_count(
                ControlFlow::Continue(TurnTransition::ContextOverflow),
                &mut count,
            )
            .await,
            DefectAction::Retry
        ));
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn break_includes_error_message() {
        let action = handle_defect(ControlFlow::Break(
            TurnTransition::ToolExecutionFailure("disk full".into()),
        ))
        .await;
        match action {
            DefectAction::Terminate(reason) => {
                assert!(reason.contains("disk full"), "reason: {reason}");
            }
            other => panic!("expected Terminate, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn execute_turn_retries_on_continue_then_succeeds() {
        use std::sync::{
            Arc,
            atomic::{AtomicU32, Ordering},
        };

        let attempts = Arc::new(AtomicU32::new(0));
        let attempts_for_closure = Arc::clone(&attempts);
        let mut count = 0u32;

        let result: Result<u32, String> = execute_turn_with_defect_handling(
            || {
                let a = Arc::clone(&attempts_for_closure);
                async move {
                    let n = a.fetch_add(1, Ordering::Relaxed) + 1;
                    if n < 2 {
                        Err(ControlFlow::Continue(
                            TurnTransition::ContextOverflow,
                        ))
                    } else {
                        Ok(n)
                    }
                }
            },
            &mut count,
        )
        .await;

        assert_eq!(result, Ok(2));
        assert_eq!(count, 1);
    }
}
