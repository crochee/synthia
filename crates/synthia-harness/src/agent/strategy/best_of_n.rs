//! Best-of-N: the reasoning loop that uses the runtime's
//! *concurrency*.
//!
//! Sampled-then-selected answers are standard practice for tasks with a
//! checkable or scorable outcome (self-consistency, unit-test-green
//! patches, structured extraction): ask the model N times, score each
//! answer, keep the best. This strategy is that loop, and it exists for
//! three reasons:
//!
//! 1. **It is useful** for exactly those tasks, at a known multiple of
//!    the cost of one answer.
//! 2. **It exercises a different part of the seam than
//!    [`ChainOfThoughtStrategy`](super::ChainOfThoughtStrategy)**:
//!    it spawns N concurrent samples through
//!    [`AgentRuntime::spawner`](super::AgentRuntime::spawner) and
//!    collects them through `futures` channels, so the strategy stays
//!    runtime-neutral and the deployment's executor is used — not a
//!    private `tokio::spawn`.
//! 3. **It shows how alternatives are published.** Candidate answers
//!    are *off-stream* (fanning five answers at a chat UI is noise);
//!    what reaches the stream is the winner, plus one progress event
//!    per candidate and a selection event. The scoring function is
//!    yours — see [`CandidateScorer`].
//!
//! ```text
//!          ┌─ candidate 1 ─ score 0.4 ┐
//! request ─┼─ candidate 2 ─ score 0.9 ┼─▶ winner published as the answer
//!          └─ candidate N ─ score 0.6 ┘
//! ```

use std::{sync::Arc, time::Instant};

use async_trait::async_trait;
use futures::{StreamExt as _, channel::mpsc};
use synthia_core::CancelToken;
use synthia_provider::{
    CompletionRequest,
    ContentPart,
    ModelProvider,
    SamplingResult,
    TextContent,
    completion_to_sampling,
};
use synthia_session::{iteration_end, iteration_start};

use super::{
    request::single_shot_request,
    scorer::{CandidateScorer, LongestAnswer},
};
use crate::{
    agent::strategy::{AgentRuntime, EventSink, ReasoningStrategy},
    events::{AgentEvent, SystemEvent},
    input::AgentInput,
};

/// How many candidates the strategy samples by default.
pub(crate) const DEFAULT_CANDIDATES: usize = 3;

/// One scored candidate: the sample index it was drawn from, its score,
/// and its text.
type Scored = (usize, f64, String);

/// Sample one candidate on the runtime's executor and report its answer.
///
/// `complete_with_stream` (rather than `complete`) so the candidate
/// honours cancellation; the deltas are deliberately discarded —
/// alternatives are not a live answer.
///
/// Free function rather than a closure inside [`BestOfNStrategy::fan_out`]:
/// the body awaits a provider, and a named task is both flatter and the
/// thing an operator finds in a backtrace.
async fn sample_candidate(
    provider: Arc<dyn ModelProvider>,
    cancel: Arc<dyn CancelToken>,
    request: CompletionRequest,
    index: usize,
    tx: mpsc::UnboundedSender<(usize, Result<String, String>)>,
) {
    let result = provider
        .complete_with_stream(request, Some(cancel), Box::new(|_chunk| {}))
        .await
        .map(|response| completion_to_sampling(&response).text)
        .map_err(|err| err.to_string());
    let _ = tx.unbounded_send((index, result));
}

/// Sample `candidates` answers concurrently, score them, publish the
/// winner.
pub struct BestOfNStrategy {
    candidates: usize,
    scorer: Arc<dyn CandidateScorer>,
}

impl std::fmt::Debug for BestOfNStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BestOfNStrategy")
            .field("candidates", &self.candidates)
            .field("scorer", &self.scorer.name())
            .finish()
    }
}

impl Default for BestOfNStrategy {
    fn default() -> Self {
        Self::new(DEFAULT_CANDIDATES, Arc::new(LongestAnswer))
    }
}

