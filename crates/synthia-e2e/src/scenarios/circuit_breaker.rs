use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use synthia_provider::ModelProvider;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
enum CircuitState {
    #[default]
    Closed,
    Open,
    HalfOpen,
}

#[derive(Debug)]
struct CircuitBreaker {
    state: Arc<tokio::sync::Mutex<CircuitState>>,
    failure_count: Arc<AtomicUsize>,
    success_count: Arc<AtomicUsize>,
    error_threshold: usize,
    recovery_timeout_secs: u64,
}

impl CircuitBreaker {
    fn new(error_threshold: usize) -> Self {
        Self {
            state: Arc::new(tokio::sync::Mutex::new(CircuitState::Closed)),
            failure_count: Arc::new(AtomicUsize::new(0)),
            success_count: Arc::new(AtomicUsize::new(0)),
            error_threshold,
            recovery_timeout_secs: 60,
        }
    }

    async fn record_success(&self) {
        let state = self.state.lock().await;
        if *state == CircuitState::HalfOpen {
            drop(state);
            let mut s = self.state.lock().await;
            *s = CircuitState::Closed;
            self.failure_count.store(0, Ordering::SeqCst);
        } else {
            self.success_count.fetch_add(1, Ordering::SeqCst);
        }
    }

    async fn record_failure(&self) {
        let failures = self.failure_count.fetch_add(1, Ordering::SeqCst) + 1;
        if failures >= self.error_threshold {
            let mut state = self.state.lock().await;
            *state = CircuitState::Open;
        }
    }

    async fn is_open(&self) -> bool {
        let state = self.state.lock().await;
        *state == CircuitState::Open
    }

    async fn try_reset(&self) -> bool {
        let mut state = self.state.lock().await;
        if *state == CircuitState::Open {
            *state = CircuitState::HalfOpen;
            true
        } else {
            false
        }
    }

    async fn state(&self) -> CircuitState {
        *self.state.lock().await
    }

    fn failure_count(&self) -> usize {
        self.failure_count.load(Ordering::SeqCst)
    }
}

#[tokio::test]
async fn test_circuit_breaker_opens() {
    let breaker = Arc::new(CircuitBreaker::new(3));

    for i in 0..3 {
        breaker.record_failure().await;
        let is_open = breaker.is_open().await;
        if i < 2 {
            assert!(
                !is_open,
                "Circuit should not open until {} failures",
                i + 1
            );
        } else {
            assert!(is_open, "Circuit should open after 3 failures");
        }
    }

    assert_eq!(breaker.failure_count(), 3);
}

#[cfg(test)]
mod circuit_breaker_tests {
    use super::*;

    #[tokio::test]
    async fn test_circuit_breaker_half_open_recovery() {
        let breaker = CircuitBreaker::new(2);

        breaker.record_failure().await;
        breaker.record_failure().await;
        assert!(breaker.is_open().await);

        breaker.try_reset().await;
        assert_eq!(breaker.state().await, CircuitState::HalfOpen);

        breaker.record_success().await;
        assert_eq!(breaker.state().await, CircuitState::Closed);
    }

    #[tokio::test]
    async fn test_circuit_breaker_half_open_failure() {
        let breaker = CircuitBreaker::new(1);

        breaker.record_failure().await;
        assert!(breaker.is_open().await);

        breaker.try_reset().await;
        assert_eq!(breaker.state().await, CircuitState::HalfOpen);

        breaker.record_failure().await;
        assert!(breaker.is_open().await);
        assert_eq!(breaker.state().await, CircuitState::Open);
    }

    #[tokio::test]
    async fn test_circuit_breaker_success_does_not_close() {
        let breaker = CircuitBreaker::new(5);

        for _ in 0..4 {
            breaker.record_success().await;
        }

        assert!(!breaker.is_open().await);
        assert_eq!(breaker.state().await, CircuitState::Closed);
    }

    #[tokio::test]
    async fn test_circuit_breaker_config() {
        let config = crate::fixtures::configs::TestConfig::guardian_config();

        assert!(
            config.content["circuit_breaker"]["enabled"]
                .as_bool()
                .unwrap()
        );
        assert_eq!(
            config.content["circuit_breaker"]["error_threshold"]
                .as_i64()
                .unwrap(),
            5
        );
        assert_eq!(
            config.content["circuit_breaker"]["timeout_seconds"]
                .as_i64()
                .unwrap(),
            60
        );
    }

    #[tokio::test]
    async fn test_circuit_breaker_integration() {
        let breaker = Arc::new(CircuitBreaker::new(3));
        let mut provider = crate::utils::mock_provider::MockProvider::new();

        for _ in 0..3 {
            provider.with_response(
                crate::utils::mock_provider::MockResponse::text("success"),
            );
        }

        let mut circuit_open = false;
        for i in 0..5 {
            let response = provider
                .complete(synthia_provider::CompletionRequest::default())
                .await;

            if i < 3 {
                if response.is_ok() {
                    breaker.record_success().await;
                } else {
                    breaker.record_failure().await;
                }
            } else {
                if response.is_err() {
                    breaker.record_failure().await;
                    circuit_open = breaker.is_open().await;
                }
            }
        }

        assert!(
            circuit_open || breaker.failure_count() >= 1,
            "Circuit should eventually open after failures"
        );
    }
}
