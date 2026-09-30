//! Chain-of-Thought: the second reasoning loop, shipped to prove the
//! seam.
//!
//! [`ReActStrategy`](super::super::ReActStrategy) interleaves thinking
//! with tool calls. This one does the opposite: **one** completion, no
//! tools, with a prompt that asks the model to reason in labelled
//! steps and finish with an explicit answer line. It exists for two
//! reasons:
//!
//! 1. **It is useful.** Plenty of work has no tool component — a
//!    classification, a rewrite, a plan, a review. Offering tools and
//!    looping costs a round trip per tool and invites the model to
//!    reach for `shell` where reading the prompt would do.
//! 2. **It proves the seam.** Everything else is shared: the same
//!    provider, the same system-prompt assembler, the same
//!    [`ContextManager`](synthia_context::ContextManager) fitting the
//!    window, the same cancel token, the same durable event stream.
//!    If this file can be written without touching the agent, the
//!    strategy seam is real.
//!
//! ```text
//! ReActAgent + ReActStrategy          → think → act → observe → … → answer
//! ReActAgent + ChainOfThoughtStrategy → think step by step → answer
//!                ↑ same runtime, same tools available, no tool calls made
//! ```
//!
//! ## Honest scope
//!
//! "Chain-of-Thought" here means *prompt-engineered* step-by-step
//! reasoning in a single pass, with the steps exposed as labelled text
//! the caller can parse ([`ChainOfThoughtStrategy::parse_steps`]). It
//! is not a multi-pass self-critique loop and does not verify its own
//! answer — a verified answer is a different composition: score the
//! samples with [`CandidateScorer`](super::CandidateScorer), or run a
//! host-driven plan with the separate `synthia-workflow` crate.

use std::sync::Arc;

use async_trait::async_trait;
use synthia_provider::{CompletionRequest, StreamChunk};
use synthia_session::{iteration_end, iteration_start};

use super::request::single_shot_request;
use crate::{
    agent::strategy::{AgentRuntime, EventSink, ReasoningStrategy},
    events::{AgentEvent, SystemEvent, WarningKind},
    input::AgentInput,
};

/// How many reasoning steps the instruction asks for, when the caller
/// does not say.
pub(crate) const DEFAULT_COT_STEPS: usize = 5;

/// One-pass, tool-free step-by-step reasoning.
///
/// See the module docs for what it is (and is not).
#[derive(Debug, Clone, Copy)]
pub struct ChainOfThoughtStrategy {
    max_steps: usize,
}

impl Default for ChainOfThoughtStrategy {
    fn default() -> Self {
        Self::new(DEFAULT_COT_STEPS)
    }
}

impl ChainOfThoughtStrategy {
    /// A strategy that asks for at most `max_steps` labelled steps.
    ///
    /// `max_steps` clamps to at least 1: "no steps" is not a
    /// configuration, it is a bug.
    #[must_use]
    pub fn new(max_steps: usize) -> Self {
        Self {
            max_steps: max_steps.max(1),
        }
    }

    /// The configured step bound.
    #[must_use]
    pub fn max_steps(&self) -> usize {
        self.max_steps
    }

    /// The instruction appended to the system prompt.
    ///
    /// Kept as a method (not a constant) so the bound appears in it and
    /// a test can pin the wording that makes the strategy what it is.
    #[must_use]
    pub fn instruction(&self) -> String {
        format!(
            "Reason before you answer. Work through at most {} numbered \
             steps, each on its own line and labelled `Step N:`. Then \
             write a final line starting with `Answer:` that contains \
             only the answer itself. Do not call tools; you have none.",
            self.max_steps
        )
    }

    /// The labelled reasoning steps in `text`, in order.
    ///
    /// Recognises `Step N:` / `Thought N:` at the start of a line
    /// (case-insensitive, after optional Markdown list markers and
    /// bold/italic emphasis), so it works on plain text and on the
    /// Markdown models actually emit. A line that merely *mentions* a
    /// step does not count; the label must open the line.
    ///
    /// This is the "chain" the strategy promises: parse it out of the
    /// answer when you want to show, score, or log the reasoning
    /// separately from the conclusion.
    #[must_use]
    pub fn parse_steps(text: &str) -> Vec<String> {
        text.lines()
            .filter_map(strip_step_label)
            .map(str::to_string)
            .collect()
    }

