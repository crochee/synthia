//! # Best-of-N with an LLM-as-judge scorer
//!
//! Run it:
//!
//! ```text
//! cargo run --example best_of_n_judge -p synthia-harness
//! ```
//!
//! Same agent builder, same tool registry, same `BestOfNStrategy` —
//! the only thing that changes is the **scorer**: `LongestAnswer` is
//! replaced with `LlmJudgeScorer`, which holds an `Arc<dyn
//! ModelProvider>` and grades each candidate against a rubric. Three
//! samples, one judge call per sample, the highest-scoring candidate
//! is published.
//!
//! The judge provider is the same scripted provider that produces
//! the candidates (a real deployment would point it at a separate
//! model — a stronger model, a rubric-tuned model, a verifier loop).
//! The judge prompt is the in-tree `DEFAULT_JUDGE_PROMPT`; a
//! deployment that wants its own criterion uses
//! `LlmJudgeScorer::with_rubric(provider, rubric)`.
//!
//! The point of the example is to show the seam in action: a
//! `CandidateScorer` can be a cheap local predicate (as in
//! `strategy_swap.rs`'s `RequiresMarker`), `LongestAnswer` (the
//! default), or a model call. The strategy does not care which.
//!
//! It ends by printing `BEST-OF-N-JUDGE: OK` after asserting —
//! from the providers' own recorded requests — that three
//! candidates were sampled, three judge calls were made (one per
//! candidate), and the judge chose the highest-scoring answer.

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use async_trait::async_trait;
use futures::StreamExt;
use parking_lot::Mutex;
use synthia_core::{AtomicCancelToken, CancelToken};
use synthia_harness::{
    Agent,
    AgentEvent,
    AgentInput,
    ReActAgent,
    agent::{BestOfNStrategy, CandidateScorer, LlmJudgeScorer},
};
use synthia_provider::{
    CompletionRequest,
    CompletionResponse,
    Content,
    ContentPart,
    ModelConfig,
    ModelProvider,
    ProviderConfig,
    TextContent,
};
use synthia_tool::ToolRegistry;

/// A scripted provider that splits its script between **candidate**
/// replies (for `BestOfNStrategy`'s sampling) and **judge** replies
/// (for `LlmJudgeScorer::score`).
///
/// The candidate script is a plain `Vec` consumed FIFO — the
/// strategy samples in `0..N` index order, so the i-th candidate
/// completion hands out `candidate_script[i]`.
///
/// The judge script is keyed by **candidate text content**, not by
/// completion order: the LLM judge embeds the candidate into its
/// prompt (see `LlmJudgeScorer::score`), so the script picks a
/// score by matching a substring of the candidate. This makes the
/// example deterministic despite the strategy's concurrent fan-out
/// (candidates finish in non-deterministic order), and proves the
/// judge — not the order candidates came back — picks the winner.
struct DualScriptedProvider {
    candidate_script: Mutex<Vec<Content>>,
    /// `Vec<(needle, score)>` — the first entry whose needle appears
    /// in the candidate prompt wins. Order matters only when two
    /// needles overlap, which the example avoids by picking
    /// disjoint substrings.
    judge_script: Vec<(String, String)>,
    candidate_requests: AtomicUsize,
    judge_requests: AtomicUsize,
    /// The model config returned to whichever caller asks first.
    /// `BestOfNStrategy` and `LlmJudgeScorer` both read it on every
    /// call; returning the same value keeps the two in sync.
    model_name: String,
}

impl DualScriptedProvider {
    fn new(
        candidate_script: Vec<Content>,
        judge_script: Vec<(String, String)>,
        model_name: impl Into<String>,
    ) -> Arc<Self> {
        Arc::new(Self {
            candidate_script: Mutex::new(candidate_script),
            judge_script,
            candidate_requests: AtomicUsize::new(0),
            judge_requests: AtomicUsize::new(0),
            model_name: model_name.into(),
        })
    }

    fn candidate_calls(&self) -> usize {
        self.candidate_requests.load(Ordering::SeqCst)
    }

