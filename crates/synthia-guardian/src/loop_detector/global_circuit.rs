//! Global circuit breaker: 30-iteration hard cap on agent loops.

use tracing::warn;

use crate::types::LoopStatus;

const GLOBAL_CIRCUIT_THRESHOLD: usize = 30;

/// Tracks iteration count to enforce a hard cap.
///
/// The current iteration is supplied via the `iteration` argument of
/// [`Self::check`]. When `iteration >= threshold` (default 30),
/// [`LoopStatus::Detected`] is returned so the caller can emit a
/// `HardBlock` action.
pub(crate) struct GlobalCircuitDetector {
    threshold: usize,
}

impl GlobalCircuitDetector {
    pub(crate) fn new() -> Self {
        Self {
            threshold: GLOBAL_CIRCUIT_THRESHOLD,
        }
    }

    pub(crate) fn check(&mut self, iteration: usize) -> LoopStatus {
        if iteration >= self.threshold {
            warn!(iteration = iteration, "Global circuit breaker triggered");
            LoopStatus::Detected
        } else {
            LoopStatus::Ok
        }
    }

    pub(crate) fn reset(&mut self) {
        // Stateless: nothing to reset.
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn global_circuit_under_threshold() {
        let mut det = GlobalCircuitDetector::new();
        for i in 0..GLOBAL_CIRCUIT_THRESHOLD - 1 {
            assert_eq!(det.check(i), LoopStatus::Ok);
        }
    }

    #[test]
    fn global_circuit_at_threshold() {
        let mut det = GlobalCircuitDetector::new();
        assert_eq!(det.check(GLOBAL_CIRCUIT_THRESHOLD), LoopStatus::Detected);
    }
}
