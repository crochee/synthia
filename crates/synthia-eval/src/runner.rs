//! Async [`AsyncMetric`] trait and the [`EvalRunner`] engine.
//!
//! Sync [`crate::Metric`] implementations plug in through
//! [`SyncMetricAdapter`].
//!
//! # Example
//!
//! ```rust
//! use synthia_eval::{
//!     EvalRunner,
//!     KeywordMetric,
//!     Metric,
//!     runner::SyncMetricAdapter,
//! };
//!
//! struct AlwaysOne;
//!
//! impl Metric for AlwaysOne {
//!     fn name(&self) -> &'static str {
//!         "always_one"
//!     }
//!
//!     fn score(&self, _: &str, _: &str, _: &[&str]) -> f64 {
//!         1.0
//!     }
//! }
//!
//! let runner = EvalRunner::new()
//!     .metric(Box::new(SyncMetricAdapter(KeywordMetric)))
//!     .metric(Box::new(SyncMetricAdapter(AlwaysOne)))
//!     .threshold(0.8);
//! ```

use std::{collections::BTreeMap, sync::Arc};

use async_trait::async_trait;
use synthia_core::Clock;

use crate::{
    EvalError,
    EvalReport,
    EvalSuite,
    KEYWORD_METRIC_NAME,
    KeywordMetric,
    Metric,
    TestResult,
};

/// Async trait for evaluation metrics.
///
/// Implement this to add custom (possibly I/O-bound) scoring
/// logic — e.g. metrics that call another model.
#[async_trait]
pub trait AsyncMetric: Send + Sync + 'static {
    /// Metric name — the key in [`TestResult::scores`].
    fn name(&self) -> &'static str;

    /// Score the actual output; `0.0` (worst) to `1.0` (best).
    async fn score(
        &self,
        input: &str,
        actual_output: &str,
        expected_keywords: &[&str],
    ) -> f64;
}

/// A callable async agent under test.
///
/// [`EvalRunner::run`] calls [`EvalAgent::respond`] once per
/// [`crate::TestCase`] and scores the returned string.
#[async_trait]
pub trait EvalAgent: Send + Sync {
    /// Run the agent on the given input and return its response.
    ///
    /// # Errors
    ///
    /// Implementations return an error when the agent itself
    /// fails; the runner aborts the whole suite with
    /// [`EvalError::Agent`] so harness bugs are not mistaken
    /// for failing cases.
    async fn respond(&self, input: &str) -> Result<String, EvalError>;
}

/// Evaluation runner — executes a suite against an agent.
///
/// A case passes iff **every** configured metric scores ≥ the
/// threshold. With no metrics configured, the runner falls back
/// to keyword matching (scored under
/// [`KEYWORD_METRIC_NAME`]).
pub struct EvalRunner {
    metrics: Vec<Arc<dyn AsyncMetric>>,
    threshold: f64,
    /// Wall-clock source for the report's `generated_at`. Default
    /// [`synthia_core::SystemClock`]; injectable via
    /// [`EvalRunner::with_clock`] so a report's timestamp is
    /// reproducible in tests.
    clock: synthia_core::SharedClock,
}

impl EvalRunner {
    /// New runner with no metrics and the default threshold 0.7.
    #[must_use]
    pub fn new() -> Self {
        Self {
            metrics: Vec::new(),
            threshold: 0.7,
            clock: synthia_core::SharedClock::system(),
        }
    }

    /// Install the wall-clock source the report's `generated_at` is
    /// read from.
    #[must_use]
    pub fn with_clock(mut self, clock: synthia_core::SharedClock) -> Self {
        self.clock = clock;
        self
    }

    /// Add a metric to score agent outputs with.
    #[must_use]
    pub fn metric(mut self, metric: Box<dyn AsyncMetric>) -> Self {
        self.metrics.push(Arc::from(metric));
        self
    }

    /// Set the minimum score threshold for a case to pass.
    #[must_use]
    pub fn threshold(mut self, threshold: f64) -> Self {
        self.threshold = threshold;
        self
    }

