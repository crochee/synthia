//! The reasoning-strategy seam: pluggable agent loops.
//!
//! [`crate::ReActAgent`] is one way to reason. This module makes that
//! a choice rather than a fact: a [`ReasoningStrategy`] receives an
//! [`AgentRuntime`] — the provider, the tool registry, the steering
//! bundle, the context manager, the clock, the cancel token, everything
//! the agent was assembled from — plus the caller's [`crate::AgentInput`],
//! and publishes [`crate::events::AgentEvent`]s through an [`EventSink`].
//!
//! ```text
//!   ReActAgent::new(provider, registry)
//!       .with_steering(steering)             ─┐
//!       .with_context_manager(cm)             │  assembled once
//!       .with_typed_event_sink(sink)          │
//!       .with_strategy(Arc::new(MyStrategy)) ─┘
//! ```
//!
//! The workspace ships three: [`crate::agent::ReActStrategy`]
//! (think → act → observe, with tools — the default),
//! [`crate::agent::ChainOfThoughtStrategy`] (one step-by-step
//! completion, no tools), and
//! [`crate::agent::BestOfNStrategy`] (N concurrent samples, scored,
//! winner published — plug a [`crate::agent::CandidateScorer`]).
//! Swapping them changes the *reasoning*, not the plumbing: same
//! provider, same tools, same guards, same context window policy,
//! same durable event stream.
//!
//! ## Why one method, not two
//!
//! Reference implementations often expose `execute()` (returns a final
//! output) and a separate `stream()` that defaults to an error, which
//! makes streaming second-class. Here the event stream **is** the
//! contract: a strategy publishes through [`EventSink`] and returns
//! `()`. Everything the loop learned is already on the stream, every
//! strategy streams, and the agent's `run` is a three-line adapter.
//!
//! ## Errors
//!
//! A strategy signals failure the way [`crate::Agent::run`]
//! documents: by emitting `SystemEvent::SessionEnded { reason }` and
//! returning. The stream itself never yields `Err`. [`EventSink`]
//! carries the three boundary calls that makes uniform —
//! [`EventSink::begin`], [`EventSink::finish`], [`EventSink::fail`].
//!
//! ## Layout
//!
//! The three shipped strategies live under this module because they
//! are three answers to one question. Each file owns one concern, and
//! the shared pieces are shared rather than copied:
//!
//! | File | Concern |
//! |---|---|
//! | `mod.rs` | Seam-level docs, [`ReasoningStrategy`], [`KNOWN_STRATEGY_NAMES`], [`from_name`] |
//! | `runtime.rs` | [`AgentRuntime`] — everything a strategy is handed |
//! | `sink.rs` | [`EventSink`] — publishing, plus the run boundaries |
//! | `request.rs` | The shared one-pass, tool-free request both single-shot strategies ask |
//! | `scorer.rs` | [`CandidateScorer`] + [`LongestAnswer`] — the best-of-N judgement seam |
//! | `cot.rs` | [`ChainOfThoughtStrategy`] — one pass, labelled steps |
//! | `best_of_n.rs` | [`BestOfNStrategy`] — N passes, scored, winner wins |
//! | `llm_judge.rs` | [`LlmJudgeScorer`] — a [`CandidateScorer`] backed by a model call |

mod best_of_n;
mod cot;
mod llm_judge;
mod request;
mod runtime;
mod scorer;
mod sink;

use std::sync::Arc;

use async_trait::async_trait;
pub use best_of_n::BestOfNStrategy;
pub use cot::ChainOfThoughtStrategy;
pub use llm_judge::{DEFAULT_JUDGE_PROMPT, LlmJudgeScorer};
pub use runtime::AgentRuntime;
#[cfg(test)]
pub(crate) use runtime::default_for_test;
pub use scorer::{CandidateScorer, LongestAnswer};
pub use sink::EventSink;

use crate::input::AgentInput;

/// A pluggable reasoning loop.
///
/// Implement this to run a session your way: the runtime hands over
/// every assembled piece and the sink publishes what happens. See the
/// module docs for why there is one method and how errors travel.
///
/// # Object safety
///
/// Held as `Arc<dyn ReasoningStrategy>` — dispatch is dynamic on
/// purpose: an LLM round trip is thousands of times the cost of a
/// vtable call, and dynamic dispatch lets a deployment pick the
/// strategy at runtime (from config, per session) without
/// recompiling.
#[async_trait]
pub trait ReasoningStrategy: Send + Sync + 'static {
    /// Stable label for logs and introspection (`"react"`,
    /// `"chain-of-thought"`, …).
    fn name(&self) -> &str;

    /// Drive one session to completion, publishing events through
    /// `sink`.
    ///
    /// The strategy owns its own concurrency: it may await, spawn
    /// through [`AgentRuntime::spawner`], and consult
    /// [`AgentRuntime::cancel`] between passes. It MUST emit exactly
    /// one terminal `SystemEvent::SessionEnded` — that event is what
    /// tells consumers (and the durable log) the run is over.
    async fn run(
        &self,
        runtime: AgentRuntime,
        input: AgentInput,
        sink: EventSink,
    );
}

