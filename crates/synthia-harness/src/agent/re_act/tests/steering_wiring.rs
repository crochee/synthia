//! Steering wiring: guards, hooks, sanitizers, hints, output
//! transformers, and the R29-Phase-K/L context-manager + chunk
//! persistence contracts.

use std::sync::Arc;

use async_trait::async_trait;
use futures::StreamExt;
use serde_json::{Value, json};
use synthia_context::{AgentState, CompactionDetails, ContextManager};
use synthia_provider::{
    ContentPart,
    Message,
    SamplingResult,
    StreamChunk,
    TextContent,
    TokenUsage,
    traits::ModelProvider,
};
use synthia_steering::{
    GuardResult,
    GuardSeverity,
    HintMessage,
    HintPriority,
    InjectionPoint,
    Steering,
};
use synthia_test_support::FakeTool;
use synthia_tool::ToolRegistry;
use tokio_util::sync::CancellationToken;

use super::{support::*, *};

async fn run_and_collect_with_steering(
    provider: Arc<dyn ModelProvider>,
    registry: Arc<ToolRegistry>,
    steering: Arc<Steering>,
    input: AgentInput,
) -> Vec<AgentEvent> {
    let agent = ReActAgent::new(provider, registry).with_steering(steering);
    let mut stream = agent.run(input, Arc::new(CancellationToken::new())).await;
    let mut out = Vec::new();
    while let Some(ev) = stream.next().await {
        out.push(ev);
    }
    out
}

struct DenyEchoGuard;
impl synthia_steering::Guard for DenyEchoGuard {
    fn name(&self) -> &str {
        "deny_echo"
    }

    fn check(
        &self,
        action: &synthia_steering::Action,
        _state: &AgentState,
    ) -> GuardResult {
        match action {
            synthia_steering::Action::ToolCall { name, .. }
                if name == "echo" =>
            {
                GuardResult::Deny {
                    reason: "echo is disabled for this run".to_string(),
                    severity: GuardSeverity::High,
                }
            }
            _ => GuardResult::Allow,
        }
    }
}

