use std::sync::Arc;

use synthia_command::registry::CommandRegistry;
use synthia_context::assembler::ContextAssembler;
use synthia_hook::HookRegistry;
use synthia_memory::types::MemoryEvent;
use synthia_permission::ApprovalService;
use synthia_provider::{
    registry::ProviderRegistry,
    router::ModelRouter,
    traits::ModelProvider,
    types::Message,
};
use synthia_sandbox::SandboxManager;
use synthia_session::{Store as SessionStore, manager::SessionManager};
use synthia_tool::registry::ToolRegistry;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::{
    checkpoint,
    config::{AgentConfig, AgentRunConfig, AgentRunStateConfig},
    config_watcher::MultiConfigWatcher,
    control::{AgentControl, AgentRegistry},
    steering::SteeringChannel,
    stream_builder::StreamBuilder,
    types::*,
};

// OpenTelemetry context injection (Task 5 of otel-feature-integration).
//
// When the `otel` cargo feature is enabled, the helpers below wrap the
// `AgentOutput` stream so that every poll of the inner stream happens
// inside six nested `tokio::task_local` scopes. The
// `SpanAttributesProcessor` (in `synthia-telemetry`) reads these
// task-locals in its `on_start` hook and injects the corresponding span
// attributes (`session.id` / `user.id` / `agent.id` / `turn.id` /
// `gen_ai.system` / `gen_ai.request.model`).
//
// The wrapper drives the inner stream with a top-level `loop` + `yield`
// inside an `async_stream::stream!` block; the `yield` stays at the
// macro's top level (not nested inside an `async {}` block) so the
// `async_stream` generator transformation is unaffected. Each `next()`
// poll of the inner stream is awaited within the nested task_local
// scopes, so the task-locals are reachable from any span created while
// the stream is being driven (e.g. `llm.call` / `tool.execute` spans
// emitted by downstream crates).
#[cfg(feature = "otel")]
pub(crate) mod otel_context {
    use std::future::Future;

    use async_stream::stream;
    use futures::StreamExt;
    use synthia_telemetry::span::attributes_processor::{
        AGENT_ID,
        GEN_AI_REQUEST_MODEL,
        GEN_AI_SYSTEM,
        SESSION_ID,
        TURN_ID,
        USER_ID,
    };
    use tracing::Instrument;

    use super::{AgentOutput, AgentRunConfig};

    /// Snapshot of the six task-local values populated at stream entry.
    ///
    /// `agent_id` and `turn_id` are not directly available at the
    /// `run_stream` / `run_stream_with_state` boundary:
    /// - `agent_id` — there is no explicit agent-instance ID field on
    ///   `AgentConfig` / `AgentRunConfig`; left empty here. A later
    ///   task can populate it (e.g. from `AgentInstance` once the agent
    ///   is registered) or the processor will graceful-skip it.
    /// - `turn_id` — generated per-iteration inside the main loop
    ///   (`LoopContext::assign_new_turn_id`). Task 7 handles
    ///   turn-level spans; the empty default here is a placeholder.
    pub(crate) struct OtelContext {
        pub session_id: String,
        pub user_id: String,
        pub agent_id: String,
        pub turn_id: String,
        pub gen_ai_system: String,
        pub gen_ai_request_model: String,
    }

    impl OtelContext {
        /// Build the context snapshot from an `AgentRunConfig`.
        ///
        /// `gen_ai_system` comes from `ModelProvider::name` (e.g.
        /// `"anthropic"` / `"openai"`); `gen_ai_request_model` from the
        /// static `AgentConfig::model` field.
        pub(crate) fn from_run_config(run_config: &AgentRunConfig) -> Self {
            Self {
                session_id: run_config.session_id.clone(),
                user_id: run_config.user_id.clone(),
                agent_id: String::new(),
                turn_id: String::new(),
                gen_ai_system: run_config.provider.name().to_string(),
                gen_ai_request_model: run_config.config.model.clone(),
            }
        }
    }

    /// Run `fut` with all six task-locals set simultaneously.
    ///
    /// The scopes are nested so that every task-local is in scope for
    /// the duration of `fut`. Each `String` is cloned into its scope
    /// (the values are cheap to clone and the cloning happens once per
    /// stream poll, which is negligible compared to the work inside).
    async fn with_otel_context<R, F>(ctx: &OtelContext, fut: F) -> R
    where
        F: Future<Output = R> + Send,
        R: Send,
    {
        SESSION_ID
            .scope(ctx.session_id.clone(), async {
                USER_ID.scope(ctx.user_id.clone(), async {
                    AGENT_ID.scope(ctx.agent_id.clone(), async {
                        TURN_ID.scope(ctx.turn_id.clone(), async {
                            GEN_AI_SYSTEM.scope(ctx.gen_ai_system.clone(), async {
                                GEN_AI_REQUEST_MODEL
                                    .scope(ctx.gen_ai_request_model.clone(), fut)
                                    .await
                            })
                            .await
                        })
                        .await
                    })
                    .await
                })
                .await
            })
            .await
    }

    /// RAII guard for the `session.start` root span.
    ///
    /// Holds a clone of the session span for the duration of the
    /// stream's lifetime. On drop (normal or panic), the span is ended
    /// via the inner `Span`'s `Drop`, which triggers `on_end` on the
    /// OTel processor. If dropped during a panic, an
    /// `exception.message` field is recorded on the span first, so the
    /// `tracing-opentelemetry` layer can mark the span as errored.
    ///
    /// This guard is `#[must_use]` because dropping it immediately
    /// would end the span prematurely.
    #[must_use]
    pub(crate) struct SessionSpanGuard {
        span: tracing::Span,
    }

    impl SessionSpanGuard {
        pub(crate) fn new(span: tracing::Span) -> Self {
            Self { span }
        }
    }

    impl Drop for SessionSpanGuard {
        fn drop(&mut self) {
            if std::thread::panicking() {
                // Record an exception event on the span. The
                // `tracing-opentelemetry` layer translates the
                // `exception.message` field into an OTel exception
                // event, which typically also sets the span status to
                // `Error`. This satisfies the spec requirement that
                // the session span's status is `Error` when `run_stream`
                // panics.
                self.span.record("exception.message", "session panicked");
            }
            // The `Span` itself is dropped when `self` is dropped,
            // which triggers `on_end` on the OTel processor — ending
            // the span. This runs on both normal return and panic
            // unwinding, satisfying the spec requirement that the span
            // is ended via RAII even on panic.
        }
    }

