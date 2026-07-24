//! Combined loop detector set: runs all five detectors in fixed order
//! and short-circuits on the first non-`Ok` result.
//!
//! # Detection Order
//!
//! 1. [`DoomLoopDetector`](super::doom_loop::DoomLoopDetector)
//! 2. [`GenericRepeatDetector`](super::generic_repeat::GenericRepeatDetector)
//! 3. [`PingPongDetector`](super::ping_pong::PingPongDetector)
//! 4. [`PollNoProgressDetector`](super::poll_no_progress::PollNoProgressDetector)
//!    — checked via [`Self::check_poll_result`], not the main flow.
//! 5. [`GlobalCircuitDetector`](super::global_circuit::GlobalCircuitDetector)

use tracing::warn;

use super::{
    doom_loop::DoomLoopDetector,
    generic_repeat::GenericRepeatDetector,
    global_circuit::GlobalCircuitDetector,
    hash::hash_tool_args,
    ping_pong::PingPongDetector,
    poll_no_progress::PollNoProgressDetector,
};
use crate::types::{LoopAction, LoopStatus};

/// Combined loop detector set with five independent detectors.
///
/// Detectors are evaluated in this order; the first non-`Ok` result
/// short-circuits the rest. The returned `(LoopStatus, Option<LoopAction>)`
/// pair tells the caller both the severity and the recommended response.
pub struct LoopDetectorSet {
    doom_loop: DoomLoopDetector,
    generic_repeat: GenericRepeatDetector,
    ping_pong: PingPongDetector,
    poll_no_progress: PollNoProgressDetector,
    global_circuit: GlobalCircuitDetector,
}

impl LoopDetectorSet {
    /// Creates a new loop detector set with default thresholds.
    #[must_use]
    pub fn new() -> Self {
        Self {
            doom_loop: DoomLoopDetector::new(),
            generic_repeat: GenericRepeatDetector::new(),
            ping_pong: PingPongDetector::new(),
            poll_no_progress: PollNoProgressDetector::new(),
            global_circuit: GlobalCircuitDetector::new(),
        }
    }

    /// Checks a tool call against all five detectors and returns
    /// `(LoopStatus, Option<LoopAction>)`. The first detector that
    /// produces a non-`Ok` status determines the result.
    pub fn check(
        &mut self,
        tool_name: &str,
        args_json: &str,
        iteration: usize,
    ) -> (LoopStatus, Option<LoopAction>) {
        // 1. DoomLoop: 3 consecutive identical calls.
        if self.doom_loop.check(tool_name, args_json) {
            warn!(tool = %tool_name, "Doom loop detected (3 identical calls)");
            return (LoopStatus::Detected, Some(LoopAction::RequirePermission));
        }

        // 2. GenericRepeat: cumulative counter per (tool, args).
        let (tool_id, args_hash) = hash_tool_args(tool_name, args_json);
        match self.generic_repeat.check(tool_id, args_hash) {
            LoopStatus::Ok => {}
            LoopStatus::Warning => {
                warn!(tool = %tool_name, "GenericRepeat warning (near threshold)");
                return (LoopStatus::Warning, Some(LoopAction::Warn));
            }
            LoopStatus::Detected => {
                warn!(tool = %tool_name, "GenericRepeat detected (block)");
                return (LoopStatus::Detected, Some(LoopAction::Block));
            }
        }

        // 3. PingPong: A-B-A-B alternation.
        if self.ping_pong.check(tool_name) == LoopStatus::Detected {
            return (LoopStatus::Detected, Some(LoopAction::Block));
        }

        // 4. PollNoProgress: handled by check_poll_result, not here.

        // 5. GlobalCircuit: iteration cap.
        if self.global_circuit.check(iteration) == LoopStatus::Detected {
            return (LoopStatus::Detected, Some(LoopAction::HardBlock));
        }

        (LoopStatus::Ok, None)
    }

    /// Checks a poll result for no-progress patterns. Independent of the
    /// main `check()` flow; callers invoke this after observing that a tool
    /// returned without making progress.
    pub fn check_poll_result(&mut self, result: &str) -> LoopStatus {
        self.poll_no_progress.check(result)
    }

