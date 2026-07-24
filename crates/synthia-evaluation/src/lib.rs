use std::sync::Arc;

use async_trait::async_trait;
use indexmap::IndexMap;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvaluationInput {
    pub task: String,
    pub agent_output: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvaluationScore {
    pub criterion: String,
    pub score: f64,
    pub rationale: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MultiDimensionScore {
    pub scores: Vec<EvaluationScore>,
    pub overall_score: f64,
    pub weights: std::collections::HashMap<String, f64>,
}

impl MultiDimensionScore {
    pub fn new(
        scores: Vec<EvaluationScore>,
        weights: std::collections::HashMap<String, f64>,
    ) -> Self {
        let total_weight: f64 = weights.values().sum();
        let overall = if total_weight > 0.0 {
            scores
                .iter()
                .map(|s| s.score * weights.get(&s.criterion).unwrap_or(&1.0))
                .sum::<f64>()
                / total_weight
        } else {
            scores.iter().map(|s| s.score).sum::<f64>()
                / scores.len().max(1) as f64
        };
        Self {
            scores,
            overall_score: overall,
            weights,
        }
    }
}

#[derive(Debug, Error)]
pub enum EvaluationError {
    #[error("Evaluator failed: {0}")]
    EvaluatorFailed(String),
}

#[async_trait]
pub trait Evaluator: Send + Sync {
    fn name(&self) -> &str;
    async fn evaluate(
        &self,
        input: &EvaluationInput,
    ) -> Result<EvaluationScore, EvaluationError>;
}

pub struct EvaluationRegistry {
    evaluators: Mutex<IndexMap<String, Arc<dyn Evaluator + Send + Sync>>>,
}

impl Default for EvaluationRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl EvaluationRegistry {
    pub fn new() -> Self {
        Self {
            evaluators: Mutex::new(IndexMap::new()),
        }
    }

    pub fn register(
        &self,
        name: String,
        evaluator: Arc<dyn Evaluator + Send + Sync>,
    ) {
        self.evaluators.lock().insert(name, evaluator);
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn Evaluator + Send + Sync>> {
        self.evaluators.lock().get(name).cloned()
    }

    pub async fn evaluate_all(
        &self,
        input: &EvaluationInput,
    ) -> MultiDimensionScore {
        let evaluators: Vec<_> = {
            let guard = self.evaluators.lock();
            guard.values().cloned().collect()
        };
        let mut scores = Vec::new();
        let mut weights = std::collections::HashMap::new();
        for evaluator in evaluators {
            if let Ok(score) = evaluator.evaluate(input).await {
                let name = evaluator.name().to_string();
                scores.push(score.clone());
                weights.insert(name, 1.0);
            }
        }
        MultiDimensionScore::new(scores, weights)
    }
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;

    use super::*;

    struct TestEvaluator;

    #[async_trait]
    impl Evaluator for TestEvaluator {
        fn name(&self) -> &str {
            "test"
        }

        async fn evaluate(
            &self,
            _input: &EvaluationInput,
        ) -> Result<EvaluationScore, EvaluationError> {
            Ok(EvaluationScore {
                criterion: "test".to_string(),
                score: 0.8,
                rationale: "Looks good".to_string(),
            })
        }
    }

    #[tokio::test]
    async fn test_evaluator_trait() {
        let evaluator = TestEvaluator;
        assert_eq!(evaluator.name(), "test");
    }

    #[tokio::test]
    async fn test_evaluation_registry() {
        let registry = EvaluationRegistry::new();
        registry.register("test".to_string(), Arc::new(TestEvaluator));
        let retrieved = registry.get("test");
        assert!(retrieved.is_some());
    }

    #[tokio::test]
    async fn test_evaluator_evaluate() {
        let evaluator = TestEvaluator;
        let input = EvaluationInput {
            task: "test".to_string(),
            agent_output: serde_json::json!("result"),
        };
        let score = evaluator.evaluate(&input).await.unwrap();
        assert_eq!(score.criterion, "test");
        assert!((score.score - 0.8).abs() < f64::EPSILON);
    }
}