    /// RAII guard for the `turn.start` span.
    ///
    /// Created at the top of each turn iteration in the agent loop
    /// (`StreamBuilder::run_with_steps`). The span's parent is
    /// auto-inherited from `tracing::Span::current()`, which is the
    /// `session.start` span established by `wrap_output_with_otel`.
    ///
    /// The guard holds the span for the duration of the turn iteration.
    /// On drop (normal completion, `continue`, `break`, `return`, or
    /// panic unwinding), the span is ended via the inner `Span`'s
    /// `Drop`, which triggers `on_end` on the OTel processor.
    ///
    /// If the turn fails, call [`TurnSpanGuard::record_error`] before
    /// the control flow exits the iteration scope; this records the
    /// OTel exception fields (`exception.type` / `exception.message`)
    /// and sets `otel.status_code` to `ERROR` on the span. If the
    /// guard is dropped during a panic AND `record_error` was not
    /// called, an `exception.message` field is recorded as a fallback.
    ///
    /// This guard is `#[must_use]` because dropping it immediately
    /// would end the span prematurely.
    #[must_use]
    pub(crate) struct TurnSpanGuard {
        span: tracing::Span,
        error_recorded: bool,
    }

    impl TurnSpanGuard {
        /// Create a `turn.start` span with `turn.id` and `turn.iteration`
        /// attributes, wrapped in a guard.
        ///
        /// The span's parent is auto-inherited from
        /// `tracing::Span::current()` (the `session.start` span). The
        /// `turn.id` attribute uses the `TurnId`'s inner `Uuid`
        /// (`Display`-formatted); `turn.iteration` is the 1-based
        /// iteration counter from `LoopContext::iteration`.
        pub(crate) fn create(
            turn_id: &crate::turn::TurnId,
            iteration: usize,
        ) -> Self {
            // The `exception.*` / `otel.status_code` fields are declared
            // as `Empty` so that `Span::record` can populate them later
            // via `record_error`. In `tracing`, `Span::record(field, value)`
            // looks up the field by name in the span's callsite; if the
            // field is not declared in the `span!` macro, the call is a
            // silent no-op. Declaring them as `Empty` reserves the field
            // slot so `record` can populate it without changing the
            // span's initial attribute set.
            let span = tracing::span!(
                target: "synthia.turn",
                tracing::Level::INFO,
                "turn.start",
                turn.id = %turn_id.0,
                turn.iteration = iteration,
                exception.type = tracing::field::Empty,
                exception.message = tracing::field::Empty,
                otel.status_code = tracing::field::Empty,
            );
            Self {
                span,
                error_recorded: false,
            }
        }

        /// Record an exception event on the span and mark the status
        /// as `Error`.
        ///
        /// This records the OTel semantic-convention exception fields:
        /// - `exception.type` — the error type / category
        /// - `exception.message` — the error message
        /// - `otel.status_code` — set to `"ERROR"`
        ///
        /// The `tracing-opentelemetry` layer translates
        /// `exception.message` into an OTel exception event on the
        /// span. Calling this more than once overwrites the previous
        /// values; the first call sets `error_recorded = true` so the
        /// `Drop` impl's panic fallback is suppressed.
        pub(crate) fn record_error(
            &mut self,
            error_type: &str,
            error_message: &str,
        ) {
            self.span.record("exception.type", error_type);
            self.span.record("exception.message", error_message);
            self.span.record("otel.status_code", "ERROR");
            self.error_recorded = true;
        }
    }

    impl Drop for TurnSpanGuard {
        fn drop(&mut self) {
            if std::thread::panicking() && !self.error_recorded {
                // Fallback: if the turn panicked without an explicit
                // `record_error` call, record a generic exception so
                // the span status is marked as `Error` by the
                // `tracing-opentelemetry` layer.
                self.span.record("exception.message", "turn panicked");
            }
            // The `Span` itself is dropped when `self` is dropped,
            // which triggers `on_end` on the OTel processor — ending
            // the span. This runs on normal completion, `continue`,
            // `break`, `return`, and panic unwinding.
        }
    }