impl BestOfNStrategy {
    /// Sample `candidates` answers and keep the highest-scoring one.
    ///
    /// `candidates` clamps to at least 1 — one candidate is the same as
    /// sampling once, which is a legitimate (if unexciting)
    /// configuration and not a bug.
    #[must_use]
    pub fn new(candidates: usize, scorer: Arc<dyn CandidateScorer>) -> Self {
        Self {
            candidates: candidates.max(1),
            scorer,
        }
    }

    /// How many samples this strategy draws.
    #[must_use]
    pub fn candidates(&self) -> usize {
        self.candidates
    }

    /// The scorer, for introspection.
    #[must_use]
    pub fn scorer(&self) -> &Arc<dyn CandidateScorer> {
        &self.scorer
    }

    /// Spawn one sampling task per candidate through the runtime's
    /// spawner and return the receiver their answers land on.
    ///
    /// The task's own copy of the sender is dropped before returning, or
    /// the channel would never close when every candidate has reported.
    fn fan_out(
        &self,
        runtime: &AgentRuntime,
        request: &CompletionRequest,
    ) -> mpsc::UnboundedReceiver<(usize, Result<String, String>)> {
        let (tx, rx) = mpsc::unbounded::<(usize, Result<String, String>)>();
        for index in 0..self.candidates {
            runtime.spawner.spawn(Box::pin(sample_candidate(
                Arc::clone(&runtime.provider),
                Arc::clone(&runtime.cancel),
                request.clone(),
                index,
                tx.clone(),
            )));
        }
        drop(tx);
        rx
    }

    /// Ask the model `candidates` times concurrently and score every
    /// answer that comes back.
    ///
    /// Returns the scored answers plus one message per candidate that
    /// produced nothing usable, so the caller can report *why* a run had
    /// no winner instead of only that it had none.
    async fn sample_and_score(
        &self,
        runtime: &AgentRuntime,
        request: &CompletionRequest,
        sink: &EventSink,
    ) -> (Vec<Scored>, Vec<String>) {
        let mut answers = self.fan_out(runtime, request);
        let mut scored: Vec<Scored> = Vec::new();
        let mut failures: Vec<String> = Vec::new();
        let mut done = 0usize;
        while let Some((index, result)) = answers.next().await {
            done += 1;
            match result {
                Ok(text) if !text.trim().is_empty() => {
                    // The candidate's task is the scorer's task; an LLM
                    // judge that does a provider call simply does it
                    // here. The runtime's spawner (already used for
                    // the candidates) carries this work — no extra
                    // tokio::spawn here.
                    let score = self.scorer.score(&text).await;
                    sink.emit(AgentEvent::System(SystemEvent::Progress {
                        message: format!(
                            "candidate {}/{candidates} (index {index}) scored \
                             {score:.3} via {}",
                            done,
                            self.scorer.name(),
                            candidates = self.candidates,
                        ),
                        step: done,
                        total: self.candidates,
                    }));
                    scored.push((index, score, text));
                }
                Ok(_) => failures.push(format!(
                    "candidate {index} produced an empty answer"
                )),
                Err(message) => {
                    failures
                        .push(format!("candidate {index} failed: {message}"));
                }
            }
        }
        (scored, failures)
    }

    /// Publish the outcome: the winner as the run's answer, or a typed
    /// failure explaining that nothing was usable.
    fn publish(
        &self,
        runtime: &AgentRuntime,
        sink: &EventSink,
        started: Instant,
        scored: Vec<Scored>,
        failures: Vec<String>,
    ) {
        // Highest score wins; a tie goes to the lowest index so a run
        // with several equally good answers is reproducible.
        let scored_count = scored.len();
        let winner = {
            let mut ranked = scored;
            ranked.sort_by(|a, b| {
                b.1.partial_cmp(&a.1)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a.0.cmp(&b.0))
            });
            ranked.into_iter().next()
        };

