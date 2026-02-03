use std::collections::HashMap;

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
    history: Vec<OperationPattern>,
    max_history: usize,
    max_loops: usize,
    circuit_breaker_threshold: usize,
    consecutive_failures: usize,
}

impl LoopDetector {
    pub fn new(max_history: usize, max_loops: usize) -> Self {
        Self {
            history: Vec::new(),
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
            history: Vec::new(),
            max_history,
            max_loops,
            circuit_breaker_threshold,
            consecutive_failures: 0,
        }
    }

    pub fn record(&mut self, pattern: OperationPattern) {
        if self.history.len() >= self.max_history {
            self.history.remove(0);
        }

        match pattern.outcome {
            Outcome::Failure => self.consecutive_failures += 1,
            Outcome::Success => self.consecutive_failures = 0,
            Outcome::Pending => {}
        }

        self.history.push(pattern);
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