    /// Wrap an `AgentOutput` stream so every poll executes inside the
    /// six task-local scopes AND inside a `session.start` root span.
    ///
    /// The wrapper is itself a `Send` stream: the inner stream is
    /// `Send`, the task-local values are `String` (`Send + 'static`),
    /// the scope futures are `Send`, and the `tracing::Span` is
    /// `Send + Sync + Clone`. The `yield` is at the `stream!` macro's
    /// top level (inside `loop` / `match`), never inside a nested
    /// `async {}` block, so `async_stream`'s generator transformation
    /// handles it correctly.
    ///
    /// # Session span lifecycle
    ///
    /// 1. **Creation**: The `session.start` span is created INSIDE the
    ///    task_local scope (via `with_otel_context`) so the
    ///    `SpanAttributesProcessor::on_start` hook can read the
    ///    task-local values and inject them as span attributes
    ///    (`session.id` / `user.id` / etc.). `parent: None` ensures
    ///    the span is a root span (no parent), per the spec.
    /// 2. **Lifetime**: The span is held by a `SessionSpanGuard` for
    ///    the entire stream lifetime. Each `inner.next()` poll is
    ///    wrapped with `instrument(span.clone())` so the session span
    ///    is the "current" span during the poll — child spans (turn /
    ///    llm.call / tool.execute) created inside have this span as
    ///    parent.
    /// 3. **End**: When the stream ends (normal `None` or panic), the
    ///    generator's state is dropped, which drops the guard, which
    ///    ends the span (calling `on_end` on the OTel processor). On
    ///    panic, an `exception.message` field is recorded first.
    pub(crate) fn wrap_output_with_otel(
        stream: AgentOutput,
        ctx: OtelContext,
    ) -> AgentOutput {
        Box::pin(stream! {
            let mut inner = stream;
            // Create the `session.start` root span INSIDE the task_local
            // scope so `SpanAttributesProcessor::on_start` can read the
            // task-local values and inject them as span attributes.
            // `parent: None` ensures the span is a root span (no parent),
            // per the spec requirement.
            //
            // The `exception.message` field is declared as `Empty` so the
            // `SessionSpanGuard`'s `Drop` impl can record it via
            // `Span::record("exception.message", ...)` when the session
            // panics. Without this callsite declaration, `Span::record` is
            // a silent no-op (Task 7 lesson: undeclared fields cannot be
            // populated by `record`). The field is `Empty` initially — it
            // does not appear in the span's serialized attributes unless
            // `Drop` records a value during panic unwinding.
            let session_span = with_otel_context(&ctx, async {
                tracing::span!(
                    target: "synthia.session",
                    parent: None,
                    tracing::Level::INFO,
                    "session.start",
                    exception.message = tracing::field::Empty,
                )
            })
            .await;
            // Hold the guard for the entire stream lifetime. On drop
            // (normal or panic), the span is ended. If dropping during
            // a panic, an `exception.message` field is recorded first.
            let _session_guard = SessionSpanGuard::new(session_span.clone());
            loop {
                // `instrument` makes the session span the "current"
                // span during `inner.next()`, so child spans created
                // inside (turn / llm.call / tool.execute) have this
                // span as parent.
                let next = with_otel_context(
                    &ctx,
                    inner.next().instrument(session_span.clone()),
                )
                .await;
                match next {
                    Some(item) => yield item,
                    None => return,
                }
            }
        })
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[tokio::test]
        async fn with_otel_context_sets_all_six_task_locals() {
            let ctx = OtelContext {
                session_id: "sess-1".to_string(),
                user_id: "user-1".to_string(),
                agent_id: "agent-1".to_string(),
                turn_id: "turn-1".to_string(),
                gen_ai_system: "anthropic".to_string(),
                gen_ai_request_model: "claude-3-5-sonnet".to_string(),
            };
            // Inside `with_otel_context`, all six task-locals must be
            // reachable with the expected values.
            with_otel_context(&ctx, async {
                assert_eq!(SESSION_ID.get().as_str(), "sess-1");
                assert_eq!(USER_ID.get().as_str(), "user-1");
                assert_eq!(AGENT_ID.get().as_str(), "agent-1");
                assert_eq!(TURN_ID.get().as_str(), "turn-1");
                assert_eq!(GEN_AI_SYSTEM.get().as_str(), "anthropic");
                assert_eq!(
                    GEN_AI_REQUEST_MODEL.get().as_str(),
                    "claude-3-5-sonnet"
                );
            })
            .await;
        }

        #[tokio::test]
        async fn task_locals_not_reachable_outside_scope() {
            // Outside any `scope()` call, `try_get` must return `Err`.
            assert!(SESSION_ID.try_get().is_err());
            assert!(USER_ID.try_get().is_err());
            assert!(AGENT_ID.try_get().is_err());
            assert!(TURN_ID.try_get().is_err());
            assert!(GEN_AI_SYSTEM.try_get().is_err());
            assert!(GEN_AI_REQUEST_MODEL.try_get().is_err());
        }

        #[tokio::test]
        async fn wrap_output_propagates_task_locals_during_poll() {
            use crate::events::AgentEvent;

            // Inner stream reads `SESSION_ID` *when polled* (not at
            // construction). The outer wrapper polls it inside
            // `with_otel_context`, so the task-local must be reachable.
            let inner: AgentOutput = Box::pin(stream! {
                // `try_get` returns an owned `String` (it clones
                // internally), so no further clone is needed.
                let msg = SESSION_ID
                    .try_get()
                    .unwrap_or_else(|_| "no-task-local".to_string());
                yield AgentEvent::Warning { message: msg };
            });
            let ctx = OtelContext {
                session_id: "sess-2".to_string(),
                user_id: String::new(),
                agent_id: String::new(),
                turn_id: String::new(),
                gen_ai_system: String::new(),
                gen_ai_request_model: String::new(),
            };
            let wrapped = wrap_output_with_otel(inner, ctx);
            let collected: Vec<AgentEvent> = wrapped.collect().await;
            assert_eq!(collected.len(), 1);
            if let AgentEvent::Warning { message } = &collected[0] {
                // Proves the task-local was set when the inner stream
                // was polled by the wrapper.
                assert_eq!(message, "sess-2");
            } else {
                panic!("expected AgentEvent::Warning");
            }
        }

        /// Regression test for Task 6.7: `wrap_output_with_otel` must
        /// still produce a working stream after the `session.start`
        /// span is added. Verifies the stream can be collected and the
        /// session span guard is dropped (ending the span) when the
        /// stream completes.
        ///
        /// Note: verifying the span is actually exported requires a
        /// tracing subscriber + OTel provider setup, which is too heavy
        /// for a unit test. Rust's `Drop` semantics guarantee the span
        /// is ended when the generator's state is dropped (after
        /// `collect` returns). The panic-path test below verifies the
        /// guard's `Drop` runs during unwinding.
        #[tokio::test]
        async fn session_span_created_and_ended_on_normal_completion() {
            use crate::events::AgentEvent;

            let inner: AgentOutput = Box::pin(stream! {
                yield AgentEvent::Warning {
                    message: "session-span-normal".to_string(),
                };
            });
            let ctx = OtelContext {
                session_id: "sess-normal".to_string(),
                user_id: "user-normal".to_string(),
                agent_id: String::new(),
                turn_id: String::new(),
                gen_ai_system: "test".to_string(),
                gen_ai_request_model: "test-model".to_string(),
            };
            let wrapped = wrap_output_with_otel(inner, ctx);
            let collected: Vec<AgentEvent> = wrapped.collect().await;
            assert_eq!(collected.len(), 1);
            if let AgentEvent::Warning { message } = &collected[0] {
                assert_eq!(message, "session-span-normal");
            } else {
                panic!("expected AgentEvent::Warning");
            }
            // After `collect` returns, the generator (and thus the
            // `SessionSpanGuard`) has been dropped. The span is ended.
        }

        /// Regression test for Task 6.7: when the inner stream panics,
        /// the panic must propagate through the wrapper, and the
        /// `SessionSpanGuard`'s `Drop` must run during unwinding
        /// (recording an `exception.message` on the span before ending
        /// it).
        ///
        /// We use `FutureExt::catch_unwind` to catch the panic and
        /// verify it propagated. The guard's `Drop` is called during
        /// unwinding (before `catch_unwind` catches the panic), so the
        /// span is ended with the exception event recorded.
        #[tokio::test]
        async fn session_span_ended_on_panic() {
            use std::panic::AssertUnwindSafe;

            use futures::FutureExt;

            use crate::events::AgentEvent;

            // Yield one event so `stream!` infers `Item = AgentEvent`,
            // then panic on the next poll. The panic propagates
            // through `wrap_output_with_otel` to `collect`.
            let inner: AgentOutput = Box::pin(stream! {
                yield AgentEvent::Warning {
                    message: "pre-panic".to_string(),
                };
                panic!("test panic in inner stream");
            });
            let ctx = OtelContext {
                session_id: "sess-panic".to_string(),
                user_id: "user-panic".to_string(),
                agent_id: String::new(),
                turn_id: String::new(),
                gen_ai_system: "test".to_string(),
                gen_ai_request_model: "test-model".to_string(),
            };
            let wrapped = wrap_output_with_otel(inner, ctx);
            // `AssertUnwindSafe` is needed because `AgentOutput` (a
            // `Pin<Box<dyn Stream + Send>>`) is not `UnwindSafe` by
            // default. The wrapper's state (task-locals, span guard)
            // is safe to unwind — the guard's `Drop` is designed for
            // this exact path.
            let result = AssertUnwindSafe(wrapped.collect::<Vec<AgentEvent>>())
                .catch_unwind()
                .await;
            assert!(
                result.is_err(),
                "expected panic to propagate through wrapper"
            );
            // The `SessionSpanGuard` was dropped during unwinding,
            // which recorded an `exception.message` on the span and
            // ended it. Verifying the recorded field requires a
            // subscriber; the panic propagation itself confirms the
            // unwind path was taken.
        }

        /// Unit test for the `SessionSpanGuard`'s panic-path behavior
        /// in isolation. Verifies that constructing a guard and then
        /// panicking causes `Drop` to observe `panicking() == true`.
        ///
        /// This is a white-box test of the guard's `Drop` impl, which
        /// is the core of the spec's "span status = Error on panic"
        /// requirement. The actual span-status mutation is delegated
        /// to `tracing::Span::record`, which is a no-op without a
        /// subscriber — but the `panicking()` check is verifiable.
        #[test]
        fn session_span_guard_drop_observes_panicking_during_unwind() {
            use std::panic;

            // We can't directly observe `Drop`'s internal
            // `panicking()` check from outside, but we can verify the
            // guard can be constructed and dropped without a subscriber
            // (no-op span), and that a panic + catch_unwind round-trip
            // doesn't abort the process.
            //
            // The `exception.message` field is declared at the callsite
            // (matching the production `wrap_output_with_otel` span) so
            // the guard's `Drop` impl can record it via `Span::record`
            // during panic unwinding. Without the declaration, `record`
            // is a silent no-op.
            let span = tracing::span!(
                target: "synthia.session",
                parent: None,
                tracing::Level::INFO,
                "session.start",
                exception.message = tracing::field::Empty,
            );
            // Constructing the guard must not panic; the span is
            // dropped at the end of this scope (no-op without a
            // subscriber, but exercises the `Drop` path).
            {
                let _guard = SessionSpanGuard::new(span);
            }

            // Now simulate a panic-drop: catch_unwind drops the guard
            // during unwinding.
            let result = panic::catch_unwind(|| {
                let _guard = SessionSpanGuard::new(tracing::span!(
                    target: "synthia.session",
                    parent: None,
                    tracing::Level::INFO,
                    "session.start",
                    exception.message = tracing::field::Empty,
                ));
                panic!("simulated panic with guard in scope");
            });
            assert!(result.is_err());
            // If the guard's `Drop` had panicked during unwinding
            // (double-panic), the test process would have aborted.
            // Reaching this assertion means `Drop` completed cleanly.
        }

        // ===== TurnSpanGuard tests (Task 7) =====

        /// Task 7.5: Verify `TurnSpanGuard` can be created and dropped
        /// without a subscriber (no-op span). The span is a `tracing`
        /// no-op without a subscriber, but the guard's `Drop` must
        /// still run cleanly.
        #[test]
        fn turn_span_guard_create_and_drop_normal() {
            let turn_id = crate::turn::TurnId::new();
            {
                let _guard = TurnSpanGuard::create(&turn_id, 1);
            } // Guard dropped here — span ended.
        }

        /// Task 7.5: Verify `record_error` can be called without a
        /// subscriber (no-op span). The recording is a no-op without a
        /// subscriber, but the call must not panic and the guard must
        /// drop cleanly afterward.
        #[test]
        fn turn_span_guard_record_error_then_drop() {
            let turn_id = crate::turn::TurnId::new();
            {
                let mut guard = TurnSpanGuard::create(&turn_id, 1);
                guard.record_error("TurnError", "test failure");
            } // Guard dropped here — span ended with error recorded.
        }

        /// Task 7.5: Verify the guard's `Drop` runs during panic
        /// unwinding and records a fallback `exception.message`. Uses
        /// `panic::catch_unwind` to catch the panic and verify it
        /// propagated without aborting.
        #[test]
        fn turn_span_guard_drop_during_panic() {
            use std::panic;
            let turn_id = crate::turn::TurnId::new();
            let result = panic::catch_unwind(|| {
                let _guard = TurnSpanGuard::create(&turn_id, 1);
                panic!("simulated turn panic");
            });
            assert!(result.is_err());
            // The guard's `Drop` ran during unwinding, recording
            // `exception.message = "turn panicked"` (no-op without a
            // subscriber, but the `panicking()` check was exercised).
        }

        /// Task 7.5: Verify N turn iterations produce N `turn.start`
        /// spans. Uses a `tracing_subscriber` counting layer to count
        /// `new_span` events for spans named `"turn.start"`.
        #[test]
        fn n_turn_iterations_produce_n_spans() {
            use std::sync::{
                Arc,
                atomic::{AtomicUsize, Ordering},
            };

            use tracing_subscriber::{
                layer::{Context, Layer},
                prelude::*,
            };

            struct SpanCounter {
                count: Arc<AtomicUsize>,
            }
            impl<S: tracing::Subscriber> Layer<S> for SpanCounter {
                fn on_new_span(
                    &self,
                    attrs: &tracing::span::Attributes<'_>,
                    _id: &tracing::span::Id,
                    _ctx: Context<'_, S>,
                ) {
                    if attrs.metadata().name() == "turn.start" {
                        self.count.fetch_add(1, Ordering::SeqCst);
                    }
                }
            }

            let counter = Arc::new(AtomicUsize::new(0));
            let _guard = tracing_subscriber::registry::Registry::default()
                .with(SpanCounter {
                    count: counter.clone(),
                })
                .set_default();

            let n = 3;
            for i in 0..n {
                let turn_id = crate::turn::TurnId::new();
                let _turn_guard = TurnSpanGuard::create(&turn_id, i + 1);
                // Guard dropped at end of iteration — span ended.
            }

            assert_eq!(counter.load(Ordering::SeqCst), n);
        }

        /// Task 7.5: Verify that `record_error` records the OTel
        /// exception fields (`exception.type`, `exception.message`,
        /// `otel.status_code`) on the span. Uses a capture layer that
        /// collects recorded field names per span.
        #[test]
        fn turn_span_records_exception_fields_on_error() {
            use std::{
                collections::HashMap,
                sync::{Arc, Mutex},
            };

            use tracing_subscriber::{
                layer::{Context, Layer},
                prelude::*,
            };

            #[derive(Default)]
            struct CapturedSpan {
                name: String,
                recorded_fields: Vec<String>,
            }

            struct CaptureLayer {
                spans: Arc<Mutex<HashMap<tracing::span::Id, CapturedSpan>>>,
            }
            impl<S: tracing::Subscriber> Layer<S> for CaptureLayer {
                fn on_new_span(
                    &self,
                    attrs: &tracing::span::Attributes<'_>,
                    id: &tracing::span::Id,
                    _ctx: Context<'_, S>,
                ) {
                    self.spans.lock().unwrap().insert(
                        id.clone(),
                        CapturedSpan {
                            name: attrs.metadata().name().to_string(),
                            recorded_fields: Vec::new(),
                        },
                    );
                }

                fn on_record(
                    &self,
                    span: &tracing::span::Id,
                    values: &tracing::span::Record<'_>,
                    _ctx: Context<'_, S>,
                ) {
                    struct FieldNameCollector {
                        names: Vec<String>,
                    }
                    impl tracing::field::Visit for FieldNameCollector {
                        fn record_debug(
                            &mut self,
                            field: &tracing::field::Field,
                            _value: &dyn std::fmt::Debug,
                        ) {
                            self.names.push(field.name().to_string());
                        }
                    }
                    let mut visitor = FieldNameCollector { names: Vec::new() };
                    values.record(&mut visitor);
                    let mut spans = self.spans.lock().unwrap();
                    if let Some(span_data) = spans.get_mut(span) {
                        span_data.recorded_fields.extend(visitor.names);
                    }
                }
            }

            let spans: Arc<Mutex<HashMap<tracing::span::Id, CapturedSpan>>> =
                Arc::new(Mutex::new(HashMap::new()));
            let _guard = tracing_subscriber::registry::Registry::default()
                .with(CaptureLayer {
                    spans: spans.clone(),
                })
                .set_default();

            let turn_id = crate::turn::TurnId::new();
            {
                let mut guard = TurnSpanGuard::create(&turn_id, 1);
                guard.record_error("TurnError", "sample_cascade_continue");
            } // Guard dropped — span ended.

            let spans = spans.lock().unwrap();
            assert_eq!(spans.len(), 1, "expected exactly one turn.start span");
            let span = spans.values().next().unwrap();
            assert_eq!(span.name, "turn.start");
            assert!(
                span.recorded_fields.iter().any(|f| f == "exception.type"),
                "expected exception.type to be recorded; got {:?}",
                span.recorded_fields
            );
            assert!(
                span.recorded_fields
                    .iter()
                    .any(|f| f == "exception.message"),
                "expected exception.message to be recorded; got {:?}",
                span.recorded_fields
            );
            assert!(
                span.recorded_fields.iter().any(|f| f == "otel.status_code"),
                "expected otel.status_code to be recorded; got {:?}",
                span.recorded_fields
            );
        }
    }
}