/// A guard denial MUST surface as the tool result (error,
/// model-facing coaching text) plus a `WarningKind::Guard`
/// system event — the run itself MUST complete normally.
#[tokio::test]
async fn guard_denial_surfaces_as_error_result_and_warning() {
    let tool_use = ToolUse {
        id: "c1".to_string(),
        name: "echo".to_string(),
        input: json!({}),
    };
    let provider = Arc::new(ScriptedStreamProvider::new(vec![
        tool_call_response(vec![tool_use.clone()]),
        empty_response(),
    ]));
    let registry = Arc::new(ToolRegistry::new());
    registry.register_entry(synthia_tool::ToolEntry::new(Arc::new(
        FakeTool::new("echo", "echoed"),
    )));
    let steering =
        Arc::new(Steering::noop().add_guard(Arc::new(DenyEchoGuard)));

    let events = run_and_collect_with_steering(
        provider,
        registry,
        steering,
        AgentInput::text("run echo"),
    )
    .await;

    let tool_results: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            AgentEvent::Model(ContentPart::ToolResult(tr)) => Some(tr.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(tool_results.len(), 1);
    assert!(tool_results[0].is_error.unwrap_or(false));
    let text = match &tool_results[0].content[0] {
        ContentPart::Text(t) => t.text.clone(),
        _ => panic!("expected text content"),
    };
    assert!(
        text.contains("[blocked by guard `deny_echo` (high)]")
            && text.contains("echo is disabled"),
        "denial text should carry guard name + reason, got: {text}"
    );

    let guard_warnings: Vec<_> = events
        .iter()
        .filter(|e| {
            matches!(
                e,
                AgentEvent::System(SystemEvent::Warning {
                    kind: WarningKind::Guard,
                    ..
                })
            )
        })
        .collect();
    assert_eq!(
        guard_warnings.len(),
        1,
        "expected exactly one Guard warning event"
    );

    // The session still ends Completed — denials are
    // errors-as-results, never run aborts.
    assert!(matches!(
        events.last(),
        Some(AgentEvent::System(SystemEvent::SessionEnded {
            reason: SessionEndReason::Completed,
        }))
    ));
}

/// A hook block MUST surface as the tool result and the
/// session MUST complete.
///
/// Also pins the **ordering** of the veto epilogue: the tracker sees the
/// denied call *before* the `on_error` fan-out. The ordering is easy to
/// invert while consolidating the three veto seams' identical tails —
/// which is exactly what happened once, silently, because no in-tree
/// tracker observes it. The observer below is the consumer that notices.
#[tokio::test]
async fn hook_block_surfaces_as_error_result() {
    /// Counts `on_tool_call`; the block hook's `on_error` reads it.
    #[derive(Default)]
    struct CountingTracker {
        calls: Arc<std::sync::atomic::AtomicUsize>,
    }

    impl synthia_steering::tracker::Tracker for CountingTracker {
        fn on_iteration(&self, _state: &AgentState) {}

        fn on_tool_call(
            &self,
            _name: &str,
            _arguments: &Value,
            _state: &AgentState,
        ) {
            self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }

        fn on_llm_response(&self, _usage: &TokenUsage, _state: &AgentState) {}

        fn recommended_concurrency(&self, _state: &AgentState) -> usize {
            1
        }
    }

    /// The block hook records whether the tracker had already seen the
    /// denied call at the moment `on_error` fired.
    struct ObservingBlockHook {
        tracker_calls: Arc<std::sync::atomic::AtomicUsize>,
        recorded: Arc<std::sync::Mutex<Option<usize>>>,
    }

    #[async_trait]
    impl synthia_steering::AgentHook for ObservingBlockHook {
        fn name(&self) -> &str {
            "observe_block"
        }

        async fn before_tool_execute(
            &self,
            name: &str,
            _arguments: &Value,
        ) -> synthia_steering::HookAction {
            if name == "echo" {
                synthia_steering::HookAction::Block(
                    "echo requires user approval".to_string(),
                )
            } else {
                synthia_steering::HookAction::Continue
            }
        }

        async fn on_error(&self, _stage: &str, _message: &str) {
            let seen =
                self.tracker_calls.load(std::sync::atomic::Ordering::SeqCst);
            *self.recorded.lock().expect("lock") = Some(seen);
        }
    }

    let tracker_calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let recorded = Arc::new(std::sync::Mutex::new(None));

    let tool_use = ToolUse {
        id: "c1".to_string(),
        name: "echo".to_string(),
        input: json!({}),
    };
    let provider = Arc::new(ScriptedStreamProvider::new(vec![
        tool_call_response(vec![tool_use.clone()]),
        empty_response(),
    ]));
    let registry = Arc::new(ToolRegistry::new());
    registry.register_entry(synthia_tool::ToolEntry::new(Arc::new(
        FakeTool::new("echo", "echoed"),
    )));
    let steering = Arc::new(
        Steering::noop()
            .add_hook(Arc::new(ObservingBlockHook {
                tracker_calls: Arc::clone(&tracker_calls),
                recorded: Arc::clone(&recorded),
            }))
            .with_tracker(Arc::new(CountingTracker {
                calls: Arc::clone(&tracker_calls),
            })),
    );

    let events = run_and_collect_with_steering(
        provider,
        registry,
        steering,
        AgentInput::text("run echo"),
    )
    .await;

    let text = events
        .iter()
        .filter_map(|e| match e {
            AgentEvent::Model(ContentPart::ToolResult(tr)) => {
                Some(tr.content.clone())
            }
            _ => None,
        })
        .next()
        .expect("tool result event");
    assert!(text.iter().any(|p| {
        matches!(p, ContentPart::Text(t)
                if t.text.contains("[blocked by hook `observe_block`]")
                    && t.text.contains("user approval"))
    }));

    let seen = recorded
        .lock()
        .expect("lock")
        .expect("the block hook must have been notified through on_error");
    assert!(
        seen >= 1,
        "the tracker must record the denied call before `on_error` \
         fans out (saw {seen} recorded calls)"
    );
}

/// A sanitized action MUST be dispatched in place of the
/// original call (tool name rewritten to a registered peer).
#[tokio::test]
async fn sanitized_action_is_adopted_for_dispatch() {
    struct RewriteToAlt;
    impl synthia_steering::Guard for RewriteToAlt {
        fn name(&self) -> &str {
            "rewrite_to_alt"
        }

        fn check(
            &self,
            action: &synthia_steering::Action,
            _state: &AgentState,
        ) -> GuardResult {
            match action {
                synthia_steering::Action::ToolCall { name, arguments }
                    if name == "echo" =>
                {
                    GuardResult::Sanitize {
                        action: synthia_steering::Action::ToolCall {
                            name: "alt".to_string(),
                            arguments: arguments.clone(),
                        },
                        warning: "routed to alt".to_string(),
                    }
                }
                _ => GuardResult::Allow,
            }
        }
    }

    let tool_use = ToolUse {
        id: "c1".to_string(),
        name: "echo".to_string(),
        input: json!({"x": 1}),
    };
    let provider = Arc::new(ScriptedStreamProvider::new(vec![
        tool_call_response(vec![tool_use.clone()]),
        empty_response(),
    ]));
    let registry = Arc::new(ToolRegistry::new());
    registry.register_entry(synthia_tool::ToolEntry::new(Arc::new(
        FakeTool::new("echo", "echoed"),
    )));
    registry.register_entry(synthia_tool::ToolEntry::new(Arc::new(
        FakeTool::new("alt", "alt-ran"),
    )));
    let steering = Arc::new(Steering::noop().add_guard(Arc::new(RewriteToAlt)));

    let events = run_and_collect_with_steering(
        provider,
        registry,
        steering,
        AgentInput::text("run echo"),
    )
    .await;

    let result_text = events
        .iter()
        .filter_map(|e| match e {
            AgentEvent::Model(ContentPart::ToolResult(tr)) => {
                Some(tr.content.clone())
            }
            _ => None,
        })
        .next()
        .expect("tool result event");
    assert!(
        result_text.iter().any(|p| {
            matches!(p, ContentPart::Text(t) if t.text == "alt-ran")
        })
    );
}

/// A triggered `BeforeNextLlmCall` hint MUST append a
/// `[reminder]` user message that the NEXT provider request
/// sees at the tail of its history.
#[tokio::test]
async fn hint_injection_reaches_next_llm_request() {
    struct AlwaysHint;
    impl synthia_steering::Hint for AlwaysHint {
        fn name(&self) -> &str {
            "always"
        }

        fn should_trigger(&self, _state: &AgentState) -> bool {
            true
        }

        fn generate(&self, _state: &AgentState) -> HintMessage {
            HintMessage {
                content: "wrap up soon".to_string(),
                priority: HintPriority::Normal,
            }
        }

        fn injection_point(&self) -> InjectionPoint {
            InjectionPoint::BeforeNextLlmCall
        }
    }

    let tool_use = ToolUse {
        id: "c1".to_string(),
        name: "echo".to_string(),
        input: json!({}),
    };
    let provider = Arc::new(CapturingProvider::new(vec![
        tool_call_response(vec![tool_use.clone()]),
        empty_response(),
    ]));
    let registry = Arc::new(ToolRegistry::new());
    registry.register_entry(synthia_tool::ToolEntry::new(Arc::new(
        FakeTool::new("echo", "echoed"),
    )));
    let steering = Arc::new(Steering::noop().add_hint(Arc::new(AlwaysHint)));

    let agent =
        ReActAgent::new(provider.clone(), registry).with_steering(steering);
    let mut stream = agent
        .run(
            AgentInput::text("run echo"),
            Arc::new(CancellationToken::new()),
        )
        .await;
    while let Some(ev) = stream.next().await {
        let _ = ev;
    }

    let requests = provider.captured.lock().await;
    assert_eq!(requests.len(), 2, "two LLM passes expected");
    let second = &requests[1];
    let tail = second.last().expect("non-empty second request");
    assert!(
        matches!(&tail.content, Content::Single(ContentPart::Text(t))
                if t.text.starts_with("[reminder]")
                    && t.text.contains("wrap up soon")),
        "second request should carry the reminder at the tail, got {:?}",
        tail.content
    );
}

/// The output transformer MUST rewrite the textual projection
/// of successful tool results committed to history.
#[tokio::test]
async fn output_transformer_rewrites_committed_result() {
    struct Uppercase;
    #[async_trait]
    impl synthia_steering::OutputTransformer for Uppercase {
        async fn transform(
            &self,
            output: String,
            _tool_name: &str,
            _state: &AgentState,
        ) -> String {
            output.to_uppercase()
        }
    }

    let tool_use = ToolUse {
        id: "c1".to_string(),
        name: "echo".to_string(),
        input: json!({}),
    };
    let provider = Arc::new(ScriptedStreamProvider::new(vec![
        tool_call_response(vec![tool_use.clone()]),
        empty_response(),
    ]));
    let registry = Arc::new(ToolRegistry::new());
    registry.register_entry(synthia_tool::ToolEntry::new(Arc::new(
        FakeTool::new("echo", "quiet"),
    )));
    let steering =
        Arc::new(Steering::noop().with_output_transformer(Arc::new(Uppercase)));

    let events = run_and_collect_with_steering(
        provider,
        registry,
        steering,
        AgentInput::text("run echo"),
    )
    .await;

    let result_text = events
        .iter()
        .filter_map(|e| match e {
            AgentEvent::Model(ContentPart::ToolResult(tr)) => {
                Some(tr.content.clone())
            }
            _ => None,
        })
        .next()
        .expect("tool result event");
    assert!(
        result_text.iter().any(|p| {
            matches!(p, ContentPart::Text(t) if t.text == "QUIET")
        })
    );
}

/// R29-Phase-K: an agent run that reads `a.rs` and writes `b.rs`
/// hands both paths to the context manager, which forwards them
/// to the summariser. Pins the whole chain: tool dispatch →
/// accumulator → `set_compaction_details` → next `prepare`.
///
/// This is a *unit* test of the accumulator + the flush; the
/// summariser-facing half is pinned in `synthia-context`.
#[tokio::test]
async fn tool_file_activity_reaches_the_context_manager() {
    use std::sync::Mutex as StdMutex;

    /// Manager that records every `set_compaction_details` call
    /// and then prunes nothing.
    struct Spy {
        seen: Arc<StdMutex<Vec<CompactionDetails>>>,
    }

    #[async_trait]
    impl ContextManager for Spy {
        async fn prepare(
            &self,
            _messages: &mut Vec<Message>,
            _state: &mut AgentState,
        ) {
        }

        fn set_compaction_details(&self, details: CompactionDetails) {
            self.seen.lock().unwrap().push(details);
        }
    }

    // One tool-only turn (read a.rs + write b.rs), then an empty
    // response so the loop ends.
    let tool_uses = vec![
        ToolUse {
            id: "t1".to_string(),
            name: "read".to_string(),
            input: json!({ "path": "a.rs" }),
        },
        ToolUse {
            id: "t2".to_string(),
            name: "write".to_string(),
            input: json!({ "path": "b.rs", "content": "hi" }),
        },
    ];
    let provider: Arc<dyn ModelProvider> =
        Arc::new(ScriptedStreamProvider::new(vec![
            tool_call_response(tool_uses),
            empty_response(),
        ]));
    let registry = Arc::new(ToolRegistry::new());
    registry.register_entry(synthia_tool::ToolEntry::new(Arc::new(
        FakeTool::new("read", "read ok"),
    )));
    registry.register_entry(synthia_tool::ToolEntry::new(Arc::new(
        FakeTool::new("write", "wrote ok"),
    )));

    let seen: Arc<StdMutex<Vec<CompactionDetails>>> =
        Arc::new(StdMutex::new(Vec::new()));
    let spy = Arc::new(Spy {
        seen: Arc::clone(&seen),
    });

    let agent = ReActAgent::with_options(
        provider,
        registry,
        PathBuf::new(),
        "SYS".to_string(),
    )
    .with_context_manager(spy);

    let mut stream = agent
        .run(AgentInput::text("go"), Arc::new(CancellationToken::new()))
        .await;
    while stream.next().await.is_some() {}

    let captured = {
        let guard = seen.lock().unwrap();
        guard.clone()
    };
    let merged =
        captured
            .iter()
            .fold(CompactionDetails::default(), |mut acc, d| {
                acc.read_files.extend(d.read_files.iter().cloned());
                acc.modified_files.extend(d.modified_files.iter().cloned());
                acc
            });
    assert!(
        merged.read_files.contains(&PathBuf::from("a.rs")),
        "the `read` target must reach the manager; got {merged:?}"
    );
    assert!(
        merged.modified_files.contains(&PathBuf::from("b.rs")),
        "the `write` target must reach the manager; got {merged:?}"
    );
}

/// R29-Phase-K: only the deterministic builtins are tracked —
/// a `shell` call with a path-looking argument is NOT reported,
/// because inferring file activity from a shell command is a
/// guess.
#[test]
fn touched_target_ignores_shell() {
    assert!(touched_target("shell", &json!({ "cmd": "rm a.rs" })).is_none());
    assert!(touched_target("web_fetch", &json!({ "url": "x" })).is_none());
    assert_eq!(
        touched_target("read", &json!({ "path": "a.rs" })),
        Some((Some(PathBuf::from("a.rs")), None))
    );
    assert_eq!(
        touched_target("write", &json!({ "path": "b.rs" })),
        Some((None, Some(PathBuf::from("b.rs"))))
    );
    // No `path` argument → no claim.
    assert_eq!(touched_target("read", &json!({})), Some((None, None)));
}

/// R29-Phase-L: every streamed delta is preserved verbatim as a
/// typed `assistant_chunk` event, in order, and
/// `synthia_session::assemble_chunks` rebuilds exactly the text
/// the assembled assistant turn carries.
#[tokio::test]
async fn assistant_chunks_are_persisted_in_order_and_reassemble() {
    use synthia_session::{SessionEvent, assemble_chunks};

    let deltas = ["a", "b", "c", "d", "e"];
    let mut chunks: Vec<StreamChunk> = deltas
        .iter()
        .map(|d| {
            StreamChunk::Content(ContentPart::Text(TextContent {
                text: (*d).to_string(),
                cache_control: None,
            }))
        })
        .collect();
    chunks.push(StreamChunk::IsDone {
        result: Box::new(SamplingResult {
            text: String::new(),
            tool_calls: vec![],
            reasoning: String::new(),
            reasoning_signature: None,
            usage: TokenUsage::default(),
            stop_reason: Some("end_turn".to_string()),
        }),
    });

    let provider: Arc<dyn ModelProvider> =
        Arc::new(ScriptedStreamProvider::new(vec![chunks]));
    let (sink, mut rx) = synthia_session::TypedEventSink::channel(64);
    let agent = ReActAgent::new(provider, Arc::new(ToolRegistry::new()))
        .with_typed_event_sink(sink);

    let mut stream = agent
        .run(AgentInput::text("hi"), Arc::new(CancellationToken::new()))
        .await;
    while stream.next().await.is_some() {}

    let mut events: Vec<SessionEvent> = Vec::new();
    while let Ok(Some(record)) = rx.try_recv() {
        events.push(record.event);
    }

    let chunk_events: Vec<&SessionEvent> = events
        .iter()
        .filter(|e| matches!(e, SessionEvent::AssistantChunk { .. }))
        .collect();
    assert_eq!(
        chunk_events.len(),
        deltas.len() + 1,
        "one assistant_chunk per streamed chunk (5 text + 1 terminal); got {chunk_events:?}"
    );
    match chunk_events.last().expect("terminal chunk") {
        SessionEvent::AssistantChunk {
            finish_reason,
            data,
            ..
        } => {
            assert_eq!(
                finish_reason.as_deref(),
                Some("end_turn"),
                "the terminal chunk must carry the provider stop reason"
            );
            assert_eq!(
                data.get("delta").and_then(|d| d.as_str()),
                Some(""),
                "the terminal chunk carries no delta"
            );
        }
        other => panic!("expected assistant_chunk, got {other:?}"),
    }
    assert_eq!(
        assemble_chunks(&events),
        "abcde",
        "the fold must rebuild exactly the streamed text"
    );
}

/// Both injection points treat a panicking hint the same way, and all
/// four `Tracker` methods do too: the surface degrades and the run
/// continues. Guarding only one of a trait's call sites would leave the
/// same consumer code fatal on one path and harmless on another.
#[tokio::test]
async fn panicking_hint_and_tracker_degrade_on_every_call_site() {
    use synthia_context::AgentState;
    use synthia_provider::TokenUsage;
    use synthia_steering::{Hint, HintMessage, tracker::Tracker};

    struct BoomHint {
        asked: Arc<std::sync::atomic::AtomicUsize>,
    }
    impl Hint for BoomHint {
        fn name(&self) -> &str {
            "boom-hint"
        }

        fn should_trigger(&self, _state: &AgentState) -> bool {
            self.asked.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            true
        }

        fn generate(&self, _state: &AgentState) -> HintMessage {
            panic!("hint blew up");
        }

        fn injection_point(&self) -> InjectionPoint {
            InjectionPoint::SystemPrompt
        }
    }

    /// Targets `AppendToToolResult`, so it is evaluated only by
    /// `append_tool_result_hints` (at commit time) and never by the
    /// pre-sample path — which is what makes this two-hint test cover
    /// both injection points rather than one twice.
    struct BoomToolHint {
        asked: Arc<std::sync::atomic::AtomicUsize>,
    }
    impl Hint for BoomToolHint {
        fn name(&self) -> &str {
            "boom-tool-hint"
        }

        fn should_trigger(&self, _state: &AgentState) -> bool {
            self.asked.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            true
        }

        fn generate(&self, _state: &AgentState) -> HintMessage {
            panic!("tool hint blew up");
        }

        fn injection_point(&self) -> InjectionPoint {
            InjectionPoint::AppendToToolResult {
                tool_name: "echo".to_string(),
            }
        }
    }

    /// Records which methods ran, then panics — so the test can prove
    /// each was *reached* and survived its panic, rather than skipped.
    type Seen = Arc<parking_lot::Mutex<std::collections::BTreeSet<String>>>;
    struct BoomTracker {
        seen: Seen,
    }
    impl BoomTracker {
        fn note(&self, which: &str) -> ! {
            self.seen.lock().insert(which.to_string());
            panic!("tracker.{which}");
        }
    }
    impl Tracker for BoomTracker {
        fn on_iteration(&self, _state: &AgentState) {
            self.note("on_iteration");
        }

        fn on_tool_call(
            &self,
            _name: &str,
            _arguments: &serde_json::Value,
            _state: &AgentState,
        ) {
            self.note("on_tool_call");
        }

        fn on_llm_response(&self, _usage: &TokenUsage, _state: &AgentState) {
            self.note("on_llm_response");
        }

        fn recommended_concurrency(&self, _state: &AgentState) -> usize {
            self.note("recommended_concurrency");
        }
    }

    let provider: Arc<dyn ModelProvider> =
        Arc::new(ScriptedStreamProvider::new(vec![
            tool_call_response(vec![ToolUse {
                id: "call_1".to_string(),
                name: "echo".to_string(),
                input: json!({}),
            }]),
            empty_response(),
        ]));
    let (registry, _calls) = echo_registry();

    let seen: Seen = Arc::new(parking_lot::Mutex::new(Default::default()));
    let pre_sample_asked = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let tool_result_asked = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let steering = Arc::new(synthia_steering::Steering {
        hints: vec![
            Arc::new(BoomHint {
                asked: Arc::clone(&pre_sample_asked),
            }),
            Arc::new(BoomToolHint {
                asked: Arc::clone(&tool_result_asked),
            }),
        ],
        tracker: Arc::new(BoomTracker {
            seen: Arc::clone(&seen),
        }),
        ..synthia_steering::Steering::noop()
    });

    let agent = ReActAgent::new(provider, registry).with_steering(steering);
    let mut stream = agent
        .run(AgentInput::text("go"), Arc::new(CancellationToken::new()))
        .await;
    let mut events = Vec::new();
    while let Some(event) = stream.next().await {
        events.push(event);
    }

    let reason = events.iter().find_map(|e| match e {
        AgentEvent::System(SystemEvent::SessionEnded { reason }) => {
            Some(reason.clone())
        }
        _ => None,
    });
    assert!(
        matches!(reason, Some(SessionEndReason::Completed)),
        "a panicking hint/tracker must not end the run, got {reason:?} \
         from {:?}",
        events.iter().map(|e| e.kind()).collect::<Vec<_>>()
    );
    let seen = seen.lock().clone();
    assert_eq!(
        seen.len(),
        4,
        "every Tracker method must have been reached and survived its \
         panic; saw {seen:?}"
    );

    // Both injection points were exercised, and the tool-result hint was
    // reached *exactly once* — by `append_tool_result_hints`, not by the
    // pre-sample path. Without the injection-point filter in
    // `inject_hints`, the tool-result hint would be evaluated there once
    // per iteration as well (this run has two), so the count would be 3.
    use std::sync::atomic::Ordering;
    assert!(
        pre_sample_asked.load(Ordering::SeqCst) > 0,
        "the pre-sample hint site must have been exercised"
    );
    assert_eq!(
        tool_result_asked.load(Ordering::SeqCst),
        1,
        "the tool-result hint must be evaluated only at commit time"
    );
}

/// A `SystemPrompt`-point hint fires **once per session**, not once per
/// triggering iteration.
///
/// `hint.rs` documents the point as "appended to the system message once
/// at session start", and `SessionState::system_hinted` exists to make
/// that true. It was never set, so the guard around
/// `append_to_system_prompt` was a constant and a hint triggering on
/// every iteration re-appended itself each time — growing the system
/// message, which is exactly the prompt-cache stability this crate keeps
/// volatile facts out of the system prompt to protect.
#[tokio::test]
async fn system_prompt_hint_is_appended_once_per_session() {
    use synthia_context::AgentState;
    use synthia_steering::{
        Hint,
        HintMessage,
        HintPriority,
        hint::InjectionPoint,
    };

    const MARKER: &str = "SYSTEM-HINT-MARKER";

    struct AlwaysHint;
    impl Hint for AlwaysHint {
        fn name(&self) -> &str {
            "always"
        }

        fn should_trigger(&self, _state: &AgentState) -> bool {
            true
        }

        fn generate(&self, _state: &AgentState) -> HintMessage {
            HintMessage {
                content: MARKER.to_string(),
                priority: HintPriority::Normal,
            }
        }

        fn injection_point(&self) -> InjectionPoint {
            InjectionPoint::SystemPrompt
        }
    }

    // Two iterations: a tool call, then a final answer — so the hint is
    // consulted (and, before the fix, re-appended) twice.
    let provider = Arc::new(CapturingProvider::new(vec![
        tool_call_response(vec![ToolUse {
            id: "call_1".to_string(),
            name: "echo".to_string(),
            input: json!({}),
        }]),
        empty_response(),
    ]));
    let (registry, _calls) = echo_registry();

    let steering = Arc::new(synthia_steering::Steering {
        hints: vec![Arc::new(AlwaysHint)],
        ..synthia_steering::Steering::noop()
    });

    let agent =
        ReActAgent::new(provider.clone(), registry).with_steering(steering);
    let mut stream = agent
        .run(AgentInput::text("go"), Arc::new(CancellationToken::new()))
        .await;
    while stream.next().await.is_some() {}

    let captured = provider.captured.lock().await;
    assert!(
        captured.len() >= 2,
        "need at least two sampling passes to test re-append; got {}",
        captured.len()
    );
    // The last request shows the system prompt as it accumulated.
    let system_text = captured
        .last()
        .unwrap()
        .first()
        .map(|m| m.content.extract_text().unwrap_or_default())
        .unwrap_or_default();
    let count = system_text.matches(MARKER).count();
    assert!(
        count >= 1,
        "the hint must actually be appended at all; system message was: \
         {system_text}"
    );
    assert_eq!(
        count, 1,
        "a SystemPrompt hint must be appended exactly once per session; \
         system message was: {system_text}"
    );
}

/// Two `SystemPrompt` hints each get injected — one must not silence the
/// other.
///
/// The once-per-session guard is keyed by hint **name** for this reason:
/// a single boolean would let the first hint to trigger block every
/// other `SystemPrompt` hint for the rest of the run, silently. The
/// companion test above uses one hint and so cannot catch that.
#[tokio::test]
async fn every_system_prompt_hint_is_injected() {
    use synthia_context::AgentState;
    use synthia_steering::{
        Hint,
        HintMessage,
        HintPriority,
        hint::InjectionPoint,
    };

    struct Named(&'static str);
    impl Hint for Named {
        fn name(&self) -> &str {
            self.0
        }

        fn should_trigger(&self, _state: &AgentState) -> bool {
            true
        }

        fn generate(&self, _state: &AgentState) -> HintMessage {
            HintMessage {
                content: format!("MARKER-{}", self.0),
                priority: HintPriority::Normal,
            }
        }

        fn injection_point(&self) -> InjectionPoint {
            InjectionPoint::SystemPrompt
        }
    }

    let provider = Arc::new(CapturingProvider::new(vec![
        tool_call_response(vec![ToolUse {
            id: "call_1".to_string(),
            name: "echo".to_string(),
            input: json!({}),
        }]),
        empty_response(),
    ]));
    let (registry, _calls) = echo_registry();

    let steering = Arc::new(synthia_steering::Steering {
        hints: vec![Arc::new(Named("alpha")), Arc::new(Named("beta"))],
        ..synthia_steering::Steering::noop()
    });

    let agent =
        ReActAgent::new(provider.clone(), registry).with_steering(steering);
    let mut stream = agent
        .run(AgentInput::text("go"), Arc::new(CancellationToken::new()))
        .await;
    while stream.next().await.is_some() {}

    let captured = provider.captured.lock().await;
    let system_text = captured
        .last()
        .and_then(|req| req.first())
        .map(|m| m.content.extract_text().unwrap_or_default())
        .unwrap_or_default();
    for name in ["alpha", "beta"] {
        assert_eq!(
            system_text.matches(&format!("MARKER-{name}")).count(),
            1,
            "hint `{name}` must be injected exactly once; system message \
             was: {system_text}"
        );
    }
}
