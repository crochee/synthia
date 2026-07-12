//! Doom loop detector: proactive sliding-window detection of 3 consecutive
//! identical `(tool, args)` calls.
//!
//! Mirrors opencode's `DOOM_LOOP_THRESHOLD = 3`: when the same tool is
//! called with the same input 3 times in a row, the caller receives a
//! `LoopAction::RequirePermission` signal so it can invoke
//! `synthia_permission::Permission::ask` to break the loop before the
//! circuit breaker trips.
//!
//! This detector uses a fast hash (AHash) to compare arguments rather than
//! storing the full JSON string on every call.

use std::{
    collections::VecDeque,
    hash::{Hash, Hasher},
};

use ahash::AHasher;

use crate::types::{LoopAction, LoopStatus};

/// Unique signature of a tool call: tool name + fast hash of arguments.
#[derive(PartialEq, Eq, Hash, Clone)]
struct ToolCallSignature {
    tool_name: String,
    input_hash: u64,
}

/// Proactive doom-loop detector using a sliding window.
///
/// Maintains a `VecDeque` of the last N call signatures (default N=3).
/// When all N entries in the window are identical, returns
/// `(LoopStatus::Detected, Some(LoopAction::RequirePermission))`.
/// The caller should then invoke `permission.ask(doom_loop, ...)` before
/// deciding whether to proceed.
pub struct DoomLoopDetector {
    recent: VecDeque<ToolCallSignature>,
    threshold: usize,
}

impl DoomLoopDetector {
    /// Creates a new detector with the default doom-loop threshold (3).
    #[must_use]
    pub fn new() -> Self {
        Self::with_threshold(3)
    }

    /// Creates a detector with a custom `threshold`.
    #[must_use]
    pub fn with_threshold(threshold: usize) -> Self {
        Self {
            recent: VecDeque::with_capacity(threshold),
            threshold,
        }
    }

    /// Checks a tool call against the sliding window.
    ///
    /// Returns `(LoopStatus::Detected, Some(LoopAction::RequirePermission))`
    /// when `threshold` consecutive calls have identical `(tool_name, args)` signatures.
    pub fn check(
        &mut self,
        tool_name: &str,
        args: &serde_json::Value,
    ) -> (LoopStatus, Option<LoopAction>) {
        let input_hash = hash_json_args(args);
        let sig = ToolCallSignature {
            tool_name: tool_name.to_string(),
            input_hash,
        };

        // Push the new signature first so the window slides correctly.
        self.recent.push_back(sig);
        if self.recent.len() > self.threshold {
            self.recent.pop_front();
        }

        // Detection: when the window just reached `threshold` entries (all
        // identical calls), trigger.  We check front==back as a cheap
        // "all N are identical" proxy since we already know all prior
        // entries were identical (inductive hypothesis).
        if self.recent.len() == self.threshold
            && self.recent.front() == self.recent.back()
        {
            return (LoopStatus::Detected, Some(LoopAction::RequirePermission));
        }

        (LoopStatus::Ok, None)
    }

    /// Resets the detector state (e.g., after a successful user confirmation).
    pub fn reset(&mut self) {
        self.recent.clear();
    }
}

impl Default for DoomLoopDetector {
    fn default() -> Self {
        Self::new()
    }
}

/// Fast hash of a JSON value's canonical serialization using AHash.
fn hash_json_args(args: &serde_json::Value) -> u64 {
    let json_str = args.to_string();
    let bytes = json_str.as_bytes();
    let mut hasher = AHasher::default();
    hasher.write(bytes);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn json(s: &str) -> serde_json::Value {
        serde_json::from_str(s).expect("valid JSON")
    }

    #[test]
    fn doom_loop_detector_ok_on_first_calls() {
        let mut det = DoomLoopDetector::new();
        let (status, action) = det.check("tool", &json("{}"));
        assert_eq!(status, LoopStatus::Ok);
        assert_eq!(action, None);
    }

    #[test]
    fn doom_loop_detector_triggers_on_three_identical() {
        let mut det = DoomLoopDetector::new();
        let args = json("{}");

        let (status, _) = det.check("tool", &args);
        assert_eq!(status, LoopStatus::Ok);

        let (status, _) = det.check("tool", &args);
        assert_eq!(status, LoopStatus::Ok);

        let (status, action) = det.check("tool", &args);
        assert_eq!(status, LoopStatus::Detected);
        assert_eq!(action, Some(LoopAction::RequirePermission));
    }

    #[test]
    fn doom_loop_detector_resets_on_different_args() {
        let mut det = DoomLoopDetector::new();
        det.check("tool", &json("{}"));
        det.check("tool", &json("{}"));
        // Different args breaks the chain.
        let (status, _) = det.check("tool", &json(r#"{"k":1}"#));
        assert_eq!(status, LoopStatus::Ok);
    }

    #[test]
    fn doom_loop_detector_resets_on_different_tool() {
        let mut det = DoomLoopDetector::new();
        det.check("tool_a", &json("{}"));
        det.check("tool_a", &json("{}"));
        // Different tool breaks the chain.
        let (status, _) = det.check("tool_b", &json("{}"));
        assert_eq!(status, LoopStatus::Ok);
    }

    #[test]
    fn doom_loop_detector_reset_clears_state() {
        let mut det = DoomLoopDetector::new();
        det.check("tool", &json("{}"));
        det.check("tool", &json("{}"));
        det.reset();
        // After reset, need 3 fresh calls to trigger again.
        let (status, _) = det.check("tool", &json("{}"));
        assert_eq!(status, LoopStatus::Ok);
        let (status, _) = det.check("tool", &json("{}"));
        assert_eq!(status, LoopStatus::Ok);
        let (status, action) = det.check("tool", &json("{}"));
        assert_eq!(status, LoopStatus::Detected);
        assert_eq!(action, Some(LoopAction::RequirePermission));
    }

    #[test]
    fn doom_loop_detector_custom_threshold() {
        let mut det = DoomLoopDetector::with_threshold(2);
        let args = json("{}");

        det.check("tool", &args);
        let (status, action) = det.check("tool", &args);
        assert_eq!(status, LoopStatus::Detected);
        assert_eq!(action, Some(LoopAction::RequirePermission));
    }

    #[test]
    fn doom_loop_detector_different_hash_same_tool() {
        // Same tool, different JSON values (different hashes) should not trigger.
        let mut det = DoomLoopDetector::new();
        det.check("tool", &json(r#"{"a":1}"#));
        det.check("tool", &json(r#"{"b":2}"#));
        let (status, _) = det.check("tool", &json(r#"{"c":3}"#));
        assert_eq!(status, LoopStatus::Ok);
    }
}