pub struct AgentInitConfig {
    pub config: AgentConfig,
    pub provider_registry: ProviderRegistry,
    pub provider: Arc<dyn ModelProvider>,
    pub tool_registry: ToolRegistry,
    pub hook_registry: Arc<HookRegistry>,
    pub command_registry: CommandRegistry,
    pub session_manager: SessionManager,
    pub context_assembler: ContextAssembler,
    pub model_router: ModelRouter,
    pub session_store: SessionStore,
    pub memory_event_sender: Option<mpsc::Sender<MemoryEvent>>,
}

pub struct Agent {
    pub config: AgentConfig,
    pub provider_registry: ProviderRegistry,
    pub provider: Arc<dyn ModelProvider>,
    pub tool_registry: ToolRegistry,
    pub hook_registry: Arc<HookRegistry>,
    pub command_registry: CommandRegistry,
    pub session_manager: SessionManager,
    pub context_assembler: Arc<ContextAssembler>,
    pub model_router: Arc<ModelRouter>,
    pub session_store: SessionStore,
    pub mcp_manager: Option<synthia_mcp::McpManager>,
    pub steering_channel: Option<Arc<dyn SteeringChannel>>,
    pub config_watcher: Option<MultiConfigWatcher>,
    pub memory_event_sender: Option<mpsc::Sender<MemoryEvent>>,
    pub approval_service: Option<Arc<dyn ApprovalService>>,
    pub sandbox_manager: Option<Arc<dyn SandboxManager>>,
}

