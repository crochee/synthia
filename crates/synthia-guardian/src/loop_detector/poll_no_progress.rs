//! Poll no-progress detector: 10 consecutive identical poll results.

use tracing::warn;

use crate::{loop_detector::hash::hash_value, types::LoopStatus};

const POLL_NO_PROGRESS_THRESHOLD: usize = 10;

/// Detects when polling calls return identical results, indicating no progress.
pub(crate) struct PollNoProgressDetector {
    result_hashes: Vec<u64>,
}

impl PollNoProgressDetector {
    pub(crate) fn new() -> Self {
        Self {
            result_hashes: Vec::new(),
        }
    }

    pub(crate) fn check(&mut self, result: &str) -> LoopStatus {
        let hash = hash_value(result);
        self.result_hashes.push(hash);

        let count = self.result_hashes.iter().filter(|&&h| h == hash).count();

        if count >= POLL_NO_PROGRESS_THRESHOLD {
            warn!(count = count, "Poll no-progress loop detected");
            return LoopStatus::Detected;
        }

        LoopStatus::Ok
    }

    pub(crate) fn reset(&mut self) {
        self.result_hashes.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn poll_no_progress_loop() {
        let mut det = PollNoProgressDetector::new();
        for i in 0..POLL_NO_PROGRESS_THRESHOLD {
            let result = det.check("same_result");
            if i < POLL_NO_PROGRESS_THRESHOLD - 1 {
                assert_eq!(result, LoopStatus::Ok);
            } else {
                assert_eq!(result, LoopStatus::Detected);
            }
        }
    }

    #[test]
    fn poll_no_progress_different_results_ok() {
        let mut det = PollNoProgressDetector::new();
        for i in 0..5 {
            let result = det.check(&format!("result_{i}"));
            assert_eq!(result, LoopStatus::Ok);
        }
    }
}
