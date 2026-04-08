use std::collections::{HashMap, VecDeque};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationPattern {
    pub tool_name: String,
    pub args_hash: u64,
    pub timestamp: DateTime<Utc>,
    pub outcome: Outcome,
    pub result_hash: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Outcome {
    Success,
    Failure,
    Pending,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LoopType {
    GenericRepeat,
    PollNoProgress,
    PingPong,
    CircuitBreaker,
}

#[derive(Debug)]
pub struct LoopDetector {
    history: VecDeque<OperationPattern>,
    max_history: usize,
    max_loops: usize,
    circuit_breaker_threshold: usize,
    consecutive_failures: usize,
}

impl LoopDetector {
    pub fn new(max_history: usize, max_loops: usize) -> Self {
        Self {
            history: VecDeque::new(),
            max_history,
            max_loops,
            circuit_breaker_threshold: 30,
            consecutive_failures: 0,
        }
    }

    pub fn with_circuit_breaker(
        max_history: usize,
        max_loops: usize,
        circuit_breaker_threshold: usize,
    ) -> Self {
        Self {
            history: VecDeque::new(),
            max_history,
            max_loops,
            circuit_breaker_threshold,
            consecutive_failures: 0,
        }
    }

    pub fn record(&mut self, pattern: OperationPattern) {
        if self.history.len() >= self.max_history {
            self.history.pop_front();
        }

        match pattern.outcome {
            Outcome::Failure => self.consecutive_failures += 1,
            Outcome::Success => self.consecutive_failures = 0,
            Outcome::Pending => {}
        }

        self.history.push_back(pattern);
    }

    pub fn detect_loop(&self) -> Option<LoopDetection> {
        if let Some(detection) = self.detect_generic_repeat() {
            return Some(detection);
        }

        if let Some(detection) = self.detect_poll_no_progress() {
            return Some(detection);
        }

        if let Some(detection) = self.detect_ping_pong() {
            return Some(detection);
        }

        if let Some(detection) = self.detect_circuit_breaker() {
            return Some(detection);
        }

        None
    }

    fn detect_generic_repeat(&self) -> Option<LoopDetection> {
        let mut pattern_counts: HashMap<(String, u64), Vec<usize>> =
            HashMap::new();
        for (i, p) in self.history.iter().enumerate() {
            pattern_counts
                .entry((p.tool_name.clone(), p.args_hash))
                .or_default()
                .push(i);
        }

        for ((tool, args_hash), indices) in pattern_counts {
            if indices.len() >= self.max_loops {
                return Some(LoopDetection {
                    loop_type: LoopType::GenericRepeat,
                    tool_name: tool,
                    args_hash,
                    occurrences: indices.len(),
                    first_seen: indices[0],
                    last_seen: indices[indices.len() - 1],
                });
            }
        }

        None
    }

    fn detect_poll_no_progress(&self) -> Option<LoopDetection> {
        let poll_tools = ["Read", "Glob", "Grep", "ListDirectory"];
        let mut poll_results: HashMap<String, Vec<(usize, u64)>> =
            HashMap::new();

        for (i, p) in self.history.iter().enumerate() {
            if poll_tools.contains(&p.tool_name.as_str())
                && let Some(hash) = p.result_hash
            {
                poll_results
                    .entry(p.tool_name.clone())
                    .or_default()
                    .push((i, hash));
            }
        }

        for (tool, results) in poll_results {
            if results.len() < 3 {
                continue;
            }

            let mut same_result_count = 1;
            let first_hash = results[0].1;

            for (_, hash) in results.iter().skip(1) {
                if *hash == first_hash {
                    same_result_count += 1;
                } else {
                    break;
                }
            }

            if same_result_count >= 3 {
                return Some(LoopDetection {
                    loop_type: LoopType::PollNoProgress,
                    tool_name: tool,
                    args_hash: first_hash,
                    occurrences: same_result_count,
                    first_seen: results[0].0,
                    last_seen: results[same_result_count - 1].0,
                });
            }
        }

        None
    }

    fn detect_ping_pong(&self) -> Option<LoopDetection> {
        if self.history.len() < 4 {
            return None;
        }

        for i in 0..self.history.len().saturating_sub(3) {
            let tool_a = &self.history[i].tool_name;
            let tool_b = &self.history[i + 1].tool_name;
            let tool_c = &self.history[i + 2].tool_name;
            let tool_d = &self.history[i + 3].tool_name;

            if tool_a == tool_c && tool_b == tool_d && tool_a != tool_b {
                return Some(LoopDetection {
                    loop_type: LoopType::PingPong,
                    tool_name: format!("{tool_a} <-> {tool_b}"),
                    args_hash: 0,
                    occurrences: 2,
                    first_seen: i,
                    last_seen: i + 3,
                });
            }
        }

        None
    }

    fn detect_circuit_breaker(&self) -> Option<LoopDetection> {
        if self.consecutive_failures >= self.circuit_breaker_threshold {
            let last_failure = self
                .history
                .iter()
                .enumerate()
                .rev()
                .find(|(_, p)| p.outcome == Outcome::Failure);

            if let Some((idx, pattern)) = last_failure {
                return Some(LoopDetection {
                    loop_type: LoopType::CircuitBreaker,
                    tool_name: pattern.tool_name.clone(),
                    args_hash: pattern.args_hash,
                    occurrences: self.consecutive_failures,
                    first_seen: idx
                        .saturating_sub(self.consecutive_failures - 1),
                    last_seen: idx,
                });
            }
        }

        None
    }

    pub fn reset_circuit_breaker(&mut self) {
        self.consecutive_failures = 0;
    }

    pub fn consecutive_failures(&self) -> usize {
        self.consecutive_failures
    }

    pub fn history_len(&self) -> usize {
        self.history.len()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopDetection {
    pub loop_type: LoopType,
    pub tool_name: String,
    pub args_hash: u64,
    pub occurrences: usize,
    pub first_seen: usize,
    pub last_seen: usize,
}

impl std::fmt::Display for LoopDetection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.loop_type {
            LoopType::GenericRepeat => write!(
                f,
                "Tool '{}' repeated {} times with same arguments",
                self.tool_name, self.occurrences
            ),
            LoopType::PollNoProgress => write!(
                f,
                "Tool '{}' returned same result {} times - no progress",
                self.tool_name, self.occurrences
            ),
            LoopType::PingPong => write!(
                f,
                "Ping-pong pattern detected: {} ({} times)",
                self.tool_name, self.occurrences
            ),
            LoopType::CircuitBreaker => write!(
                f,
                "Circuit breaker triggered: {} consecutive failures",
                self.occurrences
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // =============================================================================
    // detect_generic_repeat Tests
    // =============================================================================

    #[test]
    fn test_detect_generic_repeat_exactly_max_loops() {
        let mut detector = LoopDetector::new(10, 3);

        // Exactly 3 occurrences should trigger detection
        for _ in 0..3 {
            detector.record(OperationPattern {
                tool_name: "Read".to_string(),
                args_hash: 100,
                timestamp: chrono::Utc::now(),
                outcome: Outcome::Success,
                result_hash: None,
            });
        }

        let detection = detector.detect_generic_repeat();
        assert!(
            detection.is_some(),
            "Should detect generic repeat at exactly max_loops"
        );
        let detection = detection.unwrap();
        assert_eq!(detection.loop_type, LoopType::GenericRepeat);
        assert_eq!(detection.occurrences, 3);
    }

    #[test]
    fn test_detect_generic_repeat_below_threshold() {
        let mut detector = LoopDetector::new(10, 3);

        // Only 2 occurrences should NOT trigger detection
        for _ in 0..2 {
            detector.record(OperationPattern {
                tool_name: "Read".to_string(),
                args_hash: 100,
                timestamp: chrono::Utc::now(),
                outcome: Outcome::Success,
                result_hash: None,
            });
        }

        let detection = detector.detect_generic_repeat();
        assert!(
            detection.is_none(),
            "Should not detect generic repeat below threshold"
        );
    }

    #[test]
    fn test_detect_generic_repeat_different_args_not_duplicated() {
        let mut detector = LoopDetector::new(10, 3);

        // Same tool but different args should not trigger
        for i in 0..3 {
            detector.record(OperationPattern {
                tool_name: "Read".to_string(),
                args_hash: i as u64,
                timestamp: chrono::Utc::now(),
                outcome: Outcome::Success,
                result_hash: None,
            });
        }

        let detection = detector.detect_generic_repeat();
        assert!(
            detection.is_none(),
            "Different args should not trigger repeat detection"
        );
    }

    #[test]
    fn test_detect_generic_repeat_multiple_tools() {
        let mut detector = LoopDetector::new(10, 3);

        // Record 2 different tools with same args 3 times each
        for _ in 0..3 {
            detector.record(OperationPattern {
                tool_name: "Read".to_string(),
                args_hash: 100,
                timestamp: chrono::Utc::now(),
                outcome: Outcome::Success,
                result_hash: None,
            });
            detector.record(OperationPattern {
                tool_name: "Write".to_string(),
                args_hash: 200,
                timestamp: chrono::Utc::now(),
                outcome: Outcome::Success,
                result_hash: None,
            });
        }

        let detection = detector.detect_generic_repeat();
        assert!(
            detection.is_some(),
            "Should detect one of the repeated patterns"
        );
    }

    // =============================================================================
    // detect_poll_no_progress Tests
    // =============================================================================

    #[test]
    fn test_detect_poll_no_progress_exactly_three_same_results() {
        let mut detector = LoopDetector::new(10, 3);

        // Read tool with same result hash 3 times
        let result_hash: u64 = 99999;
        for _ in 0..3 {
            detector.record(OperationPattern {
                tool_name: "Read".to_string(),
                args_hash: 111,
                timestamp: chrono::Utc::now(),
                outcome: Outcome::Success,
                result_hash: Some(result_hash),
            });
        }

        let detection = detector.detect_poll_no_progress();
        assert!(detection.is_some(), "Should detect poll no progress");
        let detection = detection.unwrap();
        assert_eq!(detection.loop_type, LoopType::PollNoProgress);
        assert_eq!(detection.occurrences, 3);
    }

    #[test]
    fn test_detect_poll_no_progress_only_poll_tools() {
        let mut detector = LoopDetector::new(10, 3);

        // Non-poll tools should not trigger poll detection
        for _ in 0..3 {
            detector.record(OperationPattern {
                tool_name: "Write".to_string(),
                args_hash: 111,
                timestamp: chrono::Utc::now(),
                outcome: Outcome::Success,
                result_hash: Some(99999),
            });
        }

        let detection = detector.detect_poll_no_progress();
        assert!(
            detection.is_none(),
            "Non-poll tools should not trigger poll detection"
        );
    }

    #[test]
    fn test_detect_poll_no_progress_varying_results() {
        let mut detector = LoopDetector::new(10, 3);

        // Same poll tool but different result hashes
        for i in 0..3 {
            detector.record(OperationPattern {
                tool_name: "Read".to_string(),
                args_hash: 111,
                timestamp: chrono::Utc::now(),
                outcome: Outcome::Success,
                result_hash: Some(i as u64),
            });
        }

        let detection = detector.detect_poll_no_progress();
        assert!(detection.is_none(), "Varying results should not trigger");
    }

    #[test]
    fn test_detect_poll_no_progress_all_poll_tools() {
        let poll_tools = ["Read", "Glob", "Grep", "ListDirectory"];

        for tool_name in poll_tools {
            let mut detector = LoopDetector::new(10, 3);
            let result_hash: u64 = 88888;

            for _ in 0..3 {
                detector.record(OperationPattern {
                    tool_name: tool_name.to_string(),
                    args_hash: 111,
                    timestamp: chrono::Utc::now(),
                    outcome: Outcome::Success,
                    result_hash: Some(result_hash),
                });
            }

            let detection = detector.detect_poll_no_progress();
            assert!(
                detection.is_some(),
                "{tool_name} should be detected as poll tool"
            );
        }
    }

    #[test]
    fn test_detect_poll_no_progress_with_pending_outcome() {
        let mut detector = LoopDetector::new(10, 3);

        let result_hash: u64 = 77777;
        for _ in 0..3 {
            detector.record(OperationPattern {
                tool_name: "Read".to_string(),
                args_hash: 111,
                timestamp: chrono::Utc::now(),
                outcome: Outcome::Pending, // Pending outcome
                result_hash: Some(result_hash),
            });
        }

        // Poll detection should still work with Pending outcome
        let detection = detector.detect_poll_no_progress();
        assert!(detection.is_some());
    }

    // =============================================================================
    // detect_ping_pong Tests
    // =============================================================================

    #[test]
    fn test_detect_ping_pong_basic_pattern() {
        let mut detector = LoopDetector::new(10, 3);

        // Read -> Write -> Read -> Write pattern
        detector.record(OperationPattern {
            tool_name: "Read".to_string(),
            args_hash: 1,
            timestamp: chrono::Utc::now(),
            outcome: Outcome::Success,
            result_hash: None,
        });
        detector.record(OperationPattern {
            tool_name: "Write".to_string(),
            args_hash: 2,
            timestamp: chrono::Utc::now(),
            outcome: Outcome::Success,
            result_hash: None,
        });
        detector.record(OperationPattern {
            tool_name: "Read".to_string(),
            args_hash: 1,
            timestamp: chrono::Utc::now(),
            outcome: Outcome::Success,
            result_hash: None,
        });
        detector.record(OperationPattern {
            tool_name: "Write".to_string(),
            args_hash: 2,
            timestamp: chrono::Utc::now(),
            outcome: Outcome::Success,
            result_hash: None,
        });

        let detection = detector.detect_ping_pong();
        assert!(detection.is_some(), "Should detect ping-pong pattern");
        let detection = detection.unwrap();
        assert_eq!(detection.loop_type, LoopType::PingPong);
        assert!(detection.tool_name.contains("Read"));
        assert!(detection.tool_name.contains("Write"));
    }

    #[test]
    fn test_detect_ping_pong_insufficient_history() {
        let mut detector = LoopDetector::new(10, 3);

        // Only 3 items - not enough for ping-pong (needs 4)
        detector.record(OperationPattern {
            tool_name: "Read".to_string(),
            args_hash: 1,
            timestamp: chrono::Utc::now(),
            outcome: Outcome::Success,
            result_hash: None,
        });
        detector.record(OperationPattern {
            tool_name: "Write".to_string(),
            args_hash: 2,
            timestamp: chrono::Utc::now(),
            outcome: Outcome::Success,
            result_hash: None,
        });
        detector.record(OperationPattern {
            tool_name: "Read".to_string(),
            args_hash: 1,
            timestamp: chrono::Utc::now(),
            outcome: Outcome::Success,
            result_hash: None,
        });

        let detection = detector.detect_ping_pong();
        assert!(
            detection.is_none(),
            "Insufficient history should not trigger ping-pong"
        );
    }

    #[test]
    fn test_detect_ping_pong_same_tool() {
        let mut detector = LoopDetector::new(10, 3);

        // A -> A -> A -> A should NOT be ping-pong (needs A != B)
        for _ in 0..4 {
            detector.record(OperationPattern {
                tool_name: "Read".to_string(),
                args_hash: 1,
                timestamp: chrono::Utc::now(),
                outcome: Outcome::Success,
                result_hash: None,
            });
        }

        let detection = detector.detect_ping_pong();
        assert!(
            detection.is_none(),
            "Same tool should not trigger ping-pong"
        );
    }

    // =============================================================================
    // detect_circuit_breaker Tests
    // =============================================================================

    #[test]
    fn test_detect_circuit_breaker_exactly_threshold() {
        let mut detector = LoopDetector::with_circuit_breaker(10, 3, 3);

        for _ in 0..3 {
            detector.record(OperationPattern {
                tool_name: "exec".to_string(),
                args_hash: 1,
                timestamp: chrono::Utc::now(),
                outcome: Outcome::Failure,
                result_hash: None,
            });
        }

        let detection = detector.detect_circuit_breaker();
        assert!(
            detection.is_some(),
            "Should detect circuit breaker at threshold"
        );
        let detection = detection.unwrap();
        assert_eq!(detection.loop_type, LoopType::CircuitBreaker);
        assert_eq!(detection.occurrences, 3);
    }

    #[test]
    fn test_detect_circuit_breaker_below_threshold() {
        let mut detector = LoopDetector::with_circuit_breaker(10, 3, 5);

        // Only 4 failures, threshold is 5
        for _ in 0..4 {
            detector.record(OperationPattern {
                tool_name: "exec".to_string(),
                args_hash: 1,
                timestamp: chrono::Utc::now(),
                outcome: Outcome::Failure,
                result_hash: None,
            });
        }

        let detection = detector.detect_circuit_breaker();
        assert!(
            detection.is_none(),
            "Below threshold should not trigger circuit breaker"
        );
    }

    #[test]
    fn test_detect_circuit_breaker_success_resets() {
        let mut detector = LoopDetector::with_circuit_breaker(10, 3, 3);

        // 2 failures
        for _ in 0..2 {
            detector.record(OperationPattern {
                tool_name: "exec".to_string(),
                args_hash: 1,
                timestamp: chrono::Utc::now(),
                outcome: Outcome::Failure,
                result_hash: None,
            });
        }

        // Success resets counter
        detector.record(OperationPattern {
            tool_name: "Read".to_string(),
            args_hash: 1,
            timestamp: chrono::Utc::now(),
            outcome: Outcome::Success,
            result_hash: None,
        });

        // Now 2 more failures should NOT trigger (counter was reset)
        for _ in 0..2 {
            detector.record(OperationPattern {
                tool_name: "exec".to_string(),
                args_hash: 1,
                timestamp: chrono::Utc::now(),
                outcome: Outcome::Failure,
                result_hash: None,
            });
        }

        let detection = detector.detect_circuit_breaker();
        assert!(detection.is_none(), "Success should reset circuit breaker");
    }

    #[test]
    fn test_reset_circuit_breaker() {
        let mut detector = LoopDetector::with_circuit_breaker(10, 3, 3);

        for _ in 0..3 {
            detector.record(OperationPattern {
                tool_name: "exec".to_string(),
                args_hash: 1,
                timestamp: chrono::Utc::now(),
                outcome: Outcome::Failure,
                result_hash: None,
            });
        }

        assert_eq!(detector.consecutive_failures(), 3);

        detector.reset_circuit_breaker();
        assert_eq!(detector.consecutive_failures(), 0);

        let detection = detector.detect_circuit_breaker();
        assert!(detection.is_none(), "After reset, no circuit breaker");
    }

    // =============================================================================
    // Loop Detection Priority Tests
    // =============================================================================

    #[test]
    fn test_detect_loop_priority_generic_repeat_first() {
        let mut detector = LoopDetector::new(10, 3);

        // This should trigger generic repeat detection
        for _ in 0..3 {
            detector.record(OperationPattern {
                tool_name: "Read".to_string(),
                args_hash: 100,
                timestamp: chrono::Utc::now(),
                outcome: Outcome::Success,
                result_hash: None,
            });
        }

        let detection = detector.detect_loop();
        assert!(detection.is_some());
        // Generic repeat is checked first
        assert!(matches!(
            detection.unwrap().loop_type,
            LoopType::GenericRepeat
        ));
    }

    #[test]
    fn test_detect_loop_empty_history() {
        let detector = LoopDetector::new(10, 3);
        let detection = detector.detect_loop();
        assert!(
            detection.is_none(),
            "Empty history should not detect any loops"
        );
    }

    // =============================================================================
    // History Management Tests
    // =============================================================================

    #[test]
    fn test_history_eviction() {
        let mut detector = LoopDetector::new(3, 3);

        for i in 0..5 {
            detector.record(OperationPattern {
                tool_name: format!("Tool{i}"),
                args_hash: i as u64,
                timestamp: chrono::Utc::now(),
                outcome: Outcome::Success,
                result_hash: None,
            });
        }

        assert_eq!(
            detector.history_len(),
            3,
            "History should be capped at max_history"
        );
    }

    #[test]
    fn test_consecutive_failures_accumulation() {
        let mut detector = LoopDetector::new(10, 3);

        for _ in 0..3 {
            detector.record(OperationPattern {
                tool_name: "exec".to_string(),
                args_hash: 1,
                timestamp: chrono::Utc::now(),
                outcome: Outcome::Failure,
                result_hash: None,
            });
        }

        assert_eq!(detector.consecutive_failures(), 3);
    }

    // =============================================================================
    // LoopDetection Display and Structure Tests
    // =============================================================================

    #[test]
    fn test_loop_detection_display_all_types() {
        let cases = vec![
            LoopDetection {
                loop_type: LoopType::GenericRepeat,
                tool_name: "Read".to_string(),
                args_hash: 100,
                occurrences: 3,
                first_seen: 0,
                last_seen: 2,
            },
            LoopDetection {
                loop_type: LoopType::PollNoProgress,
                tool_name: "Grep".to_string(),
                args_hash: 200,
                occurrences: 4,
                first_seen: 1,
                last_seen: 4,
            },
            LoopDetection {
                loop_type: LoopType::PingPong,
                tool_name: "Read <-> Write".to_string(),
                args_hash: 0,
                occurrences: 2,
                first_seen: 0,
                last_seen: 3,
            },
            LoopDetection {
                loop_type: LoopType::CircuitBreaker,
                tool_name: "exec".to_string(),
                args_hash: 1,
                occurrences: 30,
                first_seen: 0,
                last_seen: 29,
            },
        ];

        for detection in cases {
            let display = format!("{detection}");
            assert!(!display.is_empty());
        }
    }

    #[test]
    fn test_loop_detection_clone() {
        let detection = LoopDetection {
            loop_type: LoopType::GenericRepeat,
            tool_name: "Test".to_string(),
            args_hash: 42,
            occurrences: 3,
            first_seen: 0,
            last_seen: 2,
        };

        let cloned = detection.clone();
        assert_eq!(cloned.loop_type, detection.loop_type);
        assert_eq!(cloned.tool_name, detection.tool_name);
        assert_eq!(cloned.args_hash, detection.args_hash);
        assert_eq!(cloned.occurrences, detection.occurrences);
    }

    // =============================================================================
    // Edge Cases
    // =============================================================================

    #[test]
    fn test_detect_loop_with_empty_history() {
        let detector = LoopDetector::new(5, 3);
        assert!(detector.detect_loop().is_none());
    }

    #[test]
    fn test_detect_poll_with_result_hash_none() {
        let mut detector = LoopDetector::new(10, 3);

        // Read without result_hash should not trigger poll detection
        for _ in 0..3 {
            detector.record(OperationPattern {
                tool_name: "Read".to_string(),
                args_hash: 111,
                timestamp: chrono::Utc::now(),
                outcome: Outcome::Success,
                result_hash: None,
            });
        }

        let detection = detector.detect_poll_no_progress();
        assert!(
            detection.is_none(),
            "Without result_hash, poll detection should not trigger"
        );
    }

    #[test]
    fn test_operation_pattern_creation() {
        let pattern = OperationPattern {
            tool_name: "TestTool".to_string(),
            args_hash: 12345,
            timestamp: chrono::Utc::now(),
            outcome: Outcome::Success,
            result_hash: Some(67890),
        };

        assert_eq!(pattern.tool_name, "TestTool");
        assert_eq!(pattern.args_hash, 12345);
        assert!(matches!(pattern.outcome, Outcome::Success));
        assert_eq!(pattern.result_hash, Some(67890));
    }

    #[test]
    fn test_outcome_variants() {
        let success = Outcome::Success;
        assert!(matches!(success, Outcome::Success));

        let failure = Outcome::Failure;
        assert!(matches!(failure, Outcome::Failure));

        let pending = Outcome::Pending;
        assert!(matches!(pending, Outcome::Pending));
    }

    #[test]
    fn test_loop_type_variants() {
        assert!(matches!(LoopType::GenericRepeat, LoopType::GenericRepeat));
        assert!(matches!(LoopType::PollNoProgress, LoopType::PollNoProgress));
        assert!(matches!(LoopType::PingPong, LoopType::PingPong));
        assert!(matches!(LoopType::CircuitBreaker, LoopType::CircuitBreaker));
    }

    #[test]
    fn test_detector_with_custom_circuit_breaker_threshold() {
        let detector = LoopDetector::with_circuit_breaker(100, 5, 50);
        assert_eq!(detector.consecutive_failures(), 0);
        assert_eq!(detector.history_len(), 0);
    }

    #[test]
    fn test_large_history_handling() {
        let mut detector = LoopDetector::new(1000, 3);

        // Add many entries with truly unique patterns (tool + args combination)
        for i in 0..100 {
            detector.record(OperationPattern {
                tool_name: format!("Tool{i}"),
                args_hash: i as u64,
                timestamp: chrono::Utc::now(),
                outcome: Outcome::Success,
                result_hash: Some(i as u64),
            });
        }

        assert_eq!(detector.history_len(), 100);
        // Should not have any loop detection since patterns are all different
        assert!(detector.detect_loop().is_none());
    }
}