/// H1 fix: auto-assemble a tool orchestrator into `run_config` when the
/// caller did not inject one AND no `tool_registry` tools are available.
///
/// `Agent::run_stream` is a static method that cannot reach
/// `Agent::assemble_default_orchestrator` (an instance method requiring
/// `&self`). Without this free function, CLI/Examples that call
/// `run_stream` directly — bypassing `Agent::resume` — silently degrade
/// to a `None` orchestrator, leaving tool execution unreachable.
///
/// Condition: skip auto-assembly when `tool_registry` is non-empty. The
/// `StepToolExecute::execute` step has two paths — `execute_via_orchestrator`
/// (when `tool_orchestrator` is `Some`) and `execute_via_registry` (when
/// `None`, using `tool_registry` + `PermissionChecker`). If the caller
/// registered tools, the registry path is functional and auto-assembling
/// an orchestrator would override the caller's intent (bypassing their
/// `PermissionChecker`). Auto-assembly only fires when the caller
/// provided NEITHER path — truly broken configuration.
///
/// This helper uses `run_config`'s own `approval_service` /
/// `sandbox_manager` / `config.workspace_root` fields, falling back to
/// `HeadlessApprovalService` / `NoopSandboxManager` when those are also
/// `None`. The fallback is fail-closed for permission (deny-by-default)
/// and explicit no-op for sandbox (caller accepted responsibility).
fn ensure_tool_orchestrator(run_config: &mut AgentRunConfig) {
    if run_config.tool_orchestrator.is_some() {
        return;
    }
    if !run_config.tool_registry.is_empty() {
        // Caller registered tools → `execute_via_registry` path is
        // functional. Don't override their `PermissionChecker` wiring.
        return;
    }
    tracing::warn!(
        "auto-assembled tool orchestrator (caller did not inject one)"
    );
    let approval_service =
        run_config.approval_service.clone().unwrap_or_else(|| {
            Arc::new(synthia_permission::HeadlessApprovalService)
        });
    let sandbox_manager = run_config
        .sandbox_manager
        .clone()
        .unwrap_or_else(|| Arc::new(synthia_sandbox::NoopSandboxManager));
    let (orchestrator, _resolver) =
        crate::tools::orchestrator::build_default_tool_orchestrator(
            run_config.config.workspace_root.clone(),
            approval_service,
            sandbox_manager,
        );
    run_config.tool_orchestrator = Some(orchestrator);
}

