//! Ping-pong detector: A-B-A-B alternation of two distinct tools.

use tracing::warn;

use crate::types::LoopStatus;

const PING_PONG_PATTERN_LENGTH: usize = 4;
const PING_PONG_HISTORY_MAX: usize = 20;
const PING_PONG_HISTORY_KEEP: usize = 10;

/// Detects when two tools are alternating in a cycle (A-B-A-B pattern).
pub(crate) struct PingPongDetector {
    recent_tools: Vec<String>,
}

impl PingPongDetector {
    pub(crate) fn new() -> Self {
        Self {
            recent_tools: Vec::new(),
        }
    }

    pub(crate) fn check(&mut self, tool_name: &str) -> LoopStatus {
        self.recent_tools.push(tool_name.to_string());

        if self.recent_tools.len() < PING_PONG_PATTERN_LENGTH {
            return LoopStatus::Ok;
        }

        let len = self.recent_tools.len();
        let a = &self.recent_tools[len - 4];
        let b = &self.recent_tools[len - 3];
        let c = &self.recent_tools[len - 2];
        let d = &self.recent_tools[len - 1];

        // Check for A-B-A-B pattern where A != B
        if a == c && b == d && a != b {
            warn!(tool_a = %a, tool_b = %b, "Ping-pong loop detected");
            return LoopStatus::Detected;
        }

        // Keep only recent history to bound memory usage
        if self.recent_tools.len() > PING_PONG_HISTORY_MAX {
            let drain_to = self.recent_tools.len() - PING_PONG_HISTORY_KEEP;
            self.recent_tools.drain(..drain_to);
        }

        LoopStatus::Ok
    }

    pub(crate) fn reset(&mut self) {
        self.recent_tools.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ping_pong_loop() {
        let mut det = PingPongDetector::new();
        det.check("tool_a");
        det.check("tool_b");
        det.check("tool_a");
        assert_eq!(det.check("tool_b"), LoopStatus::Detected);
    }

    #[test]
    fn ping_pong_no_loop_when_three_distinct() {
        let mut det = PingPongDetector::new();
        det.check("tool_a");
        det.check("tool_b");
        det.check("tool_c");
        assert_eq!(det.check("tool_d"), LoopStatus::Ok);
    }

    #[test]
    fn ping_pong_no_loop_same_tool_repeated() {
        let mut det = PingPongDetector::new();
        det.check("tool_a");
        det.check("tool_a");
        det.check("tool_a");
        assert_eq!(det.check("tool_a"), LoopStatus::Ok);
    }
}