    /// The single streaming pass, with deltas published as they arrive
    /// so a consumer renders the chain live.
    ///
    /// Returns the name of a tool the model tried to call (this strategy
    /// advertises none) plus the pass's outcome — `Err` carrying the
    /// provider's own message, which becomes the run's end reason.
    async fn stream_once(
        &self,
        runtime: &AgentRuntime,
        request: CompletionRequest,
        sink: &EventSink,
    ) -> (Option<String>, Result<(), String>) {
        // The tool-call slot is shared because the callback is
        // `FnMut + Send + 'static`: it reports an attempt, the
        // strategy warns about it after the pass.
        let attempt = Arc::new(parking_lot::Mutex::new(None));
        let slot = Arc::clone(&attempt);
        let stream_sink = sink.clone();
        let outcome = runtime
            .provider
            .complete_with_stream(
                request,
                Some(Arc::clone(&runtime.cancel)),
                Box::new(move |chunk| match chunk {
                    StreamChunk::Content(part) => {
                        stream_sink.emit(AgentEvent::Model(part));
                    }
                    StreamChunk::Usage(usage) => {
                        stream_sink.emit(AgentEvent::usage(
                            usage.prompt_tokens,
                            usage.completion_tokens,
                            usage.cache_read_tokens,
                            usage.cache_write_tokens,
                        ));
                    }
                    StreamChunk::IsDone { result } => {
                        if let Some(first) = result.tool_calls.first() {
                            *slot.lock() = Some(first.name.clone());
                        }
                        stream_sink.emit(AgentEvent::ModelDone(*result));
                    }
                    StreamChunk::Stop(_)
                    | StreamChunk::ToolCallStart { .. }
                    | StreamChunk::ToolCallDelta { .. }
                    | StreamChunk::ToolCallEnd { .. } => {}
                }),
            )
            .await;
        let attempted = attempt.lock().clone();
        (
            attempted,
            outcome.map(|_| ()).map_err(|err| err.to_string()),
        )
    }
}

/// `Some(rest)` when `line` opens with a step label, `None` otherwise.
fn strip_step_label(line: &str) -> Option<&str> {
    let mut rest = line.trim_start();
    // Markdown list marker / emphasis / heading noise.
    for prefix in ["- ", "* ", "+ ", "# ", "> "] {
        if let Some(stripped) = rest.strip_prefix(prefix) {
            rest = stripped.trim_start();
        }
    }
    rest = rest.trim_start_matches(['*', '_', '`']);

    for label in ["step", "thought"] {
        let Some(after) = rest.get(..label.len()) else {
            continue;
        };
        if !after.eq_ignore_ascii_case(label) {
            continue;
        }
        let tail = &rest[label.len()..];
        // Optional number, then the colon.
        let tail = tail
            .trim_start()
            .trim_start_matches(|c: char| c.is_ascii_digit());
        if let Some(body) = tail.strip_prefix(':') {
            return Some(body.trim_start_matches(['*', '_', '`']).trim());
        }
    }
    None
}

#[async_trait]
impl ReasoningStrategy for ChainOfThoughtStrategy {
    fn name(&self) -> &str {
        "chain-of-thought"
    }

    async fn run(
        &self,
        runtime: AgentRuntime,
        input: AgentInput,
        sink: EventSink,
    ) {
        if !sink.begin(&runtime.cancel) {
            return;
        }

        let instruction = self.instruction();
        let request =
            single_shot_request(&runtime, &input, Some(&instruction)).await;

        sink.emit_typed(iteration_start(1));
        let (attempted, outcome) =
            self.stream_once(&runtime, request, &sink).await;
        sink.emit_typed(iteration_end(1, "final_answer"));

        // The strategy advertises no tools; a model that emits a call
        // anyway is worth surfacing rather than silently dropping.
        if let Some(name) = attempted {
            sink.emit(AgentEvent::System(SystemEvent::Warning {
                kind: WarningKind::Loop,
                message: format!(
                    "chain-of-thought offers no tools, but the model called \
                     `{name}`; the call was ignored. Use ReActStrategy for \
                     tool work."
                ),
                iteration: Some(1),
            }));
        }

        match outcome {
            Ok(()) => sink.finish(&runtime.cancel),
            Err(message) => sink.fail(message),
        }
    }
}

#[cfg(test)]
mod tests {
    //! `ChainOfThoughtStrategy` behaviour tests.
    //!
    //! Split from the bottom of `cot.rs` (R102): the strategy's
    //! single-pass contract (no tools offered, the bound reaching the
    //! prompt, the context policy consulted once, cancellation and
    //! failure shapes) reads next to its fixtures in its own file.

    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use async_trait::async_trait;
    use parking_lot::Mutex;
    use synthia_context::{
        AgentState,
        ContextManager,
        TruncatingContextManager,
    };
    use synthia_core::Error as ProviderError;
    use synthia_provider::{
        CompletionResponse,
        Content,
        ModelConfig,
        ProviderConfig,
        TokenUsage,
        ToolChoice,
    };