impl Agent {
    pub fn with_mcp_manager(
        mut self,
        mcp_manager: synthia_mcp::McpManager,
    ) -> Self {
        self.mcp_manager = Some(mcp_manager);
        self
    }

    pub fn with_steering_channel(
        mut self,
        channel: Arc<dyn SteeringChannel>,
    ) -> Self {
        self.steering_channel = Some(channel);
        self
    }

    pub fn with_config_watcher(mut self, watcher: MultiConfigWatcher) -> Self {
        self.config_watcher = Some(watcher);
        self
    }

    pub fn with_approval_service(
        mut self,
        service: Arc<dyn ApprovalService>,
    ) -> Self {
        self.approval_service = Some(service);
        self
    }

    pub fn with_sandbox_manager(
        mut self,
        manager: Arc<dyn SandboxManager>,
    ) -> Self {
        self.sandbox_manager = Some(manager);
        self
    }

    pub fn run_stream(mut run_config: AgentRunConfig) -> AgentOutput {
        // H1 fix: auto-assemble tool orchestrator when not injected.
        // Prevents silent degradation when CLI/Examples call run_stream
        // directly without going through Agent::resume (which does assembly).
        ensure_tool_orchestrator(&mut run_config);
        #[cfg(feature = "otel")]
        {
            // Snapshot the OTel context before `run_config` is moved
            // into `StreamBuilder::run`. `StreamBuilder::from_config`
            // clones what it needs (returns an owned builder), so the
            // borrow ends before `.run(run_config)` moves the config.
            let otel_ctx =
                otel_context::OtelContext::from_run_config(&run_config);
            let stream =
                StreamBuilder::from_config(&run_config).run(run_config);
            otel_context::wrap_output_with_otel(stream, otel_ctx)
        }
        #[cfg(not(feature = "otel"))]
        {
            StreamBuilder::from_config(&run_config).run(run_config)
        }
    }

    fn assemble_default_orchestrator(&self, run_config: &mut AgentRunConfig) {
        if run_config.tool_orchestrator.is_some() {
            return;
        }
        let approval_service =
            self.approval_service.clone().unwrap_or_else(|| {
                Arc::new(synthia_permission::HeadlessApprovalService)
            });
        let sandbox_manager = self
            .sandbox_manager
            .clone()
            .unwrap_or_else(|| Arc::new(synthia_sandbox::NoopSandboxManager));
        let (orchestrator, _resolver) =
            crate::tools::orchestrator::build_default_tool_orchestrator(
                &self.config.workspace_root,
                approval_service,
                sandbox_manager,
            );
        run_config.tool_orchestrator = Some(orchestrator);
    }

    pub fn run_stream_with_state(
        state_config: AgentRunStateConfig,
    ) -> AgentOutput {
        let AgentRunStateConfig {
            run_config,
            initial_messages,
            start_iteration,
        } = state_config;
        // Snapshot the OTel context before `run_config` is moved into
        // `builder.run(...)`. This also covers `Agent::resume`, which
        // delegates here — so `resume`'s stream is wrapped transitively
        // (Task 5.3), avoiding a double-wrap if `resume` wrapped
        // separately.
        #[cfg(feature = "otel")]
        let otel_ctx = otel_context::OtelContext::from_run_config(&run_config);
        let mut builder = StreamBuilder::from_config(&run_config);
        builder.with_initial_state(initial_messages, start_iteration);
        let stream = builder.run(run_config);
        #[cfg(feature = "otel")]
        {
            otel_context::wrap_output_with_otel(stream, otel_ctx)
        }
        #[cfg(not(feature = "otel"))]
        {
            stream
        }
    }

    pub async fn shutdown(&mut self) {
        let sessions = self.session_manager.list().await;
        for id in &sessions {
            if let Some(session) = self.session_manager.get(id).await
                && let Err(e) = self.session_manager.save_metadata(&session)
            {
                tracing::warn!(
                    session_id = %id,
                    error = %e,
                    "Failed to save session during shutdown"
                );
            }
        }
        tracing::info!("Session state saved");

        if let Some(ref mcp) = self.mcp_manager {
            let _ = mcp.stop_all().await;
            tracing::info!("MCP connections closed");
        }

        if let Some(ref mut watcher) = self.config_watcher {
            watcher.shutdown().await;
            tracing::info!("Config watcher stopped");
        }

        if let Err(e) = self
            .hook_registry
            .fire_complete(&synthia_hook::AgentContext::new(
                "shutdown".to_string(),
                "shutdown-turn".to_string(),
            ))
            .await
        {
            tracing::warn!(error = %e, "Hook error during shutdown");
        }

        tracing::info!("Agent shut down complete");
    }