    /// Execute the suite against the agent.
    ///
    /// For each case: call [`EvalAgent::respond`], score the
    /// response with every metric, mark the case passed iff all
    /// scores are ≥ the threshold, and aggregate into an
    /// [`EvalReport`].
    ///
    /// # Errors
    ///
    /// Returns [`EvalError::Agent`] if the agent fails on any
    /// case (the run aborts — a broken harness is not a failed
    /// case).
    pub async fn run(
        &self,
        agent: &dyn EvalAgent,
        suite: &EvalSuite,
    ) -> Result<EvalReport, EvalError> {
        let mut results = Vec::new();
        let mut passed_count = 0usize;
        let mut total_score = 0.0f64;
        let mut score_count = 0usize;

        for case in suite.cases() {
            let actual_output = agent.respond(&case.input).await?;
            let keywords: Vec<&str> =
                case.expected_keywords.iter().map(String::as_str).collect();
            let scores = self
                .score_case(&case.input, &actual_output, &keywords)
                .await;
            let case_total = scores.values().sum::<f64>();
            total_score += case_total;
            score_count += scores.len();
            let passed = scores.values().all(|s| *s >= self.threshold);
            if passed {
                passed_count += 1;
            }
            results.push(TestResult {
                case_id: case.id.clone(),
                actual_output,
                scores,
                passed,
            });
        }

        let average_score = if score_count > 0 {
            total_score / score_count as f64
        } else {
            0.0
        };
        Ok(EvalReport {
            suite_name: suite.name().to_string(),
            generated_at: self.clock.now(),
            results,
            average_score,
            passed: passed_count,
            total: suite.cases().len(),
        })
    }

    /// Score one case: every configured metric, or the keyword
    /// fallback when the runner has no metrics.
    async fn score_case(
        &self,
        input: &str,
        actual_output: &str,
        keywords: &[&str],
    ) -> BTreeMap<String, f64> {
        let mut scores = BTreeMap::new();
        if self.metrics.is_empty() {
            let s = KeywordMetric.score(input, actual_output, keywords);
            scores.insert(KEYWORD_METRIC_NAME.to_string(), s);
            return scores;
        }
        for metric in &self.metrics {
            let s = metric.score(input, actual_output, keywords).await;
            scores.insert(metric.name().to_string(), s);
        }
        scores
    }
}

impl Default for EvalRunner {
    fn default() -> Self {
        Self::new()
    }
}

/// Wraps a sync [`Metric`] impl as an [`AsyncMetric`].
///
/// # Example
///
/// ```rust
/// use synthia_eval::{EvalRunner, KeywordMetric, runner::SyncMetricAdapter};
///
/// let runner =
///     EvalRunner::new().metric(Box::new(SyncMetricAdapter(KeywordMetric)));
/// ```
pub struct SyncMetricAdapter<M: Metric>(pub M);

