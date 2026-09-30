//! Built-in metrics beyond keyword matching.
//!
//! - [`LlmJudgeMetric`]: LLM-as-judge quality scoring behind the
//!   local [`JudgeProvider`] seam
//! - [`SchemaValidationMetric`]: validates JSON output against a
//!   JSON-Schema subset using
//!   `synthia_core::schema::validate_against_schema`

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;

use crate::{EvalError, runner::AsyncMetric};

/// A minimal provider interface for LLM judge calls.
///
/// Implement this to connect [`LlmJudgeMetric`] to any LLM
/// backend — a real provider in production, a stub in tests.
#[async_trait]
pub trait JudgeProvider: Send + Sync + 'static {
    /// Call the judge with the given prompt, return its text
    /// response.
    ///
    /// # Errors
    ///
    /// Implementations return an error when the judge call
    /// itself fails; [`LlmJudgeMetric`] then scores the case
    /// `0.0` (an unavailable judge must fail the case, not the
    /// harness).
    async fn respond(&self, prompt: &str) -> Result<String, EvalError>;
}

/// LLM-based evaluation metric with named criteria.
///
/// Builds a judge prompt from the case input, the agent's
/// actual output, and the configured criteria, then parses a
/// `0.0..=1.0` score out of the judge's reply (see
/// [`parse_score`]).
///
/// # Example
///
/// ```rust
/// use async_trait::async_trait;
/// use synthia_eval::{
///     EvalError,
///     metrics::{JudgeProvider, LlmJudgeMetric},
/// };
///
/// struct StubJudge;
///
/// #[async_trait]
/// impl JudgeProvider for StubJudge {
///     async fn respond(&self, _prompt: &str) -> Result<String, EvalError> {
///         Ok("Score: 0.85".to_string())
///     }
/// }
///
/// let metric = LlmJudgeMetric::new(StubJudge)
///     .with_criteria("accuracy", "Is the answer factually correct?");
/// ```
pub struct LlmJudgeMetric<P: JudgeProvider> {
    provider: Arc<P>,
    criteria: Vec<(String, String)>,
}

impl<P: JudgeProvider> LlmJudgeMetric<P> {
    /// New judge metric backed by the given provider.
    #[must_use]
    pub fn new(provider: P) -> Self {
        Self {
            provider: Arc::new(provider),
            criteria: Vec::new(),
        }
    }

    /// Add a named evaluation criterion.
    ///
    /// Both the name and the prompt text appear in the judge
    /// prompt; the name is documentation, the prompt is the
    /// question the judge answers.
    #[must_use]
    pub fn with_criteria(
        mut self,
        name: impl Into<String>,
        prompt: impl Into<String>,
    ) -> Self {
        self.criteria.push((name.into(), prompt.into()));
        self
    }

    /// Build the judge prompt for one case.
    fn build_prompt(&self, input: &str, actual_output: &str) -> String {
        let criteria_text = if self.criteria.is_empty() {
            "Is this a high-quality response?".to_string()
        } else {
            self.criteria
                .iter()
                .map(|(name, prompt)| format!("- {name}: {prompt}"))
                .collect::<Vec<_>>()
                .join("\n")
        };
        format!(
            "Evaluate the following agent response:\n\n\
             Input: {input}\n\n\
             Response: {actual_output}\n\n\
             Criteria:\n{criteria_text}\n\n\
             Provide a score from 0.0 to 1.0. \
             Respond with only: Score: <number>"
        )
    }
}

#[async_trait]
impl<P: JudgeProvider> AsyncMetric for LlmJudgeMetric<P> {
    fn name(&self) -> &'static str {
        "llm_judge"
    }

    async fn score(
        &self,
        input: &str,
        actual_output: &str,
        _expected_keywords: &[&str],
    ) -> f64 {
        let prompt = self.build_prompt(input, actual_output);
        match self.provider.respond(&prompt).await {
            Ok(response) => parse_score(&response),
            Err(_) => 0.0,
        }
    }
}

/// Parse a `0.0..=1.0` score from a judge reply.
///
/// Re-export of [`synthia_core::parse_judge_score`] so a single
/// parser backs both the offline grader ([`LlmJudgeMetric`]) and
/// the in-loop scorer (`synthia_harness::LlmJudgeScorer`). The
/// previous local copy was case-sensitive on the `Score:`
/// prefix; the shared one honours any case (matching the
/// documented contract) and also handles the no-space variant
/// `Score:0.5` that some models emit.
///
/// Recognises `Score: 0.85` (case-insensitive prefix, any line)
/// and a standalone number line (`0.85`). The parsed value is
/// clamped to `0.0..=1.0` so an out-of-range judge reply (e.g.
/// `Score: 7`) cannot break the score domain. Anything else
/// scores `0.0`.
#[must_use]
pub fn parse_score(response: &str) -> f64 {
    synthia_core::parse_judge_score(response)
}

/// Validates that the agent output is JSON matching a
/// JSON-Schema subset.
///
/// The output must parse as JSON and satisfy
/// [`validate_against_schema`](synthia_core::schema::validate_against_schema)
/// with **zero violations** (`1.0`), or
/// the score is `0.0`. Non-JSON output scores `0.0`.
///
/// # Example
///
/// ```rust
/// use serde_json::json;
/// use synthia_eval::metrics::SchemaValidationMetric;
///
/// let metric = SchemaValidationMetric::new(json!({
///     "type": "object",
///     "required": ["name"],
///     "properties": {"name": {"type": "string"}}
/// }));
/// ```
pub struct SchemaValidationMetric {
    schema: Value,
}

