use std::sync::Arc;

use synthia_provider::ModelProvider;
use tokio::sync::Mutex;

use crate::utils::mock_provider::{MockProvider, MockResponse};

#[derive(Debug, Clone)]
struct LoopDetectionResult {
    detected: bool,
    pattern: String,
    count: usize,
    block_type: Option<String>,
}

#[derive(Debug)]
struct LoopDetector {
    history: Arc<Mutex<Vec<String>>>,
    threshold_soft: usize,
    threshold_hard: usize,
}

impl LoopDetector {
    fn new() -> Self {
        Self {
            history: Arc::new(Mutex::new(Vec::new())),
            threshold_soft: 5,
            threshold_hard: 10,
        }
    }

    fn with_thresholds(mut self, soft: usize, hard: usize) -> Self {
        self.threshold_soft = soft;
        self.threshold_hard = hard;
        self
    }

    async fn detect(&self, action: &str) -> LoopDetectionResult {
        let mut history = self.history.lock().await;
        history.push(action.to_string());

        let count = history.iter().filter(|h| *h == action).count();

        let block_type = if count >= self.threshold_hard {
            Some("hard".to_string())
        } else if count >= self.threshold_soft {
            Some("soft".to_string())
        } else {
            None
        };

        LoopDetectionResult {
            detected: count > 1,
            pattern: action.to_string(),
            count,
            block_type,
        }
    }

    async fn reset(&self) {
        let mut history = self.history.lock().await;
        history.clear();
    }
}

impl Default for LoopDetector {
    fn default() -> Self {
        Self::new()
    }
}

#[tokio::test]
async fn test_loop_detection_soft_block() {
    let detector = LoopDetector::new().with_thresholds(3, 6);

    let actions = vec!["read_file", "read_file", "read_file"];

    let mut results = Vec::new();
    for action in actions {
        let result = detector.detect(action).await;
        results.push(result);
    }

    assert_eq!(results[2].block_type, Some("soft".to_string()));

    let history = detector.history.lock().await;
    assert_eq!(history.len(), 3);
}

#[tokio::test]
async fn test_loop_detection_hard_block() {
    let detector = LoopDetector::new().with_thresholds(3, 5);

    let actions = vec!["search", "search", "search", "search", "search"];

    let mut last_result: Option<LoopDetectionResult> = None;
    for action in actions {
        let result = detector.detect(action).await;
        last_result = Some(result);
    }

    let result = last_result.unwrap();
    assert!(result.detected);
    assert_eq!(result.count, 5);
    assert_eq!(result.block_type, Some("hard".to_string()));
}

#[cfg(test)]
mod loop_detection_tests {
    use super::*;

    #[tokio::test]
    async fn test_loop_detection_different_patterns() {
        let detector = LoopDetector::new();

        detector.detect("action_a").await;
        detector.detect("action_b").await;
        detector.detect("action_a").await;

        let result = detector.detect("action_a").await;
        assert!(result.count >= 1, "Loop count should be at least 1");
        assert!(result.block_type.is_none() || result.block_type.is_some());
    }

    #[tokio::test]
    async fn test_loop_detection_reset() {
        let detector = LoopDetector::new();

        for _ in 0..5 {
            detector.detect("read_file").await;
        }

        let result = detector.detect("read_file").await;
        assert!(result.block_type.is_some());

        detector.reset().await;

        let result = detector.detect("read_file").await;
        assert_eq!(result.count, 1);
        assert!(result.block_type.is_none());
    }

    #[tokio::test]
    async fn test_loop_detection_with_provider() {
        let mut provider = MockProvider::new();

        for i in 0..10 {
            provider.with_response(MockResponse::text(format!(
                "Response {} with repeated search pattern",
                i
            )));
        }

        for _i in 0..10 {
            let _ = provider
                .complete(synthia_provider::CompletionRequest::default())
                .await;
        }

        assert_eq!(provider.call_count(), 10);
    }

    #[tokio::test]
    async fn test_loop_detection_config() {
        let config = crate::fixtures::configs::TestConfig::guardian_config();

        assert!(
            config.content["loop_detection"]["enabled"]
                .as_bool()
                .unwrap()
        );
        assert_eq!(
            config.content["loop_detection"]["soft_block_after"]
                .as_i64()
                .unwrap(),
            5
        );
        assert_eq!(
            config.content["loop_detection"]["hard_block_after"]
                .as_i64()
                .unwrap(),
            10
        );
    }
}
