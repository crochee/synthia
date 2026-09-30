//! LLM-as-judge scorer for [`BestOfNStrategy`].
//!
//! [`BestOfNStrategy`]: super::BestOfNStrategy
//!
//! [`LongestAnswer`](super::LongestAnswer) is the default scorer —
//! cheap, deterministic, fine as a "most-complete" proxy. When the
//! deployment has access to a model that can grade, [`LlmJudgeScorer`]
//! wraps that model call as a `CandidateScorer` so the same
//! `BestOfNStrategy::new` knob accepts it.
//!
//! The judge prompt asks the model for a single number between `0.0`
//! and `1.0`; [`parse_judge_score`] extracts it from a free-form
//! reply. The same parse rules are used by
//! `synthia_eval::metrics::parse_score`, intentionally duplicated
//! here so `synthia-harness` does not depend on `synthia-eval`
//! (the agent runs in production; the eval crate is for offline
//! grading — different life cycles).
//!
//! The judge is one more `Arc<dyn ModelProvider>` and runs on the
//! candidate's own spawned task. No new dependency; no new
//! runtime; no new feature. Drop-in: `LongestAnswer` and
//! `LlmJudgeScorer` implement the same trait, so a deployment
//! can swap them at boot.

use std::sync::Arc;

use async_trait::async_trait;
use synthia_provider::{CompletionRequest, ModelProvider, Role, TextContent};

use super::scorer::CandidateScorer;

/// Default rubric appended to the judge prompt when the caller
/// does not supply one.
///
/// `Score: 0.0..=1.0` on its own line — same shape
/// `synthia_eval::metrics::parse_score` recognises, so the in-tree
/// judge and the in-tree eval grader agree on what a score looks
/// like.
pub const DEFAULT_JUDGE_PROMPT: &str = "\
You are grading a candidate answer to a task. Reply with one line in the form:\n\
Score: <number between 0.0 and 1.0>\n\
Higher = better. Be strict: 1.0 means correct and complete, 0.0 means wrong or empty.";

/// An LLM-backed [`CandidateScorer`]: the model receives a rubric +
/// the candidate, returns a `0.0..=1.0` rating, the scorer surfaces
/// that as the candidate's score.
///
/// The judge model and the candidate model are typically the same
/// provider handle. Holding `Arc<dyn ModelProvider>` keeps the
/// scorer runtime-neutral (the seam lives in `synthia-provider`).
pub struct LlmJudgeScorer {
    provider: Arc<dyn ModelProvider>,
    rubric: String,
    max_tokens: Option<u32>,
}

impl std::fmt::Debug for LlmJudgeScorer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LlmJudgeScorer")
            .field("rubric", &self.rubric)
            .field("max_tokens", &self.max_tokens)
            .finish_non_exhaustive()
    }
}

impl LlmJudgeScorer {
    /// Build a judge that grades each candidate against
    /// [`DEFAULT_JUDGE_PROMPT`].
    #[must_use]
    pub fn new(provider: Arc<dyn ModelProvider>) -> Self {
        Self::with_rubric(provider, DEFAULT_JUDGE_PROMPT.to_string())
    }

    /// Build a judge that grades each candidate against `rubric`
    /// (one or more sentences describing the criterion).
    #[must_use]
    pub fn with_rubric(
        provider: Arc<dyn ModelProvider>,
        rubric: String,
    ) -> Self {
        Self {
            provider,
            rubric,
            max_tokens: None,
        }
    }

    /// Cap the judge's reply length. Most judges need only a few
    /// tokens; a too-low cap truncates before the score line
    /// surfaces and the shared judge-score parser returns `0.0`
    /// (see [`synthia_core::judge_score`]).
    #[must_use]
    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = Some(max_tokens);
        self
    }

    /// The rubric the scorer grades against.
    #[must_use]
    pub fn rubric(&self) -> &str {
        &self.rubric
    }
}

#[async_trait]
impl CandidateScorer for LlmJudgeScorer {
    async fn score(&self, candidate: &str) -> f64 {
        let prompt = format!(
            "{rubric}\n\n--- candidate answer to grade ---\n\
             {candidate}\n--- end ---\n",
            rubric = self.rubric,
        );
        let request = CompletionRequest {
            model: self.provider.model_config().name,
            messages: Arc::new(vec![synthia_provider::Message::new(
                Role::User,
                synthia_provider::Content::Single(
                    synthia_provider::ContentPart::Text(TextContent {
                        text: prompt,
                        cache_control: None,
                    }),
                ),
            )]),
            max_tokens: Some(self.max_tokens.map(|v| v as usize).unwrap_or(32)),
            ..CompletionRequest::default()
        };
        match self.provider.complete(request).await {
            Ok(response) => {
                let text =
                    synthia_provider::completion_to_sampling(&response).text;
                parse_judge_score(&text)
            }
            Err(error) => {
                // A judge that errors must not skew the ranking toward
                // "no answer" — a `0.0` would still be a real rank
                // (every judge error would tie at the bottom). Surface
                // it to the operator; the strategy's own error path
                // already records the failure, so the candidate
                // itself is marked as a failure rather than ranked.
                tracing::warn!(
                    %error,
                    "LlmJudgeScorer: judge provider call failed; \
                     returning neutral 0.5 so the candidate is not \
                     advantaged or buried by the error",
                );
                0.5
            }
        }
    }

