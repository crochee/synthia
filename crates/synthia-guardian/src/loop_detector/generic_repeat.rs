//! Generic repeat detector: O(1) counter per `(tool_id, args_hash)` pair.
//!
//! Replaces the previous `Vec<u64>` implementation which required an
//! O(N) scan per check. Counts live in a `HashMap`; lookups and updates
//! are amortized O(1) with zero per-call `String` allocation.

use std::collections::HashMap;

use crate::types::LoopStatus;

const GENERIC_REPEAT_BLOCK_THRESHOLD: u32 = 3;

/// O(1) detector for repeated `(tool, args)` calls.
pub(crate) struct GenericRepeatDetector {
    counts: HashMap<(u64, u64), u32>,
    block_threshold: u32,
}

impl GenericRepeatDetector {
    pub(crate) fn new() -> Self {
        Self {
            counts: HashMap::new(),
            block_threshold: GENERIC_REPEAT_BLOCK_THRESHOLD,
        }
    }

    /// Records a tool call and returns its current [`LoopStatus`].
    pub(crate) fn check(&mut self, tool_id: u64, args_hash: u64) -> LoopStatus {
        let entry = self.counts.entry((tool_id, args_hash)).or_insert(0);
        *entry = entry.saturating_add(1);
        let count = *entry;

        if count >= self.block_threshold {
            LoopStatus::Detected
        } else if count + 1 >= self.block_threshold {
            // One call away from block threshold → warn.
            LoopStatus::Warning
        } else {
            LoopStatus::Ok
        }
    }

    pub(crate) fn reset(&mut self) {
        self.counts.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loop_detector::hash::hash_tool_args;

    #[test]
    fn generic_repeat_uses_hashmap_o1() {
        let mut det = GenericRepeatDetector::new();
        let (tool_id, args_hash) = hash_tool_args("tool_a", "same_args");
        // 1000 calls should be very fast (O(1) per call).
        let start = std::time::Instant::now();
        for _ in 0..1000 {
            det.check(tool_id, args_hash);
        }
        assert!(
            start.elapsed().as_micros() < 10_000,
            "1000 calls should complete in < 10ms, took {}µs",
            start.elapsed().as_micros()
        );
    }

    #[test]
    fn generic_repeat_warns_one_below_threshold() {
        let mut det = GenericRepeatDetector::new();
        let (tool_id, args_hash) = hash_tool_args("tool_a", "same");
        assert_eq!(det.check(tool_id, args_hash), LoopStatus::Ok);
        // 2nd call: count 2 → Warning (count + 1 == block_threshold 3).
        assert_eq!(det.check(tool_id, args_hash), LoopStatus::Warning);
    }

    #[test]
    fn generic_repeat_detects_at_threshold() {
        let mut det = GenericRepeatDetector::new();
        let (tool_id, args_hash) = hash_tool_args("tool_a", "same");
        det.check(tool_id, args_hash); // count 1
        det.check(tool_id, args_hash); // count 2 (Warning)
        assert_eq!(det.check(tool_id, args_hash), LoopStatus::Detected); // count 3
    }

    #[test]
    fn generic_repeat_distinguishes_by_args() {
        let mut det = GenericRepeatDetector::new();
        let (tid, ah1) = hash_tool_args("tool_a", "args_1");
        let (_, ah2) = hash_tool_args("tool_a", "args_2");
        det.check(tid, ah1);
        det.check(tid, ah1);
        // Different args → fresh count.
        assert_eq!(det.check(tid, ah2), LoopStatus::Ok);
    }

    #[test]
    fn generic_repeat_distinguishes_by_tool() {
        let mut det = GenericRepeatDetector::new();
        let (tid1, ah) = hash_tool_args("tool_a", "args");
        let (tid2, _) = hash_tool_args("tool_b", "args");
        det.check(tid1, ah);
        det.check(tid1, ah);
        // Different tool → fresh count.
        assert_eq!(det.check(tid2, ah), LoopStatus::Ok);
    }

    #[test]
    fn generic_repeat_reset_clears_state() {
        let mut det = GenericRepeatDetector::new();
        let (tid, ah) = hash_tool_args("tool", "args");
        det.check(tid, ah);
        det.check(tid, ah);
        det.reset();
        assert_eq!(det.check(tid, ah), LoopStatus::Ok);
    }
}