        let Some((index, score, text)) = winner else {
            // Every candidate failed. The first failure explains
            // why; the rest are logged, not concatenated, so a
            // quota error is not buried under four timeouts.
            for failure in failures.iter().skip(1) {
                tracing::warn!(%failure, "best-of-n candidate failed");
            }
            let message = failures
                .first()
                .cloned()
                .unwrap_or_else(|| "no candidates were sampled".to_string());
            sink.fail(format!(
                "best-of-n produced no usable answer: {message}"
            ));
            return;
        };

        sink.emit(AgentEvent::System(SystemEvent::Progress {
            message: format!(
                "selected candidate {index} (score {score:.3}) of \
                 {scored_count} in {:?}",
                started.elapsed()
            ),
            step: self.candidates,
            total: self.candidates,
        }));
        sink.emit(AgentEvent::Model(ContentPart::Text(TextContent {
            text: text.clone(),
            cache_control: None,
        })));
        sink.emit(AgentEvent::ModelDone(SamplingResult {
            text,
            usage: Default::default(),
            ..Default::default()
        }));
        sink.finish(&runtime.cancel);
    }
}

#[async_trait]
impl ReasoningStrategy for BestOfNStrategy {
    fn name(&self) -> &str {
        "best-of-n"
    }

    async fn run(
        &self,
        runtime: AgentRuntime,
        input: AgentInput,
        sink: EventSink,
    ) {
        let started = Instant::now();
        if !sink.begin(&runtime.cancel) {
            return;
        }

        let request = single_shot_request(&runtime, &input, None).await;

        sink.emit_typed(iteration_start(1));
        let (scored, failures) =
            self.sample_and_score(&runtime, &request, &sink).await;
        sink.emit_typed(iteration_end(1, "final_answer"));

        self.publish(&runtime, &sink, started, scored, failures);
    }
}

#[cfg(test)]
mod tests {
    //! `BestOfNStrategy` behaviour tests.
    //!
    //! Split from the bottom of `best_of_n.rs` (R102): the strategy's
    //! sampling loop (spawner use, scorer choice, cancellation,
    //! all-fail, tie-breaking) is easier to read next to its fixtures
    //! when the production file holds only the strategy itself.

    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use async_trait::async_trait;
    use parking_lot::Mutex;
    use synthia_core::{
        Error as ProviderError,
        spawn::{BoxFuture, Spawner},
    };
    use synthia_provider::{CompletionResponse, ModelConfig, ProviderConfig};

    use super::*;
    use crate::{
        SessionEndReason,
        agent::{AgentRuntime, EventSink},
    };

    /// Returns scripted answers in call order, and counts calls.
    struct ScriptedProvider {
        answers: Mutex<Vec<String>>,
        calls: AtomicUsize,
        fail_every: Option<usize>,
    }

    impl ScriptedProvider {
        fn new(answers: &[&str]) -> Arc<Self> {
            Arc::new(Self {
                answers: Mutex::new(
                    answers.iter().map(|a| (*a).to_string()).collect(),
                ),
                calls: AtomicUsize::new(0),
                fail_every: None,
            })
        }

        fn failing() -> Arc<Self> {
            Arc::new(Self {
                answers: Mutex::new(Vec::new()),
                calls: AtomicUsize::new(0),
                fail_every: Some(1),
            })
        }
    }

    #[async_trait]
    impl synthia_provider::ModelProvider for ScriptedProvider {
        async fn initialize(
            &mut self,
            _config: ProviderConfig,
        ) -> Result<(), ProviderError> {
            Ok(())
        }

        fn name(&self) -> &str {
            "scripted-bon"
        }

        fn model_config(&self) -> ModelConfig {
            ModelConfig {
                name: "bon-test".into(),
                provider: "scripted-bon".into(),
                context_window: 8_192,
                max_output_tokens: 512,
                supports_tools: true,
                supports_streaming: true,
                supports_reasoning: false,
            }
        }