    use super::*;
    use crate::{
        SessionEndReason,
        agent::{AgentRuntime, EventSink},
    };

    /// Records what it was asked, then answers with a fixed script.
    struct ScriptedProvider {
        requests: Mutex<Vec<CompletionRequest>>,
        answer: String,
        fail: bool,
    }

    impl ScriptedProvider {
        fn new(answer: &str) -> Arc<Self> {
            Arc::new(Self {
                requests: Mutex::new(Vec::new()),
                answer: answer.to_string(),
                fail: false,
            })
        }

        fn failing() -> Arc<Self> {
            Arc::new(Self {
                requests: Mutex::new(Vec::new()),
                answer: String::new(),
                fail: true,
            })
        }
    }

    #[async_trait]
    impl synthia_provider::traits::ModelProvider for ScriptedProvider {
        async fn initialize(
            &mut self,
            _config: ProviderConfig,
        ) -> Result<(), ProviderError> {
            Ok(())
        }

        fn name(&self) -> &str {
            "scripted-cot"
        }

        fn model_config(&self) -> ModelConfig {
            ModelConfig {
                name: "cot-test".into(),
                provider: "scripted-cot".into(),
                context_window: 8_192,
                max_output_tokens: 512,
                supports_tools: true,
                supports_streaming: true,
                supports_reasoning: false,
            }
        }

        async fn complete(
            &self,
            request: CompletionRequest,
        ) -> Result<CompletionResponse, ProviderError> {
            self.requests.lock().push(request);
            if self.fail {
                return Err(ProviderError::internal("scripted failure"));
            }
            Ok(CompletionResponse {
                content: Content::text(self.answer.clone()),
                usage: TokenUsage {
                    prompt_tokens: 10,
                    completion_tokens: 5,
                    total_tokens: 15,
                    ..TokenUsage::default()
                },
                ..CompletionResponse::default()
            })
        }
    }

    /// A manager that records how many times the window policy ran.
    struct SpyManager {
        calls: Arc<AtomicUsize>,
        inner: TruncatingContextManager,
    }

    #[async_trait]
    impl ContextManager for SpyManager {
        async fn prepare(
            &self,
            messages: &mut Vec<synthia_provider::Message>,
            state: &mut AgentState,
        ) {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.inner.prepare(messages, state).await;
        }
    }

    fn runtime(
        provider: Arc<dyn synthia_provider::traits::ModelProvider>,
        context_manager: Arc<dyn ContextManager>,
        cancel: Arc<dyn synthia_core::CancelToken>,
    ) -> AgentRuntime {
        let mut rt = crate::agent::strategy::default_for_test(provider, cancel);
        rt.descriptor.name = "cot".into();
        rt.descriptor.kind = "cot".into();
        rt.descriptor.instructions = "You are terse.".into();
        rt.context_manager = context_manager;
        rt
    }

    /// Run the strategy and collect every event it published.
    fn drive(
        strategy: &ChainOfThoughtStrategy,
        runtime: AgentRuntime,
    ) -> Vec<AgentEvent> {
        let (tx, mut rx) = futures::channel::mpsc::unbounded();
        let sink = EventSink::new(Arc::new(tx), None);
        // Every event is published before `run` returns, so draining
        // the queue afterwards loses nothing.
        futures::executor::block_on(strategy.run(
            runtime,
            AgentInput::text("What is 2 + 2?"),
            sink,
        ));
        let mut events = Vec::new();
        while let Ok(event) = rx.try_recv() {
            events.push(event);
        }
        events
    }

    fn reason_of(events: &[AgentEvent]) -> Option<SessionEndReason> {
        events.iter().find_map(|e| match e {
            AgentEvent::System(SystemEvent::SessionEnded { reason }) => {
                Some(reason.clone())
            }
            _ => None,
        })
    }

