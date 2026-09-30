//! [`CandidateScorer`] — how a best-of-N run decides which candidate
//! wins — plus the default implementation.
//!
//! The strategy owns the sampling; the scorer owns the judgement. That
//! split is the whole extension point of
//! [`BestOfNStrategy`](super::BestOfNStrategy): the same strategy with a
//! length, a similarity, a verifier, or an LLM judge behind it is four
//! different products, and none of them needs a new strategy.

use async_trait::async_trait;

/// Scores one candidate answer; **higher wins**.
///
/// Implementations run on the strategy's own task (each candidate spawns
/// through [`AgentRuntime::spawner`](super::AgentRuntime::spawner) and
/// is then scored). Pure-local scorers — length, similarity, test
/// counts — keep `score` synchronously cheap. An LLM-as-judge scorer
/// makes a provider call inside `score`; the await is fine because the
/// work is already on its own spawned task.
///
/// The score is reported in the progress events, so it should be
/// meaningful to an operator (a length, a similarity, a verifier
/// verdict, an LLM judge rating) — not a secret rank.
#[async_trait]
pub trait CandidateScorer: Send + Sync + 'static {
    /// Score `candidate` (the answer text, never empty — empty
    /// completions are rejected as failures before scoring).
    async fn score(&self, candidate: &str) -> f64;

    /// Label for logs, e.g. `"longest-answer"`.
    fn name(&self) -> &str {
        "candidate-scorer"
    }
}

/// The default scorer: prefer the longest answer.
///
/// A crude but honest proxy — on tasks where any single answer is
/// usually right and short answers are truncated or evasive, "most
/// complete" correlates with "best". Anything with a real signal (a
/// verifier, a test run, an LLM judge) should replace it.
#[derive(Debug, Clone, Copy, Default)]
pub struct LongestAnswer;

#[async_trait]
impl CandidateScorer for LongestAnswer {
    async fn score(&self, candidate: &str) -> f64 {
        // Characters, not bytes: a CJK answer must not outrank an
        // ASCII one of the same length for byte-count reasons.
        candidate.chars().count() as f64
    }

    fn name(&self) -> &str {
        "longest-answer"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The score is a *character* count: a three-character CJK answer
    /// (nine bytes) must not beat a longer ASCII one.
    #[test]
    fn longest_answer_counts_characters_not_bytes() {
        futures::executor::block_on(async {
            let scorer = LongestAnswer;
            assert_eq!(scorer.score("abc").await, 3.0);
            assert_eq!(scorer.score("文档库").await, 3.0);
            assert!(
                scorer.score("abcdefgh").await > scorer.score("文档库").await,
                "nine bytes must not outrank eight characters"
            );
        });
    }
}
