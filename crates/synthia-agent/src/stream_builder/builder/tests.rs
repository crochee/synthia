//! Doom-loop unit tests for the [`StreamBuilder`] ReAct
//! loop.
//!
//! These tests pin the **behaviour** of the
//! `synthia_guardian::LoopDetectorSet` integration inside
//! [`super::iteration::check_doom_loop`]. They are
//! deliberately co-located with the `builder` module
//! because the doom-loop signal is consumed at exactly
//! one call site (the iteration body) and the test reads
//! more naturally next to the call site than in a
//! separate `tests/` integration crate.
//!
//! The tests do **not** spin up a full
//! [`StreamBuilder`](super::types::StreamBuilder) — that
//! is covered by the higher-level integration tests in
//! `synthia-agent/tests/`. They directly drive the
//! `LoopDetectorSet` to lock down the threshold (3
//! consecutive identical calls) and the negative case
//! (alternating args).
//!
//! [`AgentEvent`]: crate::events::AgentEvent

#[cfg(test)]
mod doom_loop_tests {
    use synthia_guardian::{LoopAction, LoopDetectorSet, LoopStatus};

    /// Direct unit test for the doom-loop signal surfaced by the unified
    /// `LoopDetectorSet`. Mirrors opencode's `doom_loop` category.
    #[test]
    fn doom_loop_triggers_require_permission_after_three_identical_calls() {
        let mut det = LoopDetectorSet::new();
        let tool = "read_file";
        let args = r#"{"path":"foo.txt"}"#;

        // 1st call: GenericRepeat counter starts at 1 → Ok.
        let (s1, a1) = det.check(tool, args, 0);
        assert_eq!(s1, LoopStatus::Ok);
        assert_eq!(a1, None);

        // 2nd call: counter at 2 → Warning (one shy of block threshold).
        let (s2, a2) = det.check(tool, args, 1);
        assert_eq!(s2, LoopStatus::Warning);
        assert_eq!(a2, Some(LoopAction::Warn));

        // 3rd identical call → DoomLoop fires (3 consecutive).
        let (s3, a3) = det.check(tool, args, 2);
        assert_eq!(s3, LoopStatus::Detected);
        assert_eq!(a3, Some(LoopAction::RequirePermission));
    }

    /// Doom loop does NOT fire when consecutive calls differ in args —
    /// only strict 3-consecutive-identity is a doom loop.
    #[test]
    fn doom_loop_does_not_fire_on_alternating_args() {
        let mut det = LoopDetectorSet::new();
        for i in 0..5 {
            let args = format!(r#"{{"i":{i}}}"#);
            let (s, a) = det.check("read_file", &args, i);
            assert_eq!(s, LoopStatus::Ok);
            assert_eq!(a, None);
        }
    }
}
