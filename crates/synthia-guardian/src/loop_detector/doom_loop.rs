//! Doom loop detector: 3 consecutive identical `(tool, args)` calls.
//!
//! Mirrors opencode's `DOOM_LOOP_THRESHOLD = 3`: when the same tool is
//! called with the same input 3 times in a row, the caller is given a
//! `LoopAction::RequirePermission` signal so it can invoke
//! `synthia_permission::Permission::ask` to break the loop.

use std::collections::VecDeque;

const DOOM_LOOP_WINDOW: usize = 3;

/// Detects three consecutive identical `(tool, args)` calls.
///
/// Maintains a sliding window of the last 3 `(tool_name, args_json)`
/// pairs. When all 3 match, returns `true` from [`Self::check`].
pub(crate) struct DoomLoopDetector {
    recent_calls: VecDeque<(String, String)>,
    window_size: usize,
}

impl DoomLoopDetector {
    pub(crate) fn new() -> Self {
        Self {
            recent_calls: VecDeque::with_capacity(DOOM_LOOP_WINDOW),
            window_size: DOOM_LOOP_WINDOW,
        }
    }

    /// Records a tool call and returns `true` if the last 3 calls were identical.
    pub(crate) fn check(&mut self, tool_name: &str, args_json: &str) -> bool {
        self.recent_calls
            .push_back((tool_name.to_string(), args_json.to_string()));
        while self.recent_calls.len() > self.window_size {
            self.recent_calls.pop_front();
        }
        if self.recent_calls.len() < self.window_size {
            return false;
        }
        let last3: Vec<_> = self.recent_calls.iter().rev().take(3).collect();
        last3[0] == last3[1] && last3[1] == last3[2]
    }

    pub(crate) fn reset(&mut self) {
        self.recent_calls.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doom_loop_triggers_on_three_identical() {
        let mut det = DoomLoopDetector::new();
        assert!(!det.check("tool", "{}"));
        assert!(!det.check("tool", "{}"));
        assert!(det.check("tool", "{}"));
    }

    #[test]
    fn doom_loop_resets_on_different_args() {
        let mut det = DoomLoopDetector::new();
        det.check("tool", "{}");
        det.check("tool", "{}");
        assert!(!det.check("tool", r#"{"k":1}"#));
        // Now we need 2 more identical to trigger again.
        assert!(!det.check("tool", r#"{"k":1}"#));
        assert!(det.check("tool", r#"{"k":1}"#));
    }

    #[test]
    fn doom_loop_resets_on_different_tool() {
        let mut det = DoomLoopDetector::new();
        det.check("tool_a", "{}");
        det.check("tool_a", "{}");
        assert!(!det.check("tool_b", "{}"));
    }

    #[test]
    fn doom_loop_reset_clears_state() {
        let mut det = DoomLoopDetector::new();
        det.check("tool", "{}");
        det.check("tool", "{}");
        det.reset();
        // After reset, the first 2 calls do not trigger (window < 3).
        assert!(!det.check("tool", "{}"));
        assert!(!det.check("tool", "{}"));
    }
}