/// The strategies this crate ships, by the names a deployment may
/// configure.
///
/// Kept in one place so the error message for an unknown name, the
/// docs, and the server's config validation cannot drift apart.
pub const KNOWN_STRATEGY_NAMES: &[&str] =
    &["react", "chain-of-thought", "best-of-n"];

/// Resolve a strategy by name.
///
/// This is the deployment-facing half of the seam: an operator names a
/// strategy in config (`strategy = "chain-of-thought"`) and the run
/// factory installs it, with no code change. Names are
/// case-insensitive and accept the obvious spellings
/// (`chain-of-thought` / `cot`, `best-of-n` / `bon`).
///
/// The named variants are built with their defaults — `best-of-n` uses
/// its built-in sample count (see `best_of_n::DEFAULT_CANDIDATES`) and
/// [`LongestAnswer`] as the scorer. A deployment that
/// wants a different sample count or a real scorer (a verifier, a
/// test run) assembles the strategy in code via
/// [`crate::agent::ReActAgent::with_strategy`](crate::agent::ReActAgent::with_strategy).
///
/// # Errors
///
/// Returns [`synthia_core::Error::Validation`] naming the accepted
/// values when `name` matches nothing.
pub fn from_name(
    name: &str,
) -> Result<Arc<dyn ReasoningStrategy>, synthia_core::Error> {
    let normalized = name.trim().to_ascii_lowercase();
    let resolved: Arc<dyn ReasoningStrategy> = match normalized.as_str() {
        "react" | "re-act" | "re_act" => Arc::new(crate::agent::ReActStrategy),
        "chain-of-thought" | "chain_of_thought" | "cot" => {
            Arc::new(ChainOfThoughtStrategy::default())
        }
        "best-of-n" | "best_of_n" | "bon" => {
            Arc::new(BestOfNStrategy::default())
        }
        _ => {
            return Err(synthia_core::Error::validation(format!(
                "unknown strategy {name:?}; expected one of {}",
                KNOWN_STRATEGY_NAMES.join(", ")
            )));
        }
    };
    Ok(resolved)
}

#[cfg(test)]
mod tests {
    //! Seam-level tests for the reasoning-strategy module.
    //!
    //! Kept apart from `mod.rs` because the tests touch every piece of
    //! the seam (the trait, the runtime factory, the sink's closed-state)
    //! and the focused unit tests are easier to read when they sit in
    //! their own file.

    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use async_trait::async_trait;
    use futures::{StreamExt as _, channel::mpsc};
    use synthia_core::Error as ProviderError;
    use synthia_provider::{
        CompletionRequest,
        CompletionResponse,
        Content,
        ModelConfig,
        ModelProvider,
        ProviderConfig,
    };

    use super::*;
    use crate::{
        events::{AgentEvent, SessionEndReason, SystemEvent},
        input::AgentInput,
    };

    /// A provider whose only job is to exist — these tests exercise the
    struct NullProvider;

    #[async_trait]
    impl ModelProvider for NullProvider {
        async fn initialize(
            &mut self,
            _config: ProviderConfig,
        ) -> Result<(), ProviderError> {
            Ok(())
        }

        fn name(&self) -> &str {
            "null"
        }

        fn model_config(&self) -> ModelConfig {
            ModelConfig {
                name: "null-1".into(),
                provider: "null".into(),
                context_window: 8_192,
                max_output_tokens: 1_024,
                supports_tools: true,
                supports_streaming: false,
                supports_reasoning: false,
            }
        }

        async fn complete(
            &self,
            _request: CompletionRequest,
        ) -> Result<CompletionResponse, ProviderError> {
            Ok(CompletionResponse {
                content: Content::text("null"),
                ..CompletionResponse::default()
            })
        }
    }