#[async_trait]
impl<M: Metric> AsyncMetric for SyncMetricAdapter<M> {
    fn name(&self) -> &'static str {
        self.0.name()
    }

    async fn score(
        &self,
        input: &str,
        actual_output: &str,
        expected_keywords: &[&str],
    ) -> f64 {
        self.0.score(input, actual_output, expected_keywords)
    }
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;

    use super::*;
    use crate::{EvalSuite, TestCase};

    struct EchoAgent;

    #[async_trait]
    impl EvalAgent for EchoAgent {
        async fn respond(&self, input: &str) -> Result<String, EvalError> {
            Ok(format!("echo: {input}"))
        }
    }

    struct FailingAgent;

    #[async_trait]
    impl EvalAgent for FailingAgent {
        async fn respond(&self, _input: &str) -> Result<String, EvalError> {
            Err(EvalError::Agent("boom".to_string()))
        }
    }

    struct FixedMetric(f64, &'static str);

    #[async_trait]
    impl AsyncMetric for FixedMetric {
        fn name(&self) -> &'static str {
            self.1
        }

        async fn score(&self, _: &str, _: &str, _: &[&str]) -> f64 {
            self.0
        }
    }

    #[tokio::test]
    async fn runner_reports_one_result_per_case() {
        let suite = EvalSuite::new("suite")
            .add_case(TestCase::new("c1", "hello").expect_contains("echo"))
            .add_case(TestCase::new("c2", "world").expect_contains("echo"))
            .add_case(TestCase::new("c3", "foo").expect_contains("echo"));

        let runner = EvalRunner::new()
            .metric(Box::new(SyncMetricAdapter(KeywordMetric)))
            .threshold(0.8);

        let report = runner.run(&EchoAgent, &suite).await.unwrap();

        assert_eq!(report.results.len(), 3);
        assert_eq!(report.total, 3);
        // EchoAgent's replies always contain "echo" → full match.
        assert_eq!(report.passed, 3);
        assert!((report.average_score - 1.0).abs() < 1e-9);
    }

    #[tokio::test]
    async fn case_fails_when_score_below_threshold() {
        let suite = EvalSuite::new("s")
            .add_case(TestCase::new("c1", "hello").expect_contains("xyzabc"));

        let runner = EvalRunner::new()
            .metric(Box::new(SyncMetricAdapter(KeywordMetric)))
            .threshold(0.8);

        let report = runner.run(&EchoAgent, &suite).await.unwrap();
        assert_eq!(report.passed, 0);
        assert!(!report.results[0].passed);
        assert!(
            (report.results[0].scores[KEYWORD_METRIC_NAME] - 0.0).abs() < 1e-9
        );
    }

    #[tokio::test]
    async fn case_passes_only_if_all_metrics_meet_threshold() {
        let suite = EvalSuite::new("s").add_case(TestCase::new("c1", "x"));
        // One metric at 1.0, one at 0.5, threshold 0.7 → fail.
        let runner = EvalRunner::new()
            .metric(Box::new(FixedMetric(1.0, "good")))
            .metric(Box::new(FixedMetric(0.5, "weak")))
            .threshold(0.7);

        let report = runner.run(&EchoAgent, &suite).await.unwrap();
        assert_eq!(report.passed, 0);
        // Average spans both metrics: (1.0 + 0.5) / 2.
        assert!((report.average_score - 0.75).abs() < 1e-9);
    }

    #[tokio::test]
    async fn no_metrics_falls_back_to_keyword_match() {
        let suite = EvalSuite::new("s")
            .add_case(TestCase::new("hit", "a").expect_contains("echo"))
            .add_case(TestCase::new("miss", "b").expect_contains("nope"));

        let report = EvalRunner::new()
            .threshold(1.0)
            .run(&EchoAgent, &suite)
            .await
            .unwrap();

        assert_eq!(report.passed, 1);
        assert_eq!(report.results[0].scores.len(), 1);
        assert!(
            (report.results[0].scores[KEYWORD_METRIC_NAME] - 1.0).abs() < 1e-9
        );
        assert!(
            (report.results[1].scores[KEYWORD_METRIC_NAME] - 0.0).abs() < 1e-9
        );
    }

    #[tokio::test]
    async fn agent_error_aborts_the_run() {
        let suite = EvalSuite::new("s").add_case(TestCase::new("c1", "x"));
        let runner = EvalRunner::new()
            .metric(Box::new(SyncMetricAdapter(KeywordMetric)));
        let err = runner.run(&FailingAgent, &suite).await.unwrap_err();
        assert!(matches!(err, EvalError::Agent(_)), "got: {err}");
    }

    #[tokio::test]
    async fn empty_suite_yields_empty_report() {
        let suite = EvalSuite::new("empty");
        let runner = EvalRunner::new()
            .metric(Box::new(SyncMetricAdapter(KeywordMetric)));
        let report = runner.run(&EchoAgent, &suite).await.unwrap();
        assert!(report.results.is_empty());
        assert_eq!(report.total, 0);
        assert_eq!(report.passed, 0);
        assert!((report.average_score - 0.0).abs() < 1e-9);
    }

    #[tokio::test]
    async fn sync_metric_adapter_delegates_to_inner_metric() {
        let adapter = SyncMetricAdapter(KeywordMetric);
        let score = adapter.score("in", "hello world", &["hello"]).await;
        assert!((score - 1.0).abs() < 1e-9);
        assert_eq!(adapter.name(), KEYWORD_METRIC_NAME);
    }
}