    /// Resets all detectors. Use after a `reset` of the agent loop.
    pub fn reset(&mut self) {
        self.doom_loop.reset();
        self.generic_repeat.reset();
        self.ping_pong.reset();
        self.poll_no_progress.reset();
        self.global_circuit.reset();
    }
}

impl Default for LoopDetectorSet {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loop_detector_set_ok() {
        let mut set = LoopDetectorSet::new();
        let (status, action) = set.check("read_file", "{}", 0);
        assert_eq!(status, LoopStatus::Ok);
        assert_eq!(action, None);
    }

    #[test]
    fn loop_detector_set_doom_loop_returns_require_permission() {
        let mut set = LoopDetectorSet::new();
        let _ = set.check("tool", "{}", 0);
        let _ = set.check("tool", "{}", 1);
        let (status, action) = set.check("tool", "{}", 2);
        assert_eq!(status, LoopStatus::Detected);
        assert_eq!(action, Some(LoopAction::RequirePermission));
    }

    #[test]
    fn loop_detector_set_generic_repeat_returns_block() {
        let mut set = LoopDetectorSet::new();
        // Avoid DoomLoop (which fires on 3 consecutive identical) by
        // interspersing different args. GenericRepeat accumulates per
        // (tool, args_hash) and triggers on the 3rd occurrence.
        let _ = set.check("tool", "x", 0);
        let _ = set.check("tool", "a", 1); // first 'a'
        let _ = set.check("tool", "y", 2);
        let (s, a) = set.check("tool", "a", 3); // 2nd 'a' → Warning
        assert_eq!(s, LoopStatus::Warning);
        assert_eq!(a, Some(LoopAction::Warn));
        let _ = set.check("tool", "z", 4);
        let (s, a) = set.check("tool", "a", 5); // 3rd 'a' → Detected
        assert_eq!(s, LoopStatus::Detected);
        assert_eq!(a, Some(LoopAction::Block));
    }

    #[test]
    fn loop_detector_set_ping_pong_returns_block() {
        let mut set = LoopDetectorSet::new();
        // Avoid DoomLoop by using different args each call.
        let _ = set.check("a", "1", 0);
        let _ = set.check("b", "2", 1);
        let _ = set.check("a", "3", 2);
        let (s, a) = set.check("b", "4", 3);
        assert_eq!(s, LoopStatus::Detected);
        assert_eq!(a, Some(LoopAction::Block));
    }

    #[test]
    fn loop_detector_set_poll_no_progress_independent() {
        let mut set = LoopDetectorSet::new();
        for _ in 0..10 - 1 {
            assert_eq!(set.check_poll_result("same"), LoopStatus::Ok);
        }
        assert_eq!(set.check_poll_result("same"), LoopStatus::Detected);
    }

    #[test]
    fn loop_detector_set_global_circuit_returns_hard_block() {
        let mut set = LoopDetectorSet::new();
        let (s, a) = set.check("tool", "x", 30);
        assert_eq!(s, LoopStatus::Detected);
        assert_eq!(a, Some(LoopAction::HardBlock));
    }

    #[test]
    fn loop_detector_set_reset_clears_all() {
        let mut set = LoopDetectorSet::new();
        // Build up some state.
        let _ = set.check("tool", "a", 0);
        let _ = set.check("tool", "a", 1);
        set.reset();
        // After reset, identical fresh start.
        let (s, a) = set.check("tool", "a", 0);
        assert_eq!(s, LoopStatus::Ok);
        assert_eq!(a, None);
    }

    #[test]
    fn loop_detector_set_doom_loop_short_circuits_others() {
        // Verify DoomLoop is evaluated first and prevents other detectors
        // from being incremented.
        let mut set = LoopDetectorSet::new();
        let _ = set.check("tool", "{}", 0);
        let _ = set.check("tool", "{}", 1);
        // 3rd identical → DoomLoop fires.
        let _ = set.check("tool", "{}", 2);
        // 4th identical: DoomLoop window slid forward, still detected.
        let (s, a) = set.check("tool", "{}", 3);
        assert_eq!(s, LoopStatus::Detected);
        assert_eq!(a, Some(LoopAction::RequirePermission));
    }
}