        async fn complete(
            &self,
            _request: CompletionRequest,
        ) -> Result<CompletionResponse, ProviderError> {
            let call = self.calls.fetch_add(1, Ordering::SeqCst);
            if self
                .fail_every
                .is_some_and(|every| (call + 1).is_multiple_of(every))
            {
                return Err(ProviderError::internal("candidate failure"));
            }
            let text = {
                let mut answers = self.answers.lock();
                if answers.is_empty() {
                    format!("answer {call}")
                } else {
                    answers.remove(0)
                }
            };
            Ok(CompletionResponse {
                content: synthia_provider::Content::text(text),
                ..CompletionResponse::default()
            })
        }
    }

    /// Counts spawned tasks and runs them inline, so the test proves the
    /// strategy went through the runtime's spawner without needing a
    /// runtime of its own.
    struct InlineSpawner {
        spawns: Arc<AtomicUsize>,
    }

    impl Spawner for InlineSpawner {
        fn spawn(&self, task: BoxFuture<()>) {
            self.spawns.fetch_add(1, Ordering::SeqCst);
            let mut task = task;
            let mut cx =
                std::task::Context::from_waker(std::task::Waker::noop());
            // Candidate tasks await the provider, which is ready on the
            // first poll in these tests.
            let _ = task.as_mut().poll(&mut cx);
        }
    }

    fn runtime(
        provider: Arc<dyn synthia_provider::ModelProvider>,
        spawner: Arc<dyn synthia_core::spawn::Spawner>,
        cancel: Arc<dyn synthia_core::CancelToken>,
    ) -> AgentRuntime {
        let mut rt = crate::agent::strategy::default_for_test(provider, cancel);
        rt.descriptor.name = "bon".into();
        rt.descriptor.kind = "bon".into();
        rt.descriptor.instructions = "Answer.".into();
        rt.spawner = spawner;
        rt
    }

    fn drive(
        strategy: &BestOfNStrategy,
        runtime: AgentRuntime,
    ) -> Vec<AgentEvent> {
        let (tx, mut rx) = futures::channel::mpsc::unbounded();
        let sink = EventSink::new(Arc::new(tx), None);
        futures::executor::block_on(strategy.run(
            runtime,
            AgentInput::text("answer me"),
            sink,
        ));
        let mut events = Vec::new();
        while let Ok(event) = rx.try_recv() {
            events.push(event);
        }
        events
    }

    fn answer_of(events: &[AgentEvent]) -> Option<String> {
        events.iter().rev().find_map(|event| match event {
            AgentEvent::ModelDone(sampling) => Some(sampling.text.clone()),
            _ => None,
        })
    }

    fn reason_of(events: &[AgentEvent]) -> Option<SessionEndReason> {
        events.iter().find_map(|event| match event {
            AgentEvent::System(SystemEvent::SessionEnded { reason }) => {
                Some(reason.clone())
            }
            _ => None,
        })
    }

    fn progress_count(events: &[AgentEvent]) -> usize {
        events
            .iter()
            .filter(|event| {
                matches!(
                    event,
                    AgentEvent::System(SystemEvent::Progress { .. })
                )
            })
            .count()
    }

    /// N samples go out through the runtime's spawner, and the
    /// longest answer is published as the run's answer.
    #[test]
    fn samples_through_the_runtime_spawner_and_publishes_the_winner() {
        let provider = ScriptedProvider::new(&["short", "the longest answer"]);
        let spawns = Arc::new(AtomicUsize::new(0));
        let events = drive(
            &BestOfNStrategy::new(2, Arc::new(LongestAnswer)),
            runtime(
                provider.clone(),
                Arc::new(InlineSpawner {
                    spawns: Arc::clone(&spawns),
                }),
                synthia_core::AtomicCancelToken::shared(),
            ),
        );

        assert_eq!(
            spawns.load(Ordering::SeqCst),
            2,
            "one spawned task per candidate, through the runtime"
        );
        assert_eq!(provider.calls.load(Ordering::SeqCst), 2);
        assert_eq!(
            answer_of(&events).as_deref(),
            Some("the longest answer"),
            "the winner must be the longest answer"
        );
        assert_eq!(reason_of(&events), Some(SessionEndReason::Completed));
        assert_eq!(
            progress_count(&events),
            3,
            "one per candidate plus the selection"
        );
    }