    pub fn resume(
        &self,
        user_id: String,
        session_id: String,
        cancel_token: CancellationToken,
    ) -> AgentOutput {
        let config = self.config.clone();

        let checkpoint_dir =
            config.checkpoint_dir.clone().unwrap_or_else(|| {
                config.workspace_root.join(".agents").join("checkpoints")
            });

        let (messages, start_iteration) =
            match checkpoint::Checkpoint::load_latest_by_session(
                &checkpoint_dir,
                &session_id,
            ) {
                Ok(Some(cp)) if !cp.messages.is_empty() => {
                    let mut msgs = cp.messages;
                    checkpoint::patch_tool_calls_recovery(&mut msgs);
                    tracing::info!(
                        session_id = %session_id,
                        restored_iteration = %cp.iteration,
                        message_count = %msgs.len(),
                        "Resumed from checkpoint"
                    );
                    (msgs, cp.iteration)
                }
                _ => match self
                    .session_store
                    .load_messages_all::<Message>(&user_id, &session_id)
                {
                    Ok(msgs) if !msgs.is_empty() => {
                        tracing::info!(
                            session_id = %session_id,
                            message_count = %msgs.len(),
                            "Resumed from session JSONL (no checkpoint)"
                        );
                        (msgs, 0)
                    }
                    _ => {
                        return Box::pin(futures::stream::once(async move {
                            AgentEvent::Warning {
                                message: format!(
                                    "No checkpoint or session data found for session '{}', cannot resume",
                                    session_id
                                ),
                            }
                        }));
                    }
                },
            };

        let mut run_config = AgentRunConfig {
            provider: Arc::clone(&self.provider),
            tool_registry: self.tool_registry.clone(),
            hook_registry: Arc::clone(&self.hook_registry),
            model_router: Arc::clone(&self.model_router),
            user_id,
            session_id,
            input: AgentInput::text(""),
            config,
            context_assembler: Some(Arc::clone(&self.context_assembler)),
            session_store: self.session_store.clone(),
            steering_channel: self.steering_channel.clone(),
            session_input_queue: Some(
                self.session_manager.input_queue().clone(),
            ),
            cancel_token,
            memory_event_sender: self.memory_event_sender.clone(),
            agent_control: Some(AgentControl::new(Arc::new(
                AgentRegistry::new(),
            ))),
            fork_policy: Default::default(),
            // Resume path does not carry a runtime compaction
            // provider; callers that need L4 auto-compaction must
            // build via `AgentRunConfigBuilder::compaction_provider`.
            compaction_provider: None,
            subagent_session_factory: None,
            approval_service: None,
            sandbox_manager: None,
            tool_orchestrator: None,
            guardian_coordinator: None,
        };

        self.assemble_default_orchestrator(&mut run_config);

        let state_config = AgentRunStateConfig {
            run_config,
            initial_messages: messages,
            start_iteration,
        };

        Self::run_stream_with_state(state_config)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use async_trait::async_trait;
    use synthia_hook::HookRegistry;
    use synthia_permission::{HeadlessApprovalService, Permission};
    use synthia_provider::{
        router::ModelRouter,
        traits::ModelProvider,
        types::*,
    };
    use synthia_sandbox::NoopSandboxManager;
    use synthia_session::{Store as SessionStore, manager::SessionManager};
    use synthia_tool::registry::ToolRegistry;
    use synthia_tool_orchestrator::{
        ExecutionContext,
        ToolCallRequest,
        ToolOrchestratorError,
    };
    use tempfile::TempDir;
    use tokio_util::sync::CancellationToken;

    use super::*;
    use crate::{
        component_assembly::ComponentAssembler,
        config::AgentRunConfigBuilder,
    };

    struct NullProvider;

    #[async_trait]
    impl ModelProvider for NullProvider {
        async fn initialize(
            &mut self,
            _config: ProviderConfig,
        ) -> Result<(), synthia_core::Error> {
            Ok(())
        }

        fn name(&self) -> &str {
            "null"
        }

        fn model_config(&self) -> ModelConfig {
            ModelConfig {
                name: "null".to_string(),
                provider: "null".to_string(),
                context_window: 128_000,
                max_output_tokens: 4096,
                supports_tools: true,
                supports_streaming: false,
                supports_reasoning: false,
            }
        }

        async fn complete(
            &self,
            _request: CompletionRequest,
        ) -> Result<CompletionResponse, synthia_core::Error> {
            Ok(CompletionResponse {
                id: "null".to_string(),
                model: "null".to_string(),
                content: Content::text(""),
                usage: Default::default(),
                cached: false,
            })
        }

        async fn embed(
            &self,
            texts: Vec<String>,
        ) -> Result<Vec<Vec<f64>>, synthia_core::Error> {
            Ok(vec![vec![0.0; 1536]; texts.len()])
        }
    }

    fn test_agent(
        workspace_root: std::path::PathBuf,
    ) -> (Agent, Arc<dyn ModelProvider>) {
        let provider: Arc<dyn ModelProvider> = Arc::new(NullProvider);
        let agent = ComponentAssembler::new(AgentConfig {
            workspace_root: workspace_root.clone(),
            ..Default::default()
        })
        .with_provider(Arc::clone(&provider))
        .with_session_manager(SessionManager::new(
            workspace_root.join(".synthia").join("sessions"),
        ))
        .build();
        (agent, provider)
    }

    fn base_run_config(
        provider: Arc<dyn ModelProvider>,
        workspace_root: std::path::PathBuf,
        session_dir: std::path::PathBuf,
    ) -> AgentRunConfig {
        AgentRunConfigBuilder::new()
            .provider(provider)
            .tool_registry(ToolRegistry::new())
            .hook_registry(Arc::new(HookRegistry::new()))
            .model_router(Arc::new(ModelRouter::default()))
            .user_id("test-user".to_string())
            .session_id("test-session".to_string())
            .input(AgentInput::text(""))
            .config(AgentConfig {
                workspace_root,
                ..Default::default()
            })
            .session_store(SessionStore::new(session_dir))
            .cancel_token(CancellationToken::new())
            .build()
            .unwrap()
    }

    #[test]
    fn assemble_default_orchestrator_creates_orchestrator_when_none() {
        let tmp = TempDir::new().unwrap();
        let workspace_root = tmp.path().to_path_buf();
        let (agent, provider) = test_agent(workspace_root.clone());
        let session_dir = tmp.path().join("sessions");
        let mut run_config =
            base_run_config(provider, workspace_root, session_dir);

        assert!(run_config.tool_orchestrator.is_none());
        agent.assemble_default_orchestrator(&mut run_config);
        assert!(run_config.tool_orchestrator.is_some());
    }

    #[test]
    fn assemble_default_orchestrator_preserves_explicit_orchestrator() {
        let tmp = TempDir::new().unwrap();
        let workspace_root = tmp.path().to_path_buf();
        let (agent, provider) = test_agent(workspace_root.clone());
        let session_dir = tmp.path().join("sessions");
        let mut run_config =
            base_run_config(provider, workspace_root.clone(), session_dir);

        let (explicit, _) =
            crate::tools::orchestrator::build_default_tool_orchestrator(
                workspace_root,
                Arc::new(HeadlessApprovalService),
                Arc::new(NoopSandboxManager),
            );
        let explicit: Arc<dyn synthia_tool_orchestrator::ToolOrchestrator> =
            explicit;
        run_config.tool_orchestrator = Some(Arc::clone(&explicit));

        agent.assemble_default_orchestrator(&mut run_config);

        assert!(run_config.tool_orchestrator.is_some());
        assert!(Arc::ptr_eq(
            run_config.tool_orchestrator.as_ref().unwrap(),
            &explicit
        ));
    }

    #[tokio::test]
    async fn headless_approval_service_denies_bash() {
        let tmp = TempDir::new().unwrap();
        let workspace_root = tmp.path().to_path_buf();
        let (orchestrator, _) =
            crate::tools::orchestrator::build_default_tool_orchestrator(
                workspace_root,
                Arc::new(HeadlessApprovalService),
                Arc::new(NoopSandboxManager),
            );

        let request = ToolCallRequest {
            call_id: "call-1".to_string(),
            tool_name: "bash".to_string(),
            arguments: serde_json::json!({ "command": "echo hello" }),
            permission: Permission::RequireConfirm,
        };
        let context = ExecutionContext {
            session_id: "s".to_string(),
            workspace_root: tmp.path().to_path_buf(),
            caller_agent: "test".to_string(),
        };

        let result = orchestrator
            .execute(request, context, CancellationToken::new())
            .await;

        match result {
            Err(ToolOrchestratorError::Denied { call_id }) => {
                assert_eq!(call_id, "call-1");
            }
            other => panic!("expected bash call to be denied, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn headless_approval_service_denies_dangerous_tools() {
        let tmp = TempDir::new().unwrap();
        let workspace_root = tmp.path().to_path_buf();
        let (orchestrator, _) =
            crate::tools::orchestrator::build_default_tool_orchestrator(
                workspace_root,
                Arc::new(HeadlessApprovalService),
                Arc::new(NoopSandboxManager),
            );

        for tool_name in ["bash", "write", "apply_patch", "multi_edit"] {
            let request = ToolCallRequest {
                call_id: format!("call-{tool_name}"),
                tool_name: tool_name.to_string(),
                arguments: serde_json::json!({}),
                permission: Permission::RequireConfirm,
            };
            let context = ExecutionContext {
                session_id: "s".to_string(),
                workspace_root: tmp.path().to_path_buf(),
                caller_agent: "test".to_string(),
            };

            let result = orchestrator
                .execute(request, context, CancellationToken::new())
                .await;

            match result {
                Err(ToolOrchestratorError::Denied { call_id }) => {
                    assert_eq!(call_id, format!("call-{tool_name}"));
                }
                other => panic!(
                    "expected {tool_name} call to be denied, got {:?}",
                    other
                ),
            }
        }
    }

    // ---- H1 regression tests ----
    //
    // Verifies that `ensure_tool_orchestrator` (the free function
    // called by the static `Agent::run_stream`) auto-assembles a
    // tool orchestrator when the caller did not inject one. Before
    // the H1 fix, CLI/Examples that called `run_stream` directly
    // bypassing `Agent::resume` silently degraded to a `None`
    // orchestrator, leaving tool execution unreachable.

    #[test]
    fn ensure_tool_orchestrator_assembles_when_none() {
        let tmp = TempDir::new().unwrap();
        let workspace_root = tmp.path().to_path_buf();
        let (_, provider) = test_agent(workspace_root.clone());
        let session_dir = tmp.path().join("sessions");
        let mut run_config =
            base_run_config(provider, workspace_root, session_dir);

        assert!(run_config.tool_orchestrator.is_none());
        ensure_tool_orchestrator(&mut run_config);
        assert!(run_config.tool_orchestrator.is_some());
    }

    #[test]
    fn ensure_tool_orchestrator_preserves_explicit_orchestrator() {
        let tmp = TempDir::new().unwrap();
        let workspace_root = tmp.path().to_path_buf();
        let (_, provider) = test_agent(workspace_root.clone());
        let session_dir = tmp.path().join("sessions");
        let mut run_config =
            base_run_config(provider, workspace_root.clone(), session_dir);

        let (explicit, _) =
            crate::tools::orchestrator::build_default_tool_orchestrator(
                workspace_root,
                Arc::new(HeadlessApprovalService),
                Arc::new(NoopSandboxManager),
            );
        let explicit: Arc<dyn synthia_tool_orchestrator::ToolOrchestrator> =
            explicit;
        run_config.tool_orchestrator = Some(Arc::clone(&explicit));

        ensure_tool_orchestrator(&mut run_config);

        assert!(Arc::ptr_eq(
            run_config.tool_orchestrator.as_ref().unwrap(),
            &explicit
        ));
    }

    #[tokio::test]
    async fn ensure_tool_orchestrator_assembled_orchestrator_denies_bash() {
        // Verifies the auto-assembled orchestrator is functional: the
        // `HeadlessApprovalService` fallback denies bash execution,
        // proving `approval_service` was correctly wired into the
        // orchestrator (not just a stub). This matches the
        // fail-closed permission posture required by P6.
        let tmp = TempDir::new().unwrap();
        let workspace_root = tmp.path().to_path_buf();
        let (_, provider) = test_agent(workspace_root.clone());
        let session_dir = tmp.path().join("sessions");
        let mut run_config =
            base_run_config(provider, workspace_root.clone(), session_dir);

        ensure_tool_orchestrator(&mut run_config);
        let orchestrator = run_config.tool_orchestrator.unwrap();

        let request = ToolCallRequest {
            call_id: "call-h1".to_string(),
            tool_name: "bash".to_string(),
            arguments: serde_json::json!({ "command": "echo hi" }),
            permission: Permission::RequireConfirm,
        };
        let context = ExecutionContext {
            session_id: "s".to_string(),
            workspace_root,
            caller_agent: "test".to_string(),
        };

        let result = orchestrator
            .execute(request, context, CancellationToken::new())
            .await;

        match result {
            Err(ToolOrchestratorError::Denied { call_id }) => {
                assert_eq!(call_id, "call-h1");
            }
            other => panic!("expected bash call to be denied, got {:?}", other),
        }
    }
}
