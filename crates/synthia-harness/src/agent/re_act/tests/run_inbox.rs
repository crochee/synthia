//! Run inbox (steering / follow-up seams) + length-stop guard.

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use async_trait::async_trait;
use futures::StreamExt;
use serde_json::json;
use synthia_context::{AgentState, ContextManager};
use synthia_core::{CancelToken, Error};
use synthia_provider::{
    CompletionResponse,
    ContentPart,
    Message,
    ProviderConfig,
    Role,
    SamplingResult,
    StreamChunk,
    TextContent,
    TokenUsage,
    traits::ModelProvider,
    types::ModelConfig,
};
use synthia_tool::{Tool, ToolOutput, ToolRegistry};
use tokio::sync::Mutex as TokioMutex;
use tokio_util::sync::CancellationToken;

use super::{support::*, *};
use crate::{
    agent::{MpscInbox, RunInbox, RunInboxHandle},
    events::{SessionEndReason, SteeringSource},
};

/// Which run-inbox queue a probe provider pushes into, and on
/// which 1-based `complete_with_stream` call.
enum ProbePush {
    Steering(usize, &'static str),
    FollowUp(usize, &'static str),
}

/// Provider that records every request's message list and can
/// push a message into the run-inbox handle while serving a
/// chosen call, simulating a user typing exactly mid-run.
struct InboxProbeProvider {
    captured: Arc<TokioMutex<Vec<Arc<Vec<Message>>>>>,
    scripted: Arc<TokioMutex<Vec<Vec<StreamChunk>>>>,
    handle: RunInboxHandle,
    push: ProbePush,
    call_count: AtomicUsize,
}

impl InboxProbeProvider {
    fn new(
        scripted: Vec<Vec<StreamChunk>>,
        handle: RunInboxHandle,
        push: ProbePush,
    ) -> Self {
        Self {
            captured: Arc::new(TokioMutex::new(Vec::new())),
            scripted: Arc::new(TokioMutex::new(scripted)),
            handle,
            push,
            call_count: AtomicUsize::new(0),
        }
    }

    /// Push the configured message when `call` (1-based) is the
    /// one being served.
    fn push_if_due(&self, call: usize) {
        match self.push {
            ProbePush::Steering(due, text) if due == call => {
                self.handle
                    .send_steering(Message::user(text))
                    .expect("inbox receiver alive");
            }
            ProbePush::FollowUp(due, text) if due == call => {
                self.handle
                    .send_follow_up(Message::user(text))
                    .expect("inbox receiver alive");
            }
            _ => {}
        }
    }
}

#[async_trait]
impl ModelProvider for InboxProbeProvider {
    async fn initialize(
        &mut self,
        _config: ProviderConfig,
    ) -> Result<(), Error> {
        Ok(())
    }

    fn name(&self) -> &str {
        "inbox-probe"
    }

    fn model_config(&self) -> ModelConfig {
        ModelConfig {
            name: "fake".to_string(),
            provider: "scripted".to_string(),
            context_window: 128_000,
            max_output_tokens: 4096,
            supports_tools: true,
            supports_streaming: true,
            supports_reasoning: true,
        }
    }

    async fn complete(
        &self,
        _request: synthia_provider::CompletionRequest,
    ) -> Result<CompletionResponse, Error> {
        unreachable!("streaming path should not call complete()")
    }

    async fn complete_with_stream(
        &self,
        request: synthia_provider::CompletionRequest,
        _cancel_token: Option<Arc<dyn CancelToken>>,
        mut on_delta: Box<dyn FnMut(StreamChunk) + Send>,
    ) -> Result<CompletionResponse, Error> {
        let call = self.call_count.fetch_add(1, Ordering::SeqCst) + 1;
        self.captured.lock().await.push(request.messages.clone());
        self.push_if_due(call);
        let chunks = {
            let mut guard = self.scripted.lock().await;
            if guard.is_empty() {
                vec![StreamChunk::IsDone {
                    result: Box::new(SamplingResult::default()),
                }]
            } else {
                guard.remove(0)
            }
        };
        let mut final_sampling: Option<SamplingResult> = None;
        for chunk in chunks {
            if let StreamChunk::IsDone { result } = &chunk {
                final_sampling = Some((**result).clone());
            }
            on_delta(chunk);
        }
        let sampling = final_sampling.unwrap_or_default();
        Ok(CompletionResponse {
            id: format!("probe-{call}"),
            model: "fake".to_string(),
            content: Content::Single(ContentPart::Text(TextContent {
                text: sampling.text.clone(),
                cache_control: None,
            })),
            usage: sampling.usage.clone(),
            cached: false,
            replay_state: None,
            stop_reason: sampling.stop_reason.clone(),
        })
    }
}

/// Text-only terminal step with a normal stop reason.
fn text_response(text: &str) -> Vec<StreamChunk> {
    vec![StreamChunk::IsDone {
        result: Box::new(SamplingResult {
            text: text.to_string(),
            tool_calls: vec![],
            reasoning: String::new(),
            reasoning_signature: None,
            usage: TokenUsage::default(),
            stop_reason: Some("end_turn".to_string()),
        }),
    }]
}

/// Drive one [`ReActAgent`] to completion, draining the event
/// stream.
async fn drive_collecting(
    agent: ReActAgent,
    input: AgentInput,
) -> Vec<AgentEvent> {
    let mut stream = agent.run(input, Arc::new(CancellationToken::new())).await;
    let mut out = Vec::new();
    while let Some(ev) = stream.next().await {
        out.push(ev);
    }
    out
}

/// Joined text content of a message (multi-part joined with
/// newlines), for message-order assertions.
fn message_text(message: &Message) -> String {
    message
        .content
        .iter()
        .filter_map(ContentPart::text)
        .collect::<Vec<&str>>()
        .join("\n")
}

/// Role sequence of a captured request, for order assertions.
fn roles_of(messages: &[Message]) -> Vec<Role> {
    messages.iter().map(|m| m.role).collect()
}

/// Every `SteeringInjected` event in the stream, in order.
fn steering_injections(events: &[AgentEvent]) -> Vec<(SteeringSource, usize)> {
    events
        .iter()
        .filter_map(|e| match e {
            AgentEvent::System(SystemEvent::SteeringInjected {
                source,
                count,
            }) => Some((source.clone(), *count)),
            _ => None,
        })
        .collect()
}

/// A steering message typed while the run is in flight MUST be
/// injected as a trailing user turn between the tool-result
/// commit and the next sampling pass — never mid-turn, never
/// dropped.
#[tokio::test]
async fn steering_message_is_injected_between_tool_round_and_next_sample() {
    let tool_use = ToolUse {
        id: "c1".to_string(),
        name: "echo".to_string(),
        input: json!({ "text": "hi" }),
    };
    let (inbox, handle) = MpscInbox::channel();
    let provider = Arc::new(InboxProbeProvider::new(
        vec![tool_call_response(vec![tool_use]), text_response("done")],
        handle,
        ProbePush::Steering(1, "actually, check the tests first"),
    ));
    let (registry, _executions) = echo_registry();
    let agent = ReActAgent::new(provider.clone(), registry)
        .with_run_inbox(Arc::new(inbox));

    let events = drive_collecting(agent, AgentInput::text("hi")).await;
    let requests = provider.captured.lock().await;
    assert_eq!(requests.len(), 2, "one tool round + one final pass");
    // R30 Slice C: the runtime-context snapshot is appended
    // ONCE before the first sample, then skipped on
    // subsequent turns when the rendered snapshot is
    // byte-identical (zero-token cost, prefix cache
    // intact). The first request therefore gains one
    // trailing User turn; the second does not, since
    // `last_runtime_snapshot` matches.
    assert_eq!(
        roles_of(&requests[0]),
        vec![Role::System, Role::User, Role::User],
        "the first request carries the runtime-context snapshot appended before the sample"
    );
    assert_eq!(
        roles_of(&requests[1]),
        vec![
            Role::System,
            Role::User,
            Role::User,
            Role::Assistant,
            Role::Tool,
            Role::User,
        ],
        "steering lands as a trailing user turn after the tool result"
    );
    // The trailing user turn IS the steering message:
    // the runtime-context snapshot was unchanged across
    // turns, so the loop skipped the append on the
    // second iteration.
    // Walk back to find the injected steering turn.
    let second = &requests[1];
    let steering_turn = second
        .iter()
        .rev()
        .find(|m| {
            matches!(m.role, Role::User)
                && match &m.content {
                    synthia_provider::Content::Single(ContentPart::Text(t)) => {
                        t.text == "actually, check the tests first"
                    }
                    _ => false,
                }
        })
        .expect("injected steering turn present");
    assert_eq!(
        message_text(steering_turn),
        "actually, check the tests first"
    );
    drop(requests);

    assert_eq!(
        steering_injections(&events),
        vec![(SteeringSource::Steering, 1)],
        "the injection must be observable exactly once in the event stream"
    );
}

/// A steering message queued while a long context re-budget
/// (compaction) runs MUST reach the next sampling pass — nothing
/// is lost, and it is injected exactly once.
#[tokio::test]
async fn steering_queued_during_compaction_reaches_the_next_sample() {
    /// Manager that pushes a steering message during its second
    /// `prepare` (the post-tool re-budget).
    struct PushDuringPrepare {
        handle: RunInboxHandle,
        prepares: AtomicUsize,
    }

    #[async_trait]
    impl ContextManager for PushDuringPrepare {
        async fn prepare(
            &self,
            _messages: &mut Vec<Message>,
            _state: &mut AgentState,
        ) {
            // Call 1 = run start, call 2 = post-tool re-budget.
            if self.prepares.fetch_add(1, Ordering::SeqCst) == 1 {
                self.handle
                    .send_steering(Message::user("typed during compaction"))
                    .expect("inbox receiver alive");
            }
        }
    }

    let tool_use = ToolUse {
        id: "c1".to_string(),
        name: "echo".to_string(),
        input: json!({ "text": "hi" }),
    };
    let (inbox, handle) = MpscInbox::channel();
    let provider = Arc::new(CapturingProvider::new(vec![
        tool_call_response(vec![tool_use]),
        text_response("done"),
    ]));
    let (registry, _executions) = echo_registry();
    let agent = ReActAgent::new(provider.clone(), registry)
        .with_context_manager(Arc::new(PushDuringPrepare {
            handle,
            prepares: AtomicUsize::new(0),
        }))
        .with_run_inbox(Arc::new(inbox));

    let events = drive_collecting(agent, AgentInput::text("go")).await;

    let requests = provider.captured.lock().await;
    assert_eq!(requests.len(), 2);
    // R30 Slice C: the runtime-context snapshot was
    // appended once before the first sample; the second
    // iteration renders the SAME snapshot, so the loop
    // skips the re-append. The trailing user turn in the
    // second request is therefore the compaction-window
    // steering message, not a duplicate snapshot.
    assert_eq!(
        roles_of(&requests[1]),
        vec![
            Role::System,
            Role::User,
            Role::User,
            Role::Assistant,
            Role::Tool,
            Role::User,
        ]
    );
    let steering_turn = requests[1]
        .iter()
        .rev()
        .find(|m| {
            matches!(m.role, Role::User)
                && match &m.content {
                    synthia_provider::Content::Single(ContentPart::Text(t)) => {
                        t.text == "typed during compaction"
                    }
                    _ => false,
                }
        })
        .expect("compaction-window steering turn present");
    assert_eq!(message_text(steering_turn), "typed during compaction");
    drop(requests);

    assert_eq!(
        steering_injections(&events),
        vec![(SteeringSource::Steering, 1)],
        "compaction-window steering must be injected exactly once"
    );
}
/// A follow-up message typed while the run is in flight MUST
/// revive the about-to-stop run instead of silently dropping.
#[tokio::test]
async fn follow_up_message_revives_about_to_stop_run() {
    let (inbox, handle) = MpscInbox::channel();
    let provider = Arc::new(InboxProbeProvider::new(
        vec![
            text_response("first answer"),
            text_response("second answer"),
        ],
        handle,
        ProbePush::FollowUp(1, "also update the changelog"),
    ));
    let agent =
        ReActAgent::new(provider.clone(), Arc::new(ToolRegistry::new()))
            .with_run_inbox(Arc::new(inbox));

    let events = drive_collecting(agent, AgentInput::text("summarize")).await;

    assert_eq!(
        provider.call_count.load(Ordering::SeqCst),
        2,
        "the follow-up must drive a second sampling pass"
    );
    let requests = provider.captured.lock().await;
    assert_eq!(requests.len(), 2);
    // R30 Slice C: the runtime-context snapshot is appended
    // once before the first sample and skipped thereafter
    // while the rendered body is byte-identical, so the
    // second request carries the follow-up as its trailing
    // user turn and no duplicate snapshot.
    assert_eq!(
        roles_of(&requests[1]),
        vec![
            Role::System,
            Role::User,
            Role::User,
            Role::Assistant,
            Role::User,
        ],
        "the follow-up lands after the first final answer; \
         the unchanged runtime-context snapshot is not re-appended"
    );
    // The follow-up turn is the trailing user turn containing
    // the follow-up text.
    let follow_up_turn = requests[1]
        .iter()
        .rev()
        .find(|m| {
            matches!(m.role, Role::User)
                && match &m.content {
                    synthia_provider::Content::Single(ContentPart::Text(t)) => {
                        t.text == "also update the changelog"
                    }
                    _ => false,
                }
        })
        .expect("follow-up turn present");
    assert_eq!(message_text(follow_up_turn), "also update the changelog");
    drop(requests);

    // The run ends only after the revived turn.
    assert!(
        events.iter().any(|e| matches!(
            e,
            AgentEvent::ModelDone(r) if r.text == "second answer"
        )),
        "the revived sampling pass must reach the stream"
    );
    let ends: Vec<&SystemEvent> = events
        .iter()
        .filter_map(|e| match e {
            AgentEvent::System(ev @ SystemEvent::SessionEnded { .. }) => {
                Some(ev)
            }
            _ => None,
        })
        .collect();
    assert_eq!(ends.len(), 1, "exactly one terminal event");
    assert!(matches!(
        ends[0],
        SystemEvent::SessionEnded {
            reason: SessionEndReason::Completed
        }
    ));
    assert_eq!(
        steering_injections(&events),
        vec![(SteeringSource::FollowUp, 1)],
        "the revival must be observable in the event stream"
    );
}

/// The revived sample MUST be re-budgeted: the context manager
/// sees the follow-up as the newest message before the provider
/// call, upholding the loop's "every sample sees a freshly
/// budgeted list" invariant.
#[tokio::test]
async fn follow_up_revival_rebudgets_before_the_next_sample() {
    /// Manager that records the newest message it was asked to
    /// prepare, so the test can observe re-budget timing.
    struct SnapshottingManager {
        newest: Arc<TokioMutex<Vec<String>>>,
    }

    #[async_trait]
    impl ContextManager for SnapshottingManager {
        async fn prepare(
            &self,
            messages: &mut Vec<Message>,
            _state: &mut AgentState,
        ) {
            let last = messages.last().map(message_text).unwrap_or_default();
            self.newest.lock().await.push(last);
        }
    }

    let (inbox, handle) = MpscInbox::channel();
    let provider = Arc::new(InboxProbeProvider::new(
        vec![
            text_response("first answer"),
            text_response("second answer"),
        ],
        handle,
        ProbePush::FollowUp(1, "also update the changelog"),
    ));
    let newest = Arc::new(TokioMutex::new(Vec::new()));
    let agent = ReActAgent::new(provider, Arc::new(ToolRegistry::new()))
        .with_context_manager(Arc::new(SnapshottingManager {
            newest: Arc::clone(&newest),
        }))
        .with_run_inbox(Arc::new(inbox));

    drive_collecting(agent, AgentInput::text("summarize")).await;

    assert_eq!(
        *newest.lock().await,
        vec![
            "summarize".to_string(),
            "also update the changelog".to_string()
        ],
        "the re-budget before the revived sample must observe the follow-up"
    );
}

/// A follow-up waiting when the iteration budget is already
/// exhausted MUST NOT be drained: it could never be sampled, and
/// draining would silently discard it. It stays queued for a
/// later run on the same inbox.
#[tokio::test]
async fn follow_up_is_not_drained_when_no_iteration_remains() {
    let (raw_inbox, handle) = MpscInbox::channel();
    handle
        .send_follow_up(Message::user("too late"))
        .expect("inbox receiver alive");
    let inbox: Arc<dyn RunInbox> = Arc::new(raw_inbox);
    let provider =
        Arc::new(CapturingProvider::new(vec![text_response("only answer")]));
    let agent =
        ReActAgent::new(provider.clone(), Arc::new(ToolRegistry::new()))
            .with_max_iterations(1)
            .with_run_inbox(Arc::clone(&inbox));

    let events = drive_collecting(agent, AgentInput::text("go")).await;

    assert_eq!(
        provider.call_count.load(Ordering::SeqCst),
        1,
        "no iteration remains, so no revived sampling pass"
    );
    assert!(
        steering_injections(&events).is_empty(),
        "no injection event may claim delivery of an unsampled message"
    );
    assert!(matches!(
        events.last(),
        Some(AgentEvent::System(SystemEvent::SessionEnded {
            reason: SessionEndReason::Completed,
        }))
    ));
    assert_eq!(
        inbox.take_follow_up().await.len(),
        1,
        "the follow-up must still be queued for a later run"
    );
}

/// A length stop with tool calls MUST fail the whole batch: no
/// tool executes, every call gets a model-facing error result,
/// and the loop continues so the model can re-issue. Covers both
/// provider vocabularies (`max_tokens`, `length`).
#[tokio::test]
async fn length_stop_fails_the_tool_batch_without_executing_any_call() {
    for stop_reason in ["max_tokens", "length"] {
        let tool_use = ToolUse {
            id: "c1".to_string(),
            name: "echo".to_string(),
            input: json!({ "text": "hi" }),
        };
        let mut truncated = tool_call_response(vec![tool_use]);
        match truncated.last_mut() {
            Some(StreamChunk::IsDone { result }) => {
                result.stop_reason = Some(stop_reason.to_string());
            }
            other => panic!("expected terminal IsDone chunk; got {other:?}"),
        }

        let (registry, executions) = echo_registry();
        let provider = Arc::new(ScriptedStreamProvider::new(vec![
            truncated,
            text_response("recovered"),
        ]));
        let events = run_and_collect(
            provider,
            registry,
            CancellationToken::new(),
            AgentInput::text("go"),
        )
        .await;

        assert_eq!(
            *executions.lock(),
            0,
            "({stop_reason}) no tool call may run when the message was truncated by the token limit"
        );

        // The model still receives one ToolResult per call —
        // flagged as an error telling it to re-issue.
        let results: Vec<&synthia_provider::ToolResult> = events
            .iter()
            .filter_map(|e| match e {
                AgentEvent::Model(ContentPart::ToolResult(tr)) => Some(tr),
                _ => None,
            })
            .collect();
        assert_eq!(results.len(), 1, "({stop_reason}) one result per call");
        let tr = results[0];
        assert_eq!(tr.tool_use_id, "c1");
        assert_eq!(tr.is_error, Some(true));
        let text = tr
            .content
            .iter()
            .filter_map(ContentPart::text)
            .collect::<Vec<&str>>()
            .join("\n");
        assert!(
            text.contains("was not executed")
                && text.contains("output token limit"),
            "({stop_reason}) the error must be model-facing and usable; got: {text}"
        );

        // The loop continued: the model recovered on the next pass.
        assert!(
            events.iter().any(|e| matches!(
                e,
                AgentEvent::ModelDone(r) if r.text == "recovered"
            )),
            "({stop_reason}) the run must continue after failing the batch"
        );
        assert!(matches!(
            events.last(),
            Some(AgentEvent::System(SystemEvent::SessionEnded {
                reason: SessionEndReason::Completed,
            }))
        ));
    }
}

/// The length-stop classifier recognizes both provider
/// spellings and nothing else.
#[test]
fn is_length_stop_matches_provider_token_limit_reasons_only() {
    assert!(is_length_stop(&Some("length".to_string())));
    assert!(is_length_stop(&Some("max_tokens".to_string())));
    assert!(
        is_length_stop(&Some("MAX_TOKENS".to_string())),
        "gateway normalizations must not slip past the guard"
    );
    assert!(!is_length_stop(&Some("end_turn".to_string())));
    assert!(!is_length_stop(&Some("tool_use".to_string())));
    assert!(!is_length_stop(&Some("stop".to_string())));
    assert!(!is_length_stop(&None));
}

/// Without an inbox the loop injects nothing: the message order
/// is exactly the pre-seam shape and no injection event fires.
#[tokio::test]
async fn without_an_inbox_the_loop_injects_nothing() {
    let tool_use = ToolUse {
        id: "c1".to_string(),
        name: "echo".to_string(),
        input: json!({ "text": "hi" }),
    };
    let provider = Arc::new(CapturingProvider::new(vec![
        tool_call_response(vec![tool_use]),
        text_response("done"),
    ]));
    let (registry, _executions) = echo_registry();
    let agent = ReActAgent::new(provider.clone(), registry);
    assert!(
        agent.run_inbox().is_none(),
        "default construction wires no inbox"
    );

    let _events = drive_collecting(agent, AgentInput::text("hi")).await;
    let requests = provider.captured.lock().await;
    assert_eq!(requests.len(), 2);
    // ONCE before the first sample, then skipped on the
    // second turn when the rendered snapshot is
    // byte-identical. Without an inbox, no OTHER trailing
    // user turns may appear — the snapshot is a
    // cache-stable state declaration, not an interactive
    // injection.
    assert_eq!(
        roles_of(&requests[0]),
        vec![Role::System, Role::User, Role::User],
        "the first request carries the runtime-context snapshot"
    );
    assert_eq!(
        roles_of(&requests[1]),
        vec![
            Role::System,
            Role::User,
            Role::User,
            Role::Assistant,
            Role::Tool,
        ],
        "no trailing user injection may appear without an inbox; \
         the runtime-context snapshot was unchanged so the loop skipped the re-append"
    );
    assert_eq!(requests[1].len(), 5);
    drop(requests);
}

/// Wiring an inbox that never receives anything must not change
/// the run: the observable event stream is identical to a
/// default (inbox-less) run byte-for-byte.
#[tokio::test]
async fn idle_inbox_leaves_the_event_stream_unchanged() {
    let tool_use = ToolUse {
        id: "c1".to_string(),
        name: "echo".to_string(),
        input: json!({ "text": "hi" }),
    };
    let script = || {
        vec![
            tool_call_response(vec![tool_use.clone()]),
            text_response("done"),
        ]
    };

    let plain_provider = Arc::new(ScriptedStreamProvider::new(script()));
    let (plain_registry, _) = echo_registry();
    let plain = drive_collecting(
        ReActAgent::new(plain_provider, plain_registry),
        AgentInput::text("hi"),
    )
    .await;

    let (inbox, _handle) = MpscInbox::channel();
    let inbox_provider = Arc::new(ScriptedStreamProvider::new(script()));
    let (inbox_registry, _) = echo_registry();
    let with_inbox = drive_collecting(
        ReActAgent::new(inbox_provider, inbox_registry)
            .with_run_inbox(Arc::new(inbox)),
        AgentInput::text("hi"),
    )
    .await;

    assert_eq!(
        serde_json::to_value(&plain).expect("serialize plain run"),
        serde_json::to_value(&with_inbox).expect("serialize inbox run"),
        "an idle inbox must be unobservable"
    );
}

/// A tool whose `ToolOutput` carries a non-text part (an image) must
/// reach the wire `ToolResult` with that part intact — the loop's
/// transformer rewrites the *text* projection only, so the binary is
/// never flattened away.
///
/// This is the agent-side half of the MCP multimodal chain: an MCP
/// `tools/call` reply with an image block becomes
/// `ToolOutput::from_parts([Text, Image])` and rides through here to
/// the provider request.
#[tokio::test]
async fn non_text_tool_output_parts_reach_the_wire_tool_result() {
    struct ImageTool;

    #[async_trait]
    impl Tool for ImageTool {
        fn name(&self) -> &str {
            "screenshot"
        }

        fn description(&self) -> &str {
            "Returns a screenshot plus a caption."
        }

        fn parameters(&self) -> serde_json::Value {
            json!({"type": "object"})
        }

        async fn call(
            &self,
            _input: serde_json::Value,
            _context: &synthia_tool::Context,
        ) -> ToolOutput {
            ToolOutput::from_parts(vec![
                ContentPart::Text(synthia_provider::types::TextContent {
                    text: "captured".into(),
                    cache_control: None,
                }),
                ContentPart::Image(synthia_provider::types::ImageContent {
                    data: "iVBORw0KGgoAAAANSUhEUg==".into(),
                    mime_type: "image/png".into(),
                    detail: None,
                }),
            ])
        }
    }

    let tool_use = ToolUse {
        id: "c1".to_string(),
        name: "screenshot".to_string(),
        input: json!({}),
    };
    let provider = Arc::new(ScriptedStreamProvider::new(vec![
        tool_call_response(vec![tool_use]),
        empty_response(),
    ]));
    let registry = Arc::new(ToolRegistry::new());
    registry.register_entry(synthia_tool::ToolEntry::new(Arc::new(ImageTool)));

    let events = run_and_collect(
        provider,
        registry,
        CancellationToken::new(),
        AgentInput::text("screenshot please"),
    )
    .await;

    let content = events
        .iter()
        .find_map(|e| match e {
            AgentEvent::Model(ContentPart::ToolResult(tr))
                if tr.tool_name.as_deref() == Some("screenshot") =>
            {
                Some(tr.content.clone())
            }
            _ => None,
        })
        .expect("the screenshot tool result must reach the wire");

    assert_eq!(content.len(), 2, "text + image: {content:?}");
    assert!(content.iter().any(|p| p.text() == Some("captured")));
    let image = content
        .iter()
        .find_map(|p| match p {
            ContentPart::Image(img) => Some(img),
            _ => None,
        })
        .expect("the image part must survive the loop");
    assert_eq!(image.data, "iVBORw0KGgoAAAANSUhEUg==");
    assert_eq!(image.mime_type, "image/png");
}

/// A panicking inbox must not take the run's error reporting with it.
///
/// The inbox is consumer-supplied (`with_run_inbox`), so a panic in it is
/// third-party code panicking — the same class as a panicking tool. On
/// the pre-guard code this ended the stream with no `SessionEnded`.
#[tokio::test]
async fn panicking_inbox_still_reports_session_end() {
    struct PanickingInbox;

    #[async_trait]
    impl RunInbox for PanickingInbox {
        async fn take_steering(&self) -> Vec<Message> {
            panic!("inbox blew up");
        }
    }

    let provider = Arc::new(ScriptedStreamProvider::new(vec![
        empty_response(),
        empty_response(),
    ]));
    let agent = ReActAgent::new(provider, Arc::new(ToolRegistry::new()))
        .with_run_inbox(Arc::new(PanickingInbox));

    let mut stream = agent
        .run(AgentInput::text("go"), Arc::new(CancellationToken::new()))
        .await;
    let mut events = Vec::new();
    while let Some(event) = stream.next().await {
        events.push(event);
    }

    assert!(
        events.iter().any(|e| matches!(
            e,
            AgentEvent::System(SystemEvent::SessionEnded { .. })
        )),
        "a panicking inbox must not swallow the terminal event; got {:?}",
        events.iter().map(|e| e.kind()).collect::<Vec<_>>()
    );
    let warned = events.iter().any(|e| {
        matches!(
            e,
            AgentEvent::System(SystemEvent::Warning { message, .. })
                if message.contains("inbox blew up")
        )
    });
    assert!(warned, "the inbox panic must be surfaced, got {events:?}");
}

/// The same for a consumer-supplied `ContextManager`: a panic there
/// must not end the run unreported.
#[tokio::test]
async fn panicking_context_manager_still_reports_session_end() {
    struct PanickingManager;

    #[async_trait]
    impl ContextManager for PanickingManager {
        async fn prepare(
            &self,
            _messages: &mut Vec<Message>,
            _state: &mut AgentState,
        ) {
            panic!("context manager blew up");
        }
    }

    let provider = Arc::new(ScriptedStreamProvider::new(vec![
        empty_response(),
        empty_response(),
    ]));
    let agent = ReActAgent::new(provider, Arc::new(ToolRegistry::new()))
        .with_context_manager(Arc::new(PanickingManager));

    let mut stream = agent
        .run(AgentInput::text("go"), Arc::new(CancellationToken::new()))
        .await;
    let mut events = Vec::new();
    while let Some(event) = stream.next().await {
        events.push(event);
    }

    assert!(
        events.iter().any(|e| matches!(
            e,
            AgentEvent::System(SystemEvent::SessionEnded { .. })
        )),
        "a panicking context manager must not swallow the terminal event; \
         got {:?}",
        events.iter().map(|e| e.kind()).collect::<Vec<_>>()
    );
    // Ended *reported*, not resumed. `ContextManager::prepare` rewrites
    // the message list in place, so a panic mid-rewrite can leave a
    // half-truncated transcript — the vec the next provider call would
    // send and the session log would persist. Continuing would trade an
    // unreported death for a corrupt history, so the reason must be the
    // error itself rather than a warning the loop carries on from.
    let reason = events.iter().find_map(|e| match e {
        AgentEvent::System(SystemEvent::SessionEnded { reason }) => {
            Some(reason.clone())
        }
        _ => None,
    });
    match reason {
        Some(SessionEndReason::Error(message)) => assert!(
            message.contains("context manager blew up"),
            "the panic must reach the terminal reason, got {message}"
        ),
        other => panic!(
            "expected a reported Error end, got {other:?} from {events:?}"
        ),
    }
}

/// Nothing may reach the caller's stream after `SessionEnded`.
///
/// The cancelled-before-tool path (`dispatch_cancelled_before_tool`)
/// finalizes the run *inside* phase 4 — it emits `SessionEnded` and fans
/// out `OnAgentEnd` inline, and the orchestrator returns that pre-built
/// output verbatim. Phase 5's `close_iteration` therefore has a finished
/// run on its hands, and its inbox drain would both emit
/// `SteeringInjected` *after* the event that tells consumers the run is
/// over and append user turns to a terminal history.
///
/// Two things have to line up to see it: steering must be queued *after*
/// the iteration's pre-sample drain (otherwise the inbox is already
/// empty and the phase-5 drain finds nothing), and the cancel must land
/// after the provider call returns (a cancellation during the stream
/// ends as `Failed(Cancelled)` instead). The probe provider queues the
/// message mid-call; an `OnProviderEnd` hook fires the cancel.
#[tokio::test]
async fn nothing_follows_session_ended_on_the_cancelled_before_tool_path() {
    use std::time::Duration;

    use synthia_provider::CompletionResponse;
    use synthia_steering::hook::AgentHook;

    struct CancelAfterProvider {
        cancel: CancellationToken,
    }

    #[async_trait]
    impl AgentHook for CancelAfterProvider {
        fn name(&self) -> &str {
            "cancel-after-provider"
        }

        async fn on_provider_end(
            &self,
            _response: &CompletionResponse,
            _duration: Duration,
        ) {
            self.cancel.cancel();
        }
    }

    let cancel = CancellationToken::new();
    let (inbox, handle) = MpscInbox::channel();
    let provider = Arc::new(InboxProbeProvider::new(
        vec![tool_call_response(vec![ToolUse {
            id: "call_1".to_string(),
            name: "echo".to_string(),
            input: json!({}),
        }])],
        handle,
        ProbePush::Steering(1, "queued mid-sample, lands too late"),
    ));
    let (registry, _calls) = echo_registry();

    let hook: Arc<dyn AgentHook> = Arc::new(CancelAfterProvider {
        cancel: cancel.clone(),
    });
    let steering = Arc::new(synthia_steering::Steering {
        hooks: vec![hook],
        ..synthia_steering::Steering::noop()
    });

    let agent = ReActAgent::new(provider, registry)
        .with_run_inbox(Arc::new(inbox))
        .with_steering(steering);

    let mut stream = agent.run(AgentInput::text("go"), Arc::new(cancel)).await;
    let mut events = Vec::new();
    while let Some(event) = stream.next().await {
        events.push(event);
    }

    let kinds: Vec<&str> = events.iter().map(|e| e.kind()).collect();
    let ended = events
        .iter()
        .position(|e| {
            matches!(e, AgentEvent::System(SystemEvent::SessionEnded { .. }))
        })
        .unwrap_or_else(|| panic!("the run must end reported; got {kinds:?}"));
    assert_eq!(
        ended,
        events.len() - 1,
        "SessionEnded must be the last event; got {kinds:?}"
    );
    assert!(
        !events[ended + 1..].iter().any(|e| matches!(
            e,
            AgentEvent::System(SystemEvent::SteeringInjected { .. })
        )),
        "steering must not be injected into a finished run: {kinds:?}"
    );
}