    /// The defining difference from ReAct: the request carries **no
    /// tools**, and the system prompt carries the step instruction.
    #[test]
    fn cot_offers_no_tools_and_asks_for_steps() {
        let provider = ScriptedProvider::new("Step 1: 2 + 2 = 4\nAnswer: 4");
        let strategy = ChainOfThoughtStrategy::new(3);
        let events = drive(
            &strategy,
            runtime(
                provider.clone(),
                Arc::new(TruncatingContextManager),
                synthia_core::AtomicCancelToken::shared(),
            ),
        );

        let requests = provider.requests.lock();
        assert_eq!(requests.len(), 1, "one pass, one request");
        assert!(
            requests[0].tools.is_empty(),
            "chain-of-thought must not offer tools"
        );
        assert!(matches!(requests[0].tool_choice, ToolChoice::None));
        let system = requests[0].messages[0].content.extract_text().unwrap();
        assert!(
            system.contains("at most 3 numbered"),
            "the bound must reach the prompt: {system}"
        );
        assert!(system.contains("You are terse."));

        assert_eq!(
            reason_of(&events),
            Some(SessionEndReason::Completed),
            "events: {events:?}"
        );
        assert!(
            events.iter().any(|e| matches!(e, AgentEvent::ModelDone(_))),
            "the sampling result must be published"
        );
    }

    /// The runtime's context policy is consulted exactly once — the
    /// strategy does not reimplement window management.
    #[test]
    fn cot_uses_the_runtime_context_manager() {
        let provider = ScriptedProvider::new("Answer: 4");
        let calls = Arc::new(AtomicUsize::new(0));
        let manager: Arc<dyn ContextManager> = Arc::new(SpyManager {
            calls: Arc::clone(&calls),
            inner: TruncatingContextManager,
        });
        let _ = drive(
            &ChainOfThoughtStrategy::default(),
            runtime(
                provider,
                manager,
                synthia_core::AtomicCancelToken::shared(),
            ),
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    /// A pre-cancelled run ends as `Cancelled` without spending a
    /// request.
    #[test]
    fn cot_honours_a_pre_cancelled_token() {
        let provider = ScriptedProvider::new("Answer: 4");
        let cancel: Arc<dyn synthia_core::CancelToken> =
            synthia_core::AtomicCancelToken::shared();
        cancel.cancel();
        let events = drive(
            &ChainOfThoughtStrategy::default(),
            runtime(
                provider.clone(),
                Arc::new(TruncatingContextManager),
                cancel,
            ),
        );
        assert_eq!(reason_of(&events), Some(SessionEndReason::Cancelled));
        assert!(
            provider.requests.lock().is_empty(),
            "no request may be made after cancellation"
        );
    }

    /// A provider failure becomes a terminal `Error` reason — the
    /// stream still ends with a `SessionEnded`, never with a panic.
    #[test]
    fn cot_reports_provider_failure_as_session_end() {
        let events = drive(
            &ChainOfThoughtStrategy::default(),
            runtime(
                ScriptedProvider::failing(),
                Arc::new(TruncatingContextManager),
                synthia_core::AtomicCancelToken::shared(),
            ),
        );
        match reason_of(&events) {
            Some(SessionEndReason::Error(message)) => {
                assert!(
                    message.contains("scripted failure"),
                    "the provider's message must survive: {message}"
                );
            }
            other => panic!("expected an Error end reason, got {other:?}"),
        }
    }

    /// `parse_steps` extracts the labelled chain and ignores prose that
    /// merely mentions a step.
    #[test]
    fn parse_steps_extracts_labelled_lines_only() {
        let text = "\
    Step 1: gather the numbers
    **Step 2:** add them
    - Thought 3: sanity-check the result
    As I noted in step 2 above, this is fine.
    Answer: 4";
        let steps = ChainOfThoughtStrategy::parse_steps(text);
        assert_eq!(
            steps,
            vec![
                "gather the numbers".to_string(),
                "add them".to_string(),
                "sanity-check the result".to_string(),
            ],
            "only label-opened lines count"
        );
    }

    /// The instruction's bound is the configured one, and `max_steps(0)`
    /// clamps rather than producing a strategy that forbids reasoning.
    #[test]
    fn step_bound_is_configurable_and_never_zero() {
        assert!(
            ChainOfThoughtStrategy::new(7)
                .instruction()
                .contains("at most 7")
        );
        assert_eq!(ChainOfThoughtStrategy::new(0).max_steps(), 1);
        assert_eq!(
            ChainOfThoughtStrategy::default().max_steps(),
            DEFAULT_COT_STEPS
        );
    }

    /// The strategy names itself — the label logs and the agent's
    /// introspection use.
    #[test]
    fn strategy_names_itself() {
        assert_eq!(
            ChainOfThoughtStrategy::default().name(),
            "chain-of-thought"
        );
    }
}