impl SchemaValidationMetric {
    /// New metric validating against the given schema.
    #[must_use]
    pub fn new(schema: Value) -> Self {
        Self { schema }
    }
}

#[async_trait]
impl AsyncMetric for SchemaValidationMetric {
    fn name(&self) -> &'static str {
        "schema_validation"
    }

    async fn score(
        &self,
        _input: &str,
        actual_output: &str,
        _expected_keywords: &[&str],
    ) -> f64 {
        let Ok(value) = serde_json::from_str::<Value>(actual_output) else {
            return 0.0;
        };
        match synthia_core::validate_against_schema(&self.schema, &value) {
            Ok(()) => 1.0,
            Err(_) => 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;

    use super::*;

    struct StubJudge {
        reply: String,
        prompts: parking_lot::Mutex<Vec<String>>,
    }

    impl StubJudge {
        fn fixed(reply: &str) -> Self {
            Self {
                reply: reply.to_string(),
                prompts: parking_lot::Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait]
    impl JudgeProvider for StubJudge {
        async fn respond(&self, prompt: &str) -> Result<String, EvalError> {
            self.prompts.lock().push(prompt.to_string());
            Ok(self.reply.clone())
        }
    }

    struct FailingJudge;

    #[async_trait]
    impl JudgeProvider for FailingJudge {
        async fn respond(&self, _prompt: &str) -> Result<String, EvalError> {
            Err(EvalError::Judge("judge offline".to_string()))
        }
    }

    #[test]
    fn parse_score_reads_score_prefix() {
        assert!((parse_score("Score: 0.85") - 0.85).abs() < 1e-9);
        assert!(
            (parse_score("verdict:\nScore:0.7\nthanks") - 0.7).abs() < 1e-9
        );
    }

    #[test]
    fn parse_score_reads_standalone_number() {
        assert!((parse_score("0.5") - 0.5).abs() < 1e-9);
        assert!((parse_score("  1.0 ") - 1.0).abs() < 1e-9);
    }

    #[test]
    fn parse_score_clamps_out_of_range_values() {
        assert!((parse_score("Score: 7") - 1.0).abs() < 1e-9);
        assert!((parse_score("Score: -0.5") - 0.0).abs() < 1e-9);
    }

    #[test]
    fn parse_score_garbage_scores_zero() {
        assert!((parse_score("The response was great!") - 0.0).abs() < 1e-9);
        assert!((parse_score("") - 0.0).abs() < 1e-9);
    }

    #[tokio::test]
    async fn llm_judge_scores_parsed_reply() {
        let metric = LlmJudgeMetric::new(StubJudge::fixed("Score: 0.85"));
        let score = metric.score("q", "answer", &[]).await;
        assert!((score - 0.85).abs() < 1e-9);
    }

    #[tokio::test]
    async fn llm_judge_provider_failure_scores_zero() {
        let metric = LlmJudgeMetric::new(FailingJudge);
        let score = metric.score("q", "answer", &[]).await;
        assert!((score - 0.0).abs() < 1e-9);
    }

    #[tokio::test]
    async fn llm_judge_prompt_carries_case_and_criteria() {
        let judge = StubJudge::fixed("Score: 1");
        let metric = LlmJudgeMetric::new(judge)
            .with_criteria("accuracy", "Is it factually correct?");
        let score = metric.score("what is 2+2", "four", &[]).await;
        assert!((score - 1.0).abs() < 1e-9);

        let prompts = metric.provider.prompts.lock();
        assert_eq!(prompts.len(), 1);
        let prompt = &prompts[0];
        assert!(prompt.contains("what is 2+2"), "prompt: {prompt}");
        assert!(prompt.contains("four"), "prompt: {prompt}");
        assert!(prompt.contains("accuracy"), "prompt: {prompt}");
        assert!(
            prompt.contains("Is it factually correct?"),
            "prompt: {prompt}"
        );
    }

    #[tokio::test]
    async fn schema_metric_rewards_valid_json() {
        let metric = SchemaValidationMetric::new(serde_json::json!({
            "type": "object",
            "required": ["name"],
            "properties": {
                "name": {"type": "string"},
                "score": {"type": "number"}
            }
        }));
        let score =
            metric.score("q", r#"{"name": "x", "score": 3}"#, &[]).await;
        assert!((score - 1.0).abs() < 1e-9);
    }

    #[tokio::test]
    async fn schema_metric_zero_on_violations() {
        let metric = SchemaValidationMetric::new(serde_json::json!({
            "type": "object",
            "required": ["name"],
            "properties": {"name": {"type": "string"}}
        }));
        // Missing required property.
        let score = metric.score("q", r#"{"other": 1}"#, &[]).await;
        assert!((score - 0.0).abs() < 1e-9);
        // Wrong property type.
        let score = metric.score("q", r#"{"name": 5}"#, &[]).await;
        assert!((score - 0.0).abs() < 1e-9);
    }

    #[tokio::test]
    async fn schema_metric_zero_on_non_json_output() {
        let metric =
            SchemaValidationMetric::new(serde_json::json!({"type": "object"}));
        let score = metric.score("q", "plain text, not JSON", &[]).await;
        assert!((score - 0.0).abs() < 1e-9);
    }
}