    fn runtime() -> AgentRuntime {
        let provider: Arc<dyn ModelProvider> = Arc::new(NullProvider);
        let mut rt = default_for_test(
            provider,
            synthia_core::AtomicCancelToken::shared(),
        );
        rt.descriptor.name = "seam".into();
        rt.descriptor.kind = "seam".into();
        rt
    }

    /// A strategy can be implemented outside the crate: it sees the
    /// runtime, it can publish, and the trait is object-safe.
    #[test]
    fn a_foreign_strategy_runs_against_the_seam() {
        struct Counting;

        #[async_trait]
        impl ReasoningStrategy for Counting {
            fn name(&self) -> &str {
                "counting"
            }

            async fn run(
                &self,
                runtime: AgentRuntime,
                _input: AgentInput,
                sink: EventSink,
            ) {
                sink.emit_typed(synthia_session::iteration_start(1));
                sink.text_delta(format!(
                    "{} tools are available",
                    runtime.tool_registry.tool_count()
                ));
                sink.emit(AgentEvent::System(SystemEvent::SessionEnded {
                    reason: SessionEndReason::Completed,
                }));
            }
        }

        let strategy: Arc<dyn ReasoningStrategy> = Arc::new(Counting);
        assert_eq!(strategy.name(), "counting");

        let (tx, mut rx) = mpsc::unbounded();
        let sink = EventSink::new(Arc::new(tx), None);
        let seen = AtomicUsize::new(0);
        futures::executor::block_on(async {
            strategy.run(runtime(), AgentInput::text("go"), sink).await;
            while rx.next().await.is_some() {
                seen.fetch_add(1, Ordering::SeqCst);
            }
        });
        assert_eq!(
            seen.load(Ordering::SeqCst),
            2,
            "the text delta and the terminal event must both be published"
        );
    }

    /// Dropping the receiver makes `emit` a no-op and `is_closed`
    /// report it — a strategy can cheaply stop working for a consumer
    /// that left.
    #[test]
    fn emit_after_the_consumer_left_is_a_no_op() {
        let (tx, rx) = mpsc::unbounded();
        let sink = EventSink::new(Arc::new(tx), None);
        assert!(!sink.is_closed());
        drop(rx);
        assert!(sink.is_closed());
        sink.text_delta("into the void");
    }

    /// The runtime exposes the two derivations every strategy needs:
    /// the model config and the assembled system prompt.
    #[test]
    fn runtime_derives_model_config_and_system_prompt() {
        let runtime = runtime();
        assert_eq!(runtime.model_config().context_window, 8_192);
        let prompt = runtime.system_prompt();
        assert!(
            prompt.contains("be brief"),
            "the descriptor's instructions must reach the prompt: {prompt}"
        );
    }

    /// `Debug` is available (strategies log the runtime) and does not
    /// leak the whole registry.
    #[test]
    fn runtime_debug_names_the_agent_and_provider() {
        let text = format!("{:?}", runtime());
        assert!(text.contains("seam"), "{text}");
        assert!(text.contains("null"), "{text}");
    }

    /// Every shipped strategy is reachable by the name a deployment
    /// would write in config, and each spelling an operator is likely
    /// to try resolves to the same strategy.
    #[test]
    fn from_name_resolves_every_shipped_strategy() {
        for (name, expected) in [
            ("react", "react"),
            ("ReAct", "react"),
            ("re-act", "react"),
            ("chain-of-thought", "chain-of-thought"),
            ("chain_of_thought", "chain-of-thought"),
            ("cot", "chain-of-thought"),
            (" CoT ", "chain-of-thought"),
            ("best-of-n", "best-of-n"),
            ("best_of_n", "best-of-n"),
            ("bon", "best-of-n"),
        ] {
            let resolved = from_name(name)
                .unwrap_or_else(|e| panic!("{name:?} must resolve: {e}"));
            assert_eq!(resolved.name(), expected, "for {name:?}");
        }
        assert_eq!(KNOWN_STRATEGY_NAMES.len(), 3);
    }

    /// An unknown name is a validation error that names the accepted
    /// values — the operator reads it in a boot log, so it has to be
    /// actionable rather than "invalid config".
    #[test]
    fn from_name_rejects_an_unknown_name_with_the_accepted_list() {
        let err = match from_name("mcts") {
            Ok(strategy) => {
                panic!("mcts must not resolve, got {}", strategy.name())
            }
            Err(err) => err,
        };
        let message = err.to_string();
        assert!(message.contains("mcts"), "{message}");
        for name in KNOWN_STRATEGY_NAMES {
            assert!(
                message.contains(name),
                "the error must name {name}: {message}"
            );
        }
    }
}