    fn judge_calls(&self) -> usize {
        self.judge_requests.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl ModelProvider for DualScriptedProvider {
    async fn initialize(
        &mut self,
        _config: ProviderConfig,
    ) -> Result<(), synthia_core::Error> {
        Ok(())
    }

    fn name(&self) -> &str {
        "dual-scripted"
    }

    fn model_config(&self) -> ModelConfig {
        ModelConfig {
            name: self.model_name.clone(),
            provider: "dual-scripted".into(),
            context_window: 8_192,
            max_output_tokens: 64,
            supports_tools: false,
            supports_streaming: false,
            supports_reasoning: false,
        }
    }

    async fn complete(
        &self,
        request: CompletionRequest,
    ) -> Result<CompletionResponse, synthia_core::Error> {
        // The judge prompt is the single User turn that starts with
        // the rubric's marker; the candidate prompt is anything
        // else. The discriminator is content-based rather than
        // call-count so the example does not depend on scheduling
        // order (candidates and judges race in `BestOfNStrategy`).
        let is_judge = request
            .messages
            .first()
            .map(|m| {
                m.content
                    .extract_text()
                    .is_some_and(|t| t.contains("You are grading"))
            })
            .unwrap_or(false);
        if is_judge {
            self.judge_requests.fetch_add(1, Ordering::SeqCst);
            // The judge prompt embeds the candidate text (see
            // `LlmJudgeScorer::score`); pick the score by matching a
            // needle from `judge_script` against that text. The
            // first match wins, so an exhausted script falls back to
            // `0.0` rather than blocking the run.
            let candidate_text = request
                .messages
                .first()
                .and_then(|m| m.content.extract_text())
                .unwrap_or_default();
            let text = self
                .judge_script
                .iter()
                .find(|(needle, _)| candidate_text.contains(needle.as_str()))
                .map(|(_, score)| score.clone())
                .unwrap_or_else(|| "Score: 0.0".to_string());
            Ok(CompletionResponse {
                content: Content::Single(ContentPart::Text(TextContent {
                    text,
                    cache_control: None,
                })),
                ..CompletionResponse::default()
            })
        } else {
            self.candidate_requests.fetch_add(1, Ordering::SeqCst);
            let content = {
                let mut script = self.candidate_script.lock();
                if script.is_empty() {
                    Content::text("(script exhausted)")
                } else {
                    script.remove(0)
                }
            };
            Ok(CompletionResponse {
                content,
                ..CompletionResponse::default()
            })
        }
    }
}

async fn run_and_print(agent: &dyn Agent, prompt: &str) -> String {
    let cancel: Arc<dyn CancelToken> = AtomicCancelToken::shared();
    let mut stream = agent.run(AgentInput::text(prompt), cancel).await;

    let mut answer = String::new();
    let mut events = 0usize;
    while let Some(event) = stream.next().await {
        events += 1;
        match &event {
            AgentEvent::ModelDone(done) => answer = done.text.clone(),
            AgentEvent::System(
                synthia_harness::events::SystemEvent::SessionEnded { reason },
            ) => {
                println!("      ended        : {reason:?}");
            }
            AgentEvent::System(
                synthia_harness::events::SystemEvent::Progress {
                    message, ..
                },
            ) => println!("      progress     : {message}"),
            _ => {}
        }
    }
    println!("      events       : {events}");
    println!("      answer       : {}", answer.replace('\n', " ⏎ "));
    answer
}

#[tokio::main]
async fn main() {
    println!("=== synthia: Best-of-N with an LLM-as-judge scorer ===\n");

    // Three candidates, deliberately unordered by quality. The
    // middle one is the strongest; the third is the longest (so it
    // would win under `LongestAnswer`). A real judge should rank
    // the middle one highest.
    let candidates = vec![
        Content::text("Paris is in France."),
        Content::text(
            "The capital of France is Paris, a city of about 2.1 \
             million people on the Seine. It has been the \
             country's capital since the 10th century.",
        ),
        Content::text(
            "Paris is the largest city in France, located in the \
             north-central part of the country, and it serves as \
             the political, cultural, and economic heart of the \
             nation. It is one of the most visited cities in the \
             world.",
        ),
    ];

    // Judge scores, keyed by a substring of the candidate the
    // judge prompt will embed. The middle candidate has the
    // strongest content ("2.1 million…Seine…10th century"), so it
    // gets 0.95; the one-line answer gets the lowest score
    // (0.3); the third gets the middle (0.7). Keying by content
    // rather than completion order sidesteps the
    // `BestOfNStrategy` race — candidates finish concurrently,
    // and the i-th judge call is not necessarily the i-th candidate.
    let judge_scores = vec![
        ("Paris is in France.".to_string(), "Score: 0.3".to_string()),
        ("2.1 million".to_string(), "Score: 0.95".to_string()),
        ("north-central".to_string(), "Score: 0.7".to_string()),
    ];

    let provider =
        DualScriptedProvider::new(candidates, judge_scores, "judged-best-of-n");

    let scorer = LlmJudgeScorer::new(provider.clone());
    println!("[1/1] BestOfNStrategy + LlmJudgeScorer");
    println!("      scorer       : {} (rubric: default)", scorer.name());
    println!(
        "      judge rubric : {}",
        synthia_harness::agent::DEFAULT_JUDGE_PROMPT
            .lines()
            .next()
            .unwrap_or("")
    );

    let bon = ReActAgent::new(provider.clone(), Arc::new(ToolRegistry::new()))
        .with_workspace(".")
        .with_strategy(Arc::new(BestOfNStrategy::new(3, Arc::new(scorer))))
        .with_max_iterations(1)
        .with_name("bon-judge-run");
    let answer = run_and_print(&bon, "What is the capital of France?").await;
    let candidate_calls = provider.candidate_calls();
    let judge_calls = provider.judge_calls();

    println!("      candidate    : {candidate_calls} (expected 3)");
    println!("      judge calls  : {judge_calls} (expected 3)");

    // ---- the proof ---------------------------------------------------
    assert_eq!(
        candidate_calls, 3,
        "BestOfNStrategy must sample three candidates"
    );
    assert_eq!(
        judge_calls, 3,
        "LlmJudgeScorer must grade every candidate exactly once"
    );
    assert!(
        answer.contains("2.1 million")
            || answer.contains("Seine")
            || answer.contains("10th century"),
        "the judge must pick the strongest candidate (index 1, \
         scored 0.95). Got: {answer}"
    );

    println!("\nBEST-OF-N-JUDGE: OK");
}