    /// The scorer decides, not the strategy: a scorer that prefers
    /// short answers picks the short one.
    #[test]
    fn the_scorer_decides_which_candidate_wins() {
        struct Shortest;
        #[async_trait]
        impl CandidateScorer for Shortest {
            async fn score(&self, candidate: &str) -> f64 {
                -(candidate.chars().count() as f64)
            }

            fn name(&self) -> &str {
                "shortest"
            }
        }

        let provider =
            ScriptedProvider::new(&["short", "a much longer answer"]);
        let events = drive(
            &BestOfNStrategy::new(2, Arc::new(Shortest)),
            runtime(
                provider,
                Arc::new(InlineSpawner {
                    spawns: Arc::new(AtomicUsize::new(0)),
                }),
                synthia_core::AtomicCancelToken::shared(),
            ),
        );
        assert_eq!(answer_of(&events).as_deref(), Some("short"));
    }

    /// A pre-cancelled run samples nothing.
    #[test]
    fn a_cancelled_run_samples_nothing() {
        let provider = ScriptedProvider::new(&["a", "b"]);
        let cancel: Arc<dyn synthia_core::CancelToken> =
            synthia_core::AtomicCancelToken::shared();
        cancel.cancel();
        let events = drive(
            &BestOfNStrategy::default(),
            runtime(
                provider.clone(),
                Arc::new(InlineSpawner {
                    spawns: Arc::new(AtomicUsize::new(0)),
                }),
                cancel,
            ),
        );
        assert_eq!(reason_of(&events), Some(SessionEndReason::Cancelled));
        assert_eq!(provider.calls.load(Ordering::SeqCst), 0);
    }

    /// Every candidate failing ends the run with the first failure's
    /// reason, not with a panic or an empty success.
    #[test]
    fn all_candidates_failing_ends_with_an_error() {
        let events = drive(
            &BestOfNStrategy::new(3, Arc::new(LongestAnswer)),
            runtime(
                ScriptedProvider::failing(),
                Arc::new(InlineSpawner {
                    spawns: Arc::new(AtomicUsize::new(0)),
                }),
                synthia_core::AtomicCancelToken::shared(),
            ),
        );
        match reason_of(&events) {
            Some(SessionEndReason::Error(message)) => assert!(
                message.contains("candidate failure"),
                "the provider's reason must survive: {message}"
            ),
            other => panic!("expected an Error end reason, got {other:?}"),
        }
        assert!(answer_of(&events).is_none());
    }

    /// A tie between equal scores goes to the lowest index, so the same
    /// script always produces the same winner.
    #[test]
    fn ties_go_to_the_lowest_index() {
        let provider = ScriptedProvider::new(&["same", "same"]);
        let events = drive(
            &BestOfNStrategy::new(2, Arc::new(LongestAnswer)),
            runtime(
                provider,
                Arc::new(InlineSpawner {
                    spawns: Arc::new(AtomicUsize::new(0)),
                }),
                synthia_core::AtomicCancelToken::shared(),
            ),
        );
        let selected = events.iter().find_map(|event| match event {
            AgentEvent::System(SystemEvent::Progress { message, .. })
                if message.contains("selected candidate") =>
            {
                Some(message.clone())
            }
            _ => None,
        });
        let selected = selected.unwrap_or_default();
        assert!(
            selected.contains("candidate 0"),
            "the first candidate must win a tie: {selected:?}"
        );
    }

    /// The strategy names itself and its scorer — both appear in logs.
    #[test]
    fn names_itself_and_its_scorer() {
        let strategy = BestOfNStrategy::default();
        assert_eq!(strategy.name(), "best-of-n");
        assert_eq!(strategy.scorer().name(), "longest-answer");
        assert_eq!(strategy.candidates(), DEFAULT_CANDIDATES);
        assert_eq!(
            BestOfNStrategy::new(0, Arc::new(LongestAnswer)).candidates(),
            1
        );
        assert!(format!("{strategy:?}").contains("longest-answer"));
    }
}