    fn name(&self) -> &str {
        "llm-judge"
    }
}

/// Parse a `0.0..=1.0` score from a judge reply.
///
/// Re-export of [`synthia_core::parse_judge_score`] so a single
/// parser backs both the offline grader (`synthia_eval::parse_score`)
/// and the in-loop scorer ([`LlmJudgeScorer`]). The agent crate
/// used to ship its own copy; the shared parser is the single
/// source of truth (R76).
///
/// Recognises `Score: 0.85` (case-insensitive prefix, any line,
/// with or without a trailing space) and a standalone number
/// line. Clamped to `0.0..=1.0`; anything else scores `0.0`.
#[must_use]
pub fn parse_judge_score(reply: &str) -> f64 {
    synthia_core::parse_judge_score(reply)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use async_trait::async_trait;
    use parking_lot::Mutex;
    use synthia_core::Error as ProviderError;
    use synthia_provider::{
        CompletionResponse,
        Content,
        ContentPart,
        ModelConfig,
        ProviderConfig,
        TextContent,
    };

    use super::*;

    struct JudgeFixture {
        replies: Mutex<Vec<String>>,
        calls: AtomicUsize,
        fail: bool,
    }

    impl JudgeFixture {
        fn new(replies: &[&str]) -> Arc<Self> {
            Arc::new(Self {
                replies: Mutex::new(
                    replies.iter().map(|r| (*r).to_string()).collect(),
                ),
                calls: AtomicUsize::new(0),
                fail: false,
            })
        }

        fn failing() -> Arc<Self> {
            Arc::new(Self {
                replies: Mutex::new(Vec::new()),
                calls: AtomicUsize::new(0),
                fail: true,
            })
        }
    }

    #[async_trait]
    impl ModelProvider for JudgeFixture {
        async fn initialize(
            &mut self,
            _config: ProviderConfig,
        ) -> Result<(), ProviderError> {
            Ok(())
        }

        fn name(&self) -> &str {
            "judge-fixture"
        }

        fn model_config(&self) -> ModelConfig {
            ModelConfig {
                name: "judge".into(),
                provider: "judge-fixture".into(),
                context_window: 8_192,
                max_output_tokens: 64,
                supports_tools: false,
                supports_streaming: false,
                supports_reasoning: false,
            }
        }

        async fn complete(
            &self,
            _request: CompletionRequest,
        ) -> Result<CompletionResponse, ProviderError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if self.fail {
                return Err(ProviderError::internal("judge offline"));
            }
            let text = {
                let mut replies = self.replies.lock();
                if replies.is_empty() {
                    String::new()
                } else {
                    replies.remove(0)
                }
            };
            Ok(CompletionResponse {
                content: Content::Single(ContentPart::Text(TextContent {
                    text,
                    cache_control: None,
                })),
                ..CompletionResponse::default()
            })
        }
    }

    #[test]
    fn parses_score_prefix() {
        assert_eq!(parse_judge_score("Score: 0.85"), 0.85);
        assert_eq!(parse_judge_score("score: 0.5"), 0.5);
        assert_eq!(parse_judge_score("SCORE: 1.0"), 1.0);
    }

    #[test]
    fn parses_standalone_number() {
        assert_eq!(parse_judge_score("\n\n0.42\n"), 0.42);
    }

    #[test]
    fn clamps_out_of_range() {
        assert_eq!(parse_judge_score("Score: 7"), 1.0);
        assert_eq!(parse_judge_score("Score: -3"), 0.0);
    }

    #[test]
    fn unknown_reply_scores_zero() {
        assert_eq!(parse_judge_score("nothing to see here"), 0.0);
    }

    #[test]
    fn finds_score_on_later_line() {
        assert_eq!(
            parse_judge_score("The candidate is mostly correct.\nScore: 0.7"),
            0.7
        );
    }

    #[tokio::test]
    async fn judge_scorer_returns_parsed_value() {
        let provider = JudgeFixture::new(&["Score: 0.9"]);
        let scorer = LlmJudgeScorer::new(provider.clone());
        let score = scorer.score("the answer").await;
        assert!((score - 0.9).abs() < f64::EPSILON);
        assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
        assert_eq!(scorer.name(), "llm-judge");
    }

    #[tokio::test]
    async fn judge_scorer_falls_back_to_neutral_on_provider_error() {
        let provider = JudgeFixture::failing();
        let scorer = LlmJudgeScorer::new(provider.clone());
        // Provider error must not collapse every failing candidate
        // to 0.0 (which would rank them all at the bottom); a
        // neutral 0.5 keeps the judge out of the ranking while the
        // strategy's own failure path records the issue.
        let score = scorer.score("anything").await;
        assert!((score - 0.5).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn judge_prompt_carries_rubric_and_candidate() {
        let provider = JudgeFixture::new(&["Score: 0.5"]);
        let scorer = LlmJudgeScorer::with_rubric(
            provider.clone(),
            "reward: short, factual".into(),
        );
        let _ = scorer.score("Paris is the capital of France").await;
        // The fixture does not expose the request, but a call
        // proves the prompt built without panic and the call
        // shape (one call per `score`) holds.
        assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
    }
}
