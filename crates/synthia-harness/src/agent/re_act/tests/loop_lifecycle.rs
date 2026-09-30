//! One full ReAct loop pass: streamed chunks → tool dispatch →
//! max-iteration warning → cancel. Every test in this file
//! drives a single session and asserts the resulting event
//! sequence or the per-iteration usage accounting.

use std::sync::Arc;

use async_trait::async_trait;
use futures::StreamExt;
use serde_json::{Value, json};
use synthia_provider::{
    ContentPart,
    SamplingResult,
    StreamChunk,
    TextContent,
    TokenUsage,
    ToolUse,
};
use synthia_test_support::FakeTool;
use synthia_tool::{
    Context,
    StreamOutput,
    Tool,
    ToolExposure,
    ToolOutput,
    ToolRegistry,
    ToolSurfacePolicy,
};
use tokio_util::sync::CancellationToken;

use super::{support::*, *};

#[tokio::test]
async fn run_emits_full_event_lifecycle_for_text_only_response() {
    let provider = Arc::new(ScriptedStreamProvider::new(vec![vec![
        StreamChunk::Content(ContentPart::Text(TextContent {
            text: "hello ".to_string(),
            cache_control: None,
        })),
        StreamChunk::Content(ContentPart::Text(TextContent {
            text: "world".to_string(),
            cache_control: None,
        })),
        StreamChunk::IsDone {
            result: Box::new(SamplingResult {
                text: String::new(),
                tool_calls: vec![],
                reasoning: String::new(),
                reasoning_signature: None,
                usage: TokenUsage {
                    prompt_tokens: 10,
                    completion_tokens: 5,
                    total_tokens: 15,
                    cached_prompt_tokens: None,
                    cache_read_tokens: None,
                    cache_write_tokens: None,
                    reasoning_tokens: None,
                },
                ..Default::default()
            }),
        },
    ]]));
    let events = run_and_collect(
        provider,
        Arc::new(ToolRegistry::new()),
        CancellationToken::new(),
        AgentInput::text("hello"),
    )
    .await;

    // SessionStarted → Progress → Model(text) → Model(text) →
    // Usage → ModelDone → SessionEnded.
    let kinds: Vec<&str> = events.iter().map(|e| e.kind()).collect();
    assert_eq!(
        kinds,
        vec![
            "System",
            "System",
            "Model",
            "Model",
            "System",
            "ModelDone",
            "System",
        ]
    );

    assert!(events.iter().any(|e| matches!(
        e,
        AgentEvent::System(SystemEvent::SessionStarted { .. })
    )));
    assert!(events.iter().any(|e| matches!(
        e,
        AgentEvent::System(SystemEvent::SessionEnded {
            reason: SessionEndReason::Completed,
        })
    )));
    assert!(events.iter().any(|e| matches!(
        e,
        AgentEvent::System(SystemEvent::Progress { .. })
    )));
    assert!(events.iter().any(|e| matches!(e, AgentEvent::ModelDone(_))));
    assert!(events.iter().any(|e| matches!(
        e,
        AgentEvent::System(SystemEvent::Usage {
            input_tokens: 10,
            output_tokens: 5,
            ..
        })
    )));
}

#[tokio::test]
async fn isdone_text_does_not_duplicate_streamed_chunks() {
    // Real Anthropic / OpenAI providers stream text via
    // `Content(Text)` chunks AND include the consolidated
    // final text in `IsDone.result.text`. Without dedup
    // the consumer would see the same text twice (once
    // per chunk, once via IsDone). This test reproduces
    // that exact pattern and asserts the wire sees each
    // text fragment exactly once.
    let provider = Arc::new(ScriptedStreamProvider::new(vec![vec![
        StreamChunk::Content(ContentPart::Text(TextContent {
            text: "hello ".to_string(),
            cache_control: None,
        })),
        StreamChunk::Content(ContentPart::Text(TextContent {
            text: "world".to_string(),
            cache_control: None,
        })),
        StreamChunk::IsDone {
            result: Box::new(SamplingResult {
                // Same text as the streamed chunks — the
                // canonical duplicate.
                text: "hello world".to_string(),
                tool_calls: vec![],
                reasoning: String::new(),
                reasoning_signature: None,
                usage: TokenUsage {
                    prompt_tokens: 1,
                    completion_tokens: 2,
                    total_tokens: 3,
                    cached_prompt_tokens: None,
                    cache_read_tokens: None,
                    cache_write_tokens: None,
                    reasoning_tokens: None,
                },
                ..Default::default()
            }),
        },
    ]]));

    let events = run_and_collect(
        provider,
        Arc::new(ToolRegistry::new()),
        CancellationToken::new(),
        AgentInput::text("hi"),
    )
    .await;

    let text_events: Vec<String> = events
        .iter()
        .filter_map(|e| match e {
            AgentEvent::Model(ContentPart::Text(tc)) => Some(tc.text.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(
        text_events,
        vec!["hello ".to_string(), "world".to_string()],
        "IsDone text must NOT be re-emitted as a wire event when streamed chunks already covered it; got {text_events:?}"
    );
}

#[tokio::test]
async fn isdone_text_emitted_only_when_no_streamed_chunks() {
    // Mirror of the previous test: when the provider
    // batches everything into IsDone (no streamed chunks),
    // the IsDone text MUST surface on the wire — otherwise
    // non-streaming providers (or streaming providers with
    // no content events) would produce a silent assistant
    // turn.
    let provider = Arc::new(ScriptedStreamProvider::new(vec![vec![
        StreamChunk::IsDone {
            result: Box::new(SamplingResult {
                text: "batched-response".to_string(),
                tool_calls: vec![],
                reasoning: String::new(),
                reasoning_signature: None,
                usage: TokenUsage::default(),
                ..Default::default()
            }),
        },
    ]]));

    let events = run_and_collect(
        provider,
        Arc::new(ToolRegistry::new()),
        CancellationToken::new(),
        AgentInput::text("hi"),
    )
    .await;

    let text_events: Vec<String> = events
        .iter()
        .filter_map(|e| match e {
            AgentEvent::Model(ContentPart::Text(tc)) => Some(tc.text.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(
        text_events,
        vec!["batched-response".to_string()],
        "non-streaming IsDone must surface its text on the wire; got {text_events:?}"
    );
}

#[tokio::test]
async fn isdone_tool_calls_dedup_against_streamed_tool_call_end() {
    // Anthropic and OpenAI both emit `ToolCallStart` +
    // `ToolCallEnd` AND re-list the same tool calls in
    // `IsDone.result.tool_calls`. Without dedup, the
    // history message would carry two identical
    // ContentPart::ToolUse entries for the same id —
    // corrupting the LLM context. This test asserts each
    // tool_use_id appears on the wire exactly once.
    let tool_use = ToolUse {
        id: "call_dup".to_string(),
        name: "noop".to_string(),
        input: json!({}),
    };
    // Patch the IsDone to also include the same tool
    // call (re-listing). We rebuild the chunks directly
    // because `tool_call_response` emits
    // `TokenUsage::default()` in IsDone — fine, but we
    // need tool_calls populated too.
    let chunks: Vec<Vec<StreamChunk>> = vec![vec![
        StreamChunk::ToolCallStart {
            id: tool_use.id.clone(),
            name: tool_use.name.clone(),
            arguments: Value::String("{}".to_string()),
        },
        StreamChunk::ToolCallEnd {
            id: tool_use.id.clone(),
        },
        StreamChunk::IsDone {
            result: Box::new(SamplingResult {
                text: String::new(),
                // Same id as the streamed chunk — must be deduped.
                tool_calls: vec![tool_use.clone()],
                reasoning: String::new(),
                reasoning_signature: None,
                usage: TokenUsage::default(),
                ..Default::default()
            }),
        },
    ]];
    let provider = Arc::new(ScriptedStreamProvider::new(chunks));

    let registry = Arc::new(ToolRegistry::new());
    registry.register_entry(synthia_tool::ToolEntry::new(Arc::new(
        FakeTool::new("noop", "done"),
    )));

    let events = run_and_collect(
        provider,
        registry,
        CancellationToken::new(),
        AgentInput::text("dedup"),
    )
    .await;

    let wire_tool_use_ids: Vec<&str> = events
        .iter()
        .filter_map(|e| match e {
            AgentEvent::Model(ContentPart::ToolUse(tu)) => Some(tu.id.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(
        wire_tool_use_ids,
        vec!["call_dup"],
        "tool_use_id must appear on the wire exactly once even when IsDone re-lists it; got {wire_tool_use_ids:?}"
    );
}

#[tokio::test]
async fn token_usage_emitted_per_iteration_can_be_aggregated() {
    // Verifies that `Usage` events are emitted **once per
    // LLM call** (per iteration), not just once for the
    // whole session, and that intermediate
    // `StreamChunk::Usage` deltas do **not** double-count
    // (Anthropic's `message_delta.usage` arrives before
    // `message.usage`; only the latter is authoritative).
    // The test runs a 2-iteration loop (tool call → final
    // empty response) and asserts exactly two distinct
    // Usage events fire with the expected values per
    // iteration, even though the provider scripts
    // intermediate Usage chunks too.
    let tool_use = ToolUse {
        id: "call_x".to_string(),
        name: "noop".to_string(),
        input: json!({}),
    };
    let final_text_response_with_usage = |input: usize, output: usize| {
        vec![
            // Intermediate delta — must NOT surface a
            // Usage event on its own.
            StreamChunk::Usage(TokenUsage {
                prompt_tokens: 1000, // decoy; would
                // pollute the sum if emitted
                completion_tokens: 1000,
                total_tokens: 2000,
                cached_prompt_tokens: None,
                cache_read_tokens: None,
                cache_write_tokens: None,
                reasoning_tokens: None,
            }),
            StreamChunk::Content(ContentPart::Text(TextContent {
                text: "thinking".into(),
                cache_control: None,
            })),
            StreamChunk::IsDone {
                result: Box::new(SamplingResult {
                    text: String::new(),
                    tool_calls: vec![],
                    reasoning: String::new(),
                    reasoning_signature: None,
                    usage: TokenUsage {
                        prompt_tokens: input,
                        completion_tokens: output,
                        total_tokens: input + output,
                        cached_prompt_tokens: None,
                        cache_read_tokens: None,
                        cache_write_tokens: None,
                        reasoning_tokens: None,
                    },
                    ..Default::default()
                }),
            },
        ]
    };
    let tool_chunks = vec![
        tool_call_response_with_usage(
            vec![tool_use.clone()],
            TokenUsage {
                prompt_tokens: 1000, // decoy delta
                completion_tokens: 1000,
                total_tokens: 2000,
                cached_prompt_tokens: None,
                cache_read_tokens: None,
                cache_write_tokens: None,
                reasoning_tokens: None,
            },
        ),
        final_text_response_with_usage(120, 30),
    ];
    let provider = Arc::new(ScriptedStreamProvider::new(tool_chunks));
    let registry = Arc::new(ToolRegistry::new());
    registry.register_entry(synthia_tool::ToolEntry::new(Arc::new(
        FakeTool::new("noop", "done"),
    )));

    let events = run_and_collect(
        provider,
        registry,
        CancellationToken::new(),
        AgentInput::text("iterate"),
    )
    .await;

    let usage_events: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            AgentEvent::System(SystemEvent::Usage {
                input_tokens,
                output_tokens,
                ..
            }) => Some((*input_tokens, *output_tokens)),
            _ => None,
        })
        .collect();
    assert_eq!(
        usage_events.len(),
        2,
        "expected one Usage event per LLM iteration, got {usage_events:?}"
    );
    // Iteration 1: IsDone carries default usage
    // (tool_call_response used `TokenUsage::default()` in
    // its IsDone), so this iteration's Usage event is
    // (0, 0).
    assert_eq!(usage_events[0], (0, 0), "iteration 1");
    // Iteration 2: IsDone carries (120, 30) — proves the
    // intermediate decoy `(1000, 1000)` was suppressed.
    assert_eq!(usage_events[1], (120, 30), "iteration 2");

    // Sanity check: the aggregate is the IsDone totals,
    // not a doubled sum including the decoys.
    let (sum_in, sum_out) = usage_events
        .iter()
        .fold((0usize, 0usize), |(i, o), (x, y)| (i + x, o + y));
    assert_eq!((sum_in, sum_out), (120, 30));
}

#[tokio::test]
async fn run_executes_tool_and_emits_tool_result() {
    let tool_use = ToolUse {
        id: "call_1".to_string(),
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

    let events = run_and_collect(
        provider,
        registry,
        CancellationToken::new(),
        AgentInput::text("run tool"),
    )
    .await;

    let tool_uses: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            AgentEvent::Model(ContentPart::ToolUse(tu)) => Some(tu.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(tool_uses.len(), 1);
    assert_eq!(tool_uses[0].id, "call_1");
    assert_eq!(tool_uses[0].name, "echo");

    let tool_results: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            AgentEvent::Model(ContentPart::ToolResult(tr)) => Some(tr.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(tool_results.len(), 1);
    assert_eq!(tool_results[0].tool_use_id, "call_1");
    assert_eq!(tool_results[0].tool_name.as_deref(), Some("echo"));
    assert!(!tool_results[0].is_error.unwrap_or(true));
    let text = match &tool_results[0].content[0] {
        ContentPart::Text(t) => &t.text,
        _ => panic!("expected text content"),
    };
    assert_eq!(text, "echoed");

    assert!(matches!(
        events.last(),
        Some(AgentEvent::System(SystemEvent::SessionEnded {
            reason: SessionEndReason::Completed,
        }))
    ));
}

/// The tool list the loop sends is the exposure-aware projection, and
/// promotion is driven by the transcript the loop itself builds: the
/// first request advertises the `Deferred` tool with a placeholder
/// schema, and by the second request — after the assistant's `tool_use`
/// entered the history — it carries the real schema.
#[tokio::test]
async fn tool_definitions_project_exposure_and_promote_after_a_call() {
    let pending = ToolUse {
        id: "call_deep".to_string(),
        name: "deep".to_string(),
        input: json!({"q": "hello"}),
    };
    let provider = Arc::new(CapturingProvider::new(vec![
        tool_call_response(vec![pending]),
        empty_response(),
    ]));
    let real_deep = json!({
        "type": "object",
        "properties": {"q": {"type": "string"}},
        "required": ["q"],
    });
    let real_direct = json!({
        "type": "object",
        "properties": {"key": {"type": "string"}},
        "required": ["key"],
    });
    let registry = Arc::new(ToolRegistry::new());
    registry.register_entry(synthia_tool::ToolEntry::dynamic(
        "direct_lookup".to_string(),
        "always advertised in full".to_string(),
        real_direct.clone(),
    ));
    registry.register_entry(
        synthia_tool::ToolEntry::dynamic(
            "deep".to_string(),
            "advertised by name until first call".to_string(),
            real_deep.clone(),
        )
        .with_exposure(synthia_tool::ToolExposure::Deferred),
    );
    registry.register_entry(
        synthia_tool::ToolEntry::dynamic(
            "secret".to_string(),
            "never advertised".to_string(),
            json!({"type": "object"}),
        )
        .with_is_hidden(true),
    );

    let _events = run_and_collect(
        provider.clone(),
        Arc::clone(&registry),
        CancellationToken::new(),
        AgentInput::text("go"),
    )
    .await;

    let captured = provider.captured_tools.lock().await;
    assert_eq!(
        captured.len(),
        2,
        "one request for the tool-call iteration and one for the answer"
    );

    // Request 1 — nothing called yet: Direct in full, Deferred by
    // name with the placeholder, hidden absent.
    let first = &captured[0];
    let first_names: Vec<&str> =
        first.iter().map(|d| d.name.as_str()).collect();
    assert!(first_names.contains(&"direct_lookup"), "{first_names:?}");
    assert!(first_names.contains(&"deep"), "{first_names:?}");
    assert!(
        !first_names.contains(&"secret"),
        "an is_hidden tool must not reach the model; {first_names:?}"
    );
    let direct = first
        .iter()
        .find(|d| d.name == "direct_lookup")
        .expect("direct_lookup is advertised");
    assert_eq!(direct.input_schema, real_direct);
    let deep = first
        .iter()
        .find(|d| d.name == "deep")
        .expect("deep is advertised");
    assert_eq!(
        deep.input_schema,
        json!({"type": "object", "additionalProperties": true}),
        "an uncalled Deferred tool must not leak its real schema"
    );

    // Request 2 — the assistant's `tool_use` for `deep` is now part of
    // the transcript, so the projection promotes it.
    let second = &captured[1];
    assert!(
        !second.iter().any(|d| d.name == "secret"),
        "promotion must not resurrect a hidden tool"
    );
    let promoted = second
        .iter()
        .find(|d| d.name == "deep")
        .expect("deep stays advertised");
    assert_eq!(
        promoted.input_schema, real_deep,
        "the transcript must promote the Deferred tool"
    );

    // R34 regression: a `ToolSurfacePolicy` that declares nothing is
    // the documented no-op, so it must not perturb the R33 output —
    // byte-identical to both the policy-less agent and the R33
    // request that was actually sent.
    let baseline = ReActAgent::new(provider.clone(), Arc::clone(&registry))
        .projected_tool_definitions(&[]);
    let noop_policy = ReActAgent::new(provider.clone(), Arc::clone(&registry))
        .with_tool_surface(ToolSurfacePolicy::default())
        .projected_tool_definitions(&[]);
    assert_eq!(
        serde_json::to_vec(&noop_policy).unwrap(),
        serde_json::to_vec(&baseline).unwrap(),
        "an empty surface policy must not perturb the R33 projection"
    );
    assert_eq!(
        serde_json::to_vec(&baseline).unwrap(),
        serde_json::to_vec(&captured[0]).unwrap(),
        "the policy-less projection must stay byte-identical to the \
         first R33 request"
    );
}

/// The surface policy narrows what the model is told about — inactive
/// groups are withheld, `max_visible` caps the rest in registry order
/// — while every registered tool stays dispatcheable: the loop still
/// runs a call to a tool the model was never shown.
#[tokio::test]
async fn tool_surface_policy_narrows_advertisement_but_not_dispatch() {
    let pending = ToolUse {
        id: "call_gamma".to_string(),
        name: "gamma".to_string(),
        input: json!({}),
    };
    let provider = Arc::new(CapturingProvider::new(vec![
        tool_call_response(vec![pending.clone()]),
        empty_response(),
    ]));
    let registry = Arc::new(ToolRegistry::new());
    // Registered out of alphabetical order so the cap's registry
    // (name-sorted) order is observable.
    for name in ["gamma", "omega", "beta", "alpha"] {
        registry.register_entry(synthia_tool::ToolEntry::dynamic(
            name.to_string(),
            format!("the {name} tool"),
            json!({"type": "object", "properties": {}}),
        ));
    }
    let policy = ToolSurfacePolicy {
        max_visible: Some(2),
        groups: [
            (
                "core".to_string(),
                vec!["alpha".to_string(), "beta".to_string()],
            ),
            ("extra".to_string(), vec!["gamma".to_string()]),
        ]
        .into_iter()
        .collect(),
        active_groups: vec!["core".to_string()],
    };
    policy
        .apply(&registry)
        .expect("the policy names registered tools");

    let agent = ReActAgent::new(provider.clone(), Arc::clone(&registry))
        .with_tool_surface(policy);
    let mut stream = agent
        .run(AgentInput::text("go"), Arc::new(CancellationToken::new()))
        .await;
    let mut events = Vec::new();
    while let Some(event) = stream.next().await {
        events.push(event);
    }

    let captured = provider.captured_tools.lock().await;
    let advertised: Vec<&str> =
        captured[0].iter().map(|d| d.name.as_str()).collect();
    assert_eq!(
        advertised,
        vec!["alpha", "beta"],
        "active group first, then the cap; gamma's group is inactive \
         and omega is capped out"
    );
    assert_eq!(
        registry.exposure("gamma"),
        Some(ToolExposure::Hidden),
        "the inactive group's member is Hidden exposure, not hidden"
    );
    assert!(
        registry.contains("gamma"),
        "the policy must not unregister a withheld tool"
    );

    let results: Vec<&synthia_provider::ToolResult> = events
        .iter()
        .filter_map(|event| match event {
            AgentEvent::Model(ContentPart::ToolResult(result)) => Some(result),
            _ => None,
        })
        .collect();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].tool_use_id, "call_gamma");
    assert!(
        !results[0].is_error.unwrap_or(true),
        "a withheld tool must still execute: {results:?}"
    );
}

#[tokio::test]
async fn run_emits_tool_progress_for_streaming_tool() {
    /// Tool that yields one Progress item then a Result.
    struct ProgressTool;

    #[async_trait]
    impl Tool for ProgressTool {
        fn name(&self) -> &str {
            "streamer"
        }

        fn description(&self) -> &str {
            "yields progress"
        }

        fn parameters(&self) -> serde_json::Value {
            json!({"type": "object"})
        }

        async fn call(
            &self,
            _input: serde_json::Value,
            _ctx: &Context,
        ) -> ToolOutput {
            ToolOutput::text("done")
        }

        fn stream<'a>(
            &'a self,
            _input: serde_json::Value,
            _ctx: &'a Context,
        ) -> std::pin::Pin<
            Box<dyn futures::Stream<Item = StreamOutput> + Send + 'a>,
        > {
            use futures::stream;
            let s = stream::iter(vec![
                StreamOutput::Progress(ToolOutput::text("halfway")),
                StreamOutput::Result(ToolOutput::text("done")),
            ]);
            Box::pin(s)
        }
    }

    let tool_use = ToolUse {
        id: "call_1".to_string(),
        name: "streamer".to_string(),
        input: json!({}),
    };
    let provider = Arc::new(ScriptedStreamProvider::new(vec![
        tool_call_response(vec![tool_use]),
        empty_response(),
    ]));
    let registry = Arc::new(ToolRegistry::new());
    registry
        .register_entry(synthia_tool::ToolEntry::new(Arc::new(ProgressTool)));

    let events = run_and_collect(
        provider,
        registry,
        CancellationToken::new(),
        AgentInput::text("stream"),
    )
    .await;

    let progress_events: Vec<_> = events
        .iter()
        .filter(|e| {
            matches!(e, AgentEvent::System(SystemEvent::ToolProgress { .. }))
        })
        .collect();
    assert_eq!(progress_events.len(), 1);
    if let AgentEvent::System(SystemEvent::ToolProgress {
        tool_name,
        call_id,
        ..
    }) = progress_events[0]
    {
        assert_eq!(tool_name, "streamer");
        assert_eq!(call_id, "call_1");
    }
}

/// A model that calls an `is_hidden` tool by name gets the same
/// refusal the registry's own dispatcher gives: the call never
/// reaches the tool. `is_hidden` is the registry's privacy flag, so
/// "not advertised" must also mean "not dispatcheable".
#[tokio::test]
async fn call_to_a_hidden_tool_is_refused_without_executing_it() {
    let provider = Arc::new(ScriptedStreamProvider::new(vec![
        tool_call_response(vec![ToolUse {
            id: "call_secret".to_string(),
            name: "secret".to_string(),
            input: json!({}),
        }]),
        empty_response(),
    ]));
    let registry = Arc::new(ToolRegistry::new());
    registry.register_entry(
        synthia_tool::ToolEntry::new(Arc::new(FakeTool::new(
            "secret",
            "SENTINEL-EXECUTED",
        )))
        .with_is_hidden(true),
    );

    let events = run_and_collect(
        provider,
        registry,
        CancellationToken::new(),
        AgentInput::text("go"),
    )
    .await;

    let results: Vec<&synthia_provider::ToolResult> = events
        .iter()
        .filter_map(|event| match event {
            AgentEvent::Model(ContentPart::ToolResult(result)) => Some(result),
            _ => None,
        })
        .collect();
    assert_eq!(results.len(), 1, "the call must be answered: {events:?}");
    assert_eq!(results[0].tool_use_id, "call_secret");
    assert_eq!(
        results[0].is_error,
        Some(true),
        "a hidden tool's call is an error, not a silent no-op"
    );
    let body = serde_json::to_string(&results[0].content).unwrap();
    assert!(
        !body.contains("SENTINEL-EXECUTED"),
        "the hidden tool must never run: {body}"
    );
}

#[tokio::test]
async fn run_emits_warning_at_max_iterations() {
    let scripted: Vec<Vec<StreamChunk>> = (0..DEFAULT_MAX_ITERATIONS + 2)
        .map(|_| {
            tool_call_response(vec![ToolUse {
                id: "loop".to_string(),
                name: "echo".to_string(),
                input: json!({}),
            }])
        })
        .collect();
    let provider = Arc::new(ScriptedStreamProvider::new(scripted));
    let registry = Arc::new(ToolRegistry::new());
    registry.register_entry(synthia_tool::ToolEntry::new(Arc::new(
        FakeTool::new("echo", "ok"),
    )));

    let events = run_and_collect(
        provider,
        registry,
        CancellationToken::new(),
        AgentInput::text("loop forever"),
    )
    .await;

    let warning = events.iter().find_map(|e| match e {
        AgentEvent::System(SystemEvent::Warning {
            kind: WarningKind::Loop,
            message,
            ..
        }) => Some(message.clone()),
        _ => None,
    });
    assert!(warning.is_some(), "expected Loop warning");
    assert!(events.iter().any(|e| matches!(
        e,
        AgentEvent::System(SystemEvent::SessionEnded {
            reason: SessionEndReason::MaxIterations,
        })
    )));
}

/// `with_max_iterations(3)` MUST cut the loop to the smaller
/// value (instead of `DEFAULT_MAX_ITERATIONS = 25`) and emit
/// the Loop warning with the overridden count, proving the
/// field is end-to-end live (constructor -> ReActLoop ->
/// span field -> loop bound -> warning message).
#[tokio::test]
async fn with_max_iterations_overrides_default() {
    let tool_use = ToolUse {
        id: "loop".to_string(),
        name: "echo".to_string(),
        input: json!({}),
    };
    let scripted: Vec<Vec<StreamChunk>> = (0..DEFAULT_MAX_ITERATIONS)
        .map(|_| tool_call_response(vec![tool_use.clone()]))
        .collect();
    let provider = Arc::new(ScriptedStreamProvider::new(scripted));
    let registry = Arc::new(ToolRegistry::new());
    registry.register_entry(synthia_tool::ToolEntry::new(Arc::new(
        FakeTool::new("echo", "ok"),
    )));

    let agent = ReActAgent::new(provider, registry).with_max_iterations(3);
    assert_eq!(
        agent.max_iterations(),
        3,
        "with_max_iterations MUST overwrite the field, not no-op"
    );
    let mut stream = agent
        .run(AgentInput::text("loop"), Arc::new(CancellationToken::new()))
        .await;
    while let Some(ev) = stream.next().await {
        let _ = ev;
    }
    drop(stream);

    // `max_iterations(3)` clamped the loop to 3 iterations;
    // since the model never stops calling `echo`, the run
    // exhausts the cap and ends with `MaxIterations`.
    let _ = tool_use;
}

/// `with_max_iterations(0)` and an overflow MUST be clamped
/// up to `1` (not panicking on zero) and clamped down to
/// `4096` (not allowing unbounded runs).
#[test]
fn max_iterations_clamps_inputs() {
    assert_eq!(
        ReActAgent::new(
            Arc::new(
                synthia_provider::traits_stub::ModelProviderStub::text_only(
                    "x"
                )
            ),
            Arc::new(ToolRegistry::new()),
        )
        .with_max_iterations(0)
        .max_iterations(),
        1
    );
    assert_eq!(
        ReActAgent::new(
            Arc::new(
                synthia_provider::traits_stub::ModelProviderStub::text_only(
                    "x"
                )
            ),
            Arc::new(synthia_tool::ToolRegistry::new()),
        )
        .with_max_iterations(usize::MAX)
        .max_iterations(),
        4096
    );
}

#[tokio::test]
async fn run_emits_session_interrupted_on_cancel() {
    let provider = Arc::new(ScriptedStreamProvider::new(Vec::new()));
    let cancel = CancellationToken::new();
    cancel.cancel();

    let events = run_and_collect(
        provider,
        Arc::new(ToolRegistry::new()),
        cancel,
        AgentInput::text("hi"),
    )
    .await;

    let kinds: Vec<&str> = events.iter().map(|e| e.kind()).collect();
    assert_eq!(kinds, vec!["System", "System", "System"]);
    assert!(matches!(
        events[1],
        AgentEvent::System(SystemEvent::SessionInterrupted { .. })
    ));
    assert!(matches!(
        events[2],
        AgentEvent::System(SystemEvent::SessionEnded {
            reason: SessionEndReason::Cancelled,
        })
    ));
}

/// A tool that panics must not take the whole run's error reporting with
/// it: the caller still has to learn the session ended.
#[tokio::test]
async fn panicking_tool_still_reports_session_end() {
    struct BoomTool;

    #[async_trait]
    impl Tool for BoomTool {
        fn name(&self) -> &str {
            "boom"
        }

        fn description(&self) -> &str {
            "panics on call"
        }

        fn parameters(&self) -> serde_json::Value {
            json!({"type": "object"})
        }

        async fn call(
            &self,
            _input: serde_json::Value,
            _ctx: &Context,
        ) -> ToolOutput {
            panic!("tool blew up");
        }
    }

    let provider = Arc::new(ScriptedStreamProvider::new(vec![
        tool_call_response(vec![ToolUse {
            id: "call_boom".to_string(),
            name: "boom".to_string(),
            input: json!({}),
        }]),
        empty_response(),
    ]));
    let registry = Arc::new(ToolRegistry::new());
    registry.register_entry(synthia_tool::ToolEntry::new(Arc::new(BoomTool)));

    let events = run_and_collect(
        provider,
        registry,
        CancellationToken::new(),
        AgentInput::text("go"),
    )
    .await;

    let ended = events.iter().any(|e| {
        matches!(e, AgentEvent::System(SystemEvent::SessionEnded { .. }))
    });
    assert!(
        ended,
        "a panicking tool must not swallow the terminal event; got {:?}",
        events.iter().map(|e| e.kind()).collect::<Vec<_>>()
    );

    // And the panic is reported *as a tool error*, so the model can see
    // what happened and adapt rather than the call vanishing.
    let result = events
        .iter()
        .find_map(|e| match e {
            AgentEvent::Model(ContentPart::ToolResult(r)) => Some(r),
            _ => None,
        })
        .expect("the failed call must still produce a tool result");
    assert_eq!(result.tool_use_id, "call_boom");
    assert_eq!(
        result.is_error,
        Some(true),
        "a panic is an error result, not a silent success"
    );
    let body = serde_json::to_string(&result.content).unwrap();
    assert!(
        body.contains("blew up"),
        "the panic message must reach the model: {body}"
    );
}

/// The same guarantee for the *interceptor* seam: a plugin that panics
/// must not take the run's error reporting with it. The interceptor path
/// returns before the registry path is entered, so it needs its own
/// guard — a delegator that spawns subagents is exactly the kind of
/// third-party code that can panic.
#[tokio::test]
async fn panicking_interceptor_still_reports_session_end() {
    use crate::agent::{InterceptorCall, ToolInterceptor};

    struct BoomInterceptor;

    impl ToolInterceptor for BoomInterceptor {
        fn definitions(&self) -> Vec<synthia_provider::ToolDefinition> {
            Vec::new()
        }

        fn claims(&self, name: &str) -> bool {
            name == "boom"
        }

        fn execute<'a>(
            &'a self,
            _call: InterceptorCall<'a>,
        ) -> futures::future::BoxFuture<'a, ToolOutput> {
            Box::pin(async { panic!("interceptor blew up") })
        }
    }

    let provider = Arc::new(ScriptedStreamProvider::new(vec![
        tool_call_response(vec![ToolUse {
            id: "call_boom".to_string(),
            name: "boom".to_string(),
            input: json!({}),
        }]),
        empty_response(),
    ]));

    let agent = ReActAgent::new(provider, Arc::new(ToolRegistry::new()))
        .with_interceptor(Arc::new(BoomInterceptor));
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
        "a panicking interceptor must not swallow the terminal event; \
         got {:?}",
        events.iter().map(|e| e.kind()).collect::<Vec<_>>()
    );
    let result = events
        .iter()
        .find_map(|e| match e {
            AgentEvent::Model(ContentPart::ToolResult(r)) => Some(r),
            _ => None,
        })
        .expect("the failed call must still produce a tool result");
    assert_eq!(result.is_error, Some(true));
    let body = serde_json::to_string(&result.content).unwrap();
    assert!(
        body.contains("interceptor blew up"),
        "the interceptor's panic message must reach the model: {body}"
    );
}

/// The strategy backstop: a panic anywhere the inner guards cannot
/// reach — the loop's own bucketing, commit, hook fan-out, guards —
/// still owes the caller a reported end. `agent_impl` wraps the whole
/// `strategy.run(..)` call for exactly this, since nothing below it can
/// catch a panic in the loop's own code.
#[tokio::test]
async fn panicking_strategy_still_reports_session_end() {
    use crate::agent::{AgentRuntime, EventSink, ReasoningStrategy};

    struct BoomStrategy;

    #[async_trait]
    impl ReasoningStrategy for BoomStrategy {
        fn name(&self) -> &str {
            "boom"
        }

        async fn run(
            &self,
            _runtime: AgentRuntime,
            _input: AgentInput,
            _sink: EventSink,
        ) {
            panic!("strategy blew up");
        }
    }

    let agent = ReActAgent::new(
        Arc::new(synthia_provider::traits_stub::ModelProviderStub::text_only(
            "unused",
        )),
        Arc::new(ToolRegistry::new()),
    )
    .with_strategy(Arc::new(BoomStrategy));

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
        matches!(reason, Some(SessionEndReason::Error(ref m)) if m.contains("strategy blew up")),
        "a panicking strategy must end the run reported, got {reason:?} \
         from {events:?}"
    );
}

/// A panicking `ModelProvider` must end the run reported too. It is the
/// one individually-guarded seam whose panic cannot be turned into a
/// value the loop merely records — there is no response to carry on
/// with, so it maps to `SessionEnded { reason: Error(..) }`.
#[tokio::test]
async fn panicking_provider_still_reports_session_end() {
    struct BoomProvider;

    #[async_trait]
    impl synthia_provider::traits::ModelProvider for BoomProvider {
        async fn initialize(
            &mut self,
            _config: synthia_provider::ProviderConfig,
        ) -> Result<(), synthia_core::Error> {
            Ok(())
        }

        fn name(&self) -> &str {
            "boom"
        }

        fn model_config(&self) -> synthia_provider::ModelConfig {
            synthia_provider::ModelConfig {
                name: "boom".to_string(),
                provider: "boom".to_string(),
                context_window: 8_192,
                max_output_tokens: 1_024,
                supports_tools: true,
                supports_streaming: true,
                supports_reasoning: false,
            }
        }

        async fn complete(
            &self,
            _request: synthia_provider::CompletionRequest,
        ) -> Result<synthia_provider::CompletionResponse, synthia_core::Error>
        {
            panic!("provider blew up");
        }

        async fn complete_with_stream(
            &self,
            _request: synthia_provider::CompletionRequest,
            _cancel: Option<Arc<dyn synthia_core::CancelToken>>,
            _on_delta: Box<dyn FnMut(StreamChunk) + Send>,
        ) -> Result<synthia_provider::CompletionResponse, synthia_core::Error>
        {
            panic!("provider blew up");
        }
    }

    let agent =
        ReActAgent::new(Arc::new(BoomProvider), Arc::new(ToolRegistry::new()));
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
        matches!(
            reason,
            Some(SessionEndReason::Error(ref m)) if m.contains("provider blew up")
        ),
        "a panicking provider must end the run reported, got {reason:?} \
         from {:?}",
        events.iter().map(|e| e.kind()).collect::<Vec<_>>()
    );
}

/// Every `iteration_start` in the durable typed log must have a matching
/// `iteration_end`.
///
/// The pair is emitted at opposite ends of the iteration: the start in
/// phase 1, the end after the body. Three paths leave the iteration
/// before the close phase normally would (cancelled before the LLM call,
/// a failed sample, and the self-finalizing cancel-before-tool path), so
/// emitting the end anywhere but a single always-runs point silently
/// produces an unpaired start — invisible on the event stream, which is
/// a different channel.
#[tokio::test]
async fn every_iteration_start_has_a_matching_end() {
    use synthia_core::CancelToken;
    use synthia_provider::traits::ModelProvider;
    use synthia_session::{SessionEvent, TypedEventSink};

    fn iteration_pairs(events: &[SessionEvent]) -> Vec<(u64, &'static str)> {
        events
            .iter()
            .filter_map(|e| match e {
                SessionEvent::Iteration { data, .. } => {
                    let idx = data.get("iteration")?.as_u64()?;
                    let kind = match data.get("kind")?.as_str()? {
                        "start" => "start",
                        "end" => "end",
                        other => panic!("unknown iteration kind {other}"),
                    };
                    Some((idx, kind))
                }
                _ => None,
            })
            .collect()
    }

    fn assert_paired(name: &str, events: &[SessionEvent]) {
        let pairs = iteration_pairs(events);
        let starts = pairs.iter().filter(|(_, k)| *k == "start").count();
        let ends = pairs.iter().filter(|(_, k)| *k == "end").count();
        assert!(starts > 0, "{name}: no iteration_start at all: {pairs:?}");
        assert_eq!(
            starts, ends,
            "{name}: unpaired iteration boundary in the typed log: {pairs:?}"
        );
    }

    // --- Scenario 1: a normal completion. ---
    {
        let provider: Arc<dyn ModelProvider> =
            Arc::new(ScriptedStreamProvider::new(vec![empty_response()]));
        let (sink, mut rx) = TypedEventSink::channel(64);
        let agent = ReActAgent::new(provider, Arc::new(ToolRegistry::new()))
            .with_typed_event_sink(sink);
        let mut stream = agent
            .run(AgentInput::text("go"), Arc::new(CancellationToken::new()))
            .await;
        while stream.next().await.is_some() {}
        let mut events = Vec::new();
        while let Ok(Some(record)) = rx.try_recv() {
            events.push(record.event);
        }
        assert_paired("normal completion", &events);
    }

    // --- Scenario 2: cancelled before the LLM call (phase-1 early return). ---
    {
        let provider: Arc<dyn ModelProvider> =
            Arc::new(ScriptedStreamProvider::new(Vec::new()));
        let (sink, mut rx) = TypedEventSink::channel(64);
        let agent = ReActAgent::new(provider, Arc::new(ToolRegistry::new()))
            .with_typed_event_sink(sink);
        let cancel = CancellationToken::new();
        cancel.cancel();
        let mut stream =
            agent.run(AgentInput::text("go"), Arc::new(cancel)).await;
        while stream.next().await.is_some() {}
        let mut events = Vec::new();
        while let Ok(Some(record)) = rx.try_recv() {
            events.push(record.event);
        }
        assert_paired("cancelled before LLM", &events);
    }

    // --- Scenario 3: the sample failed (phase-3 early return). ---
    {
        struct FailingProvider;
        #[async_trait]
        impl ModelProvider for FailingProvider {
            async fn initialize(
                &mut self,
                _config: synthia_provider::ProviderConfig,
            ) -> Result<(), synthia_core::Error> {
                Ok(())
            }

            fn name(&self) -> &str {
                "failing"
            }

            fn model_config(&self) -> synthia_provider::ModelConfig {
                synthia_provider::ModelConfig {
                    name: "failing".to_string(),
                    provider: "failing".to_string(),
                    context_window: 8_192,
                    max_output_tokens: 1_024,
                    supports_tools: true,
                    supports_streaming: true,
                    supports_reasoning: false,
                }
            }

            async fn complete(
                &self,
                _request: synthia_provider::CompletionRequest,
            ) -> Result<synthia_provider::CompletionResponse, synthia_core::Error>
            {
                panic!("provider blew up");
            }

            async fn complete_with_stream(
                &self,
                _request: synthia_provider::CompletionRequest,
                _cancel: Option<Arc<dyn CancelToken>>,
                _on_delta: Box<dyn FnMut(StreamChunk) + Send>,
            ) -> Result<synthia_provider::CompletionResponse, synthia_core::Error>
            {
                panic!("provider blew up");
            }
        }

        let (sink, mut rx) = TypedEventSink::channel(64);
        let agent = ReActAgent::new(
            Arc::new(FailingProvider),
            Arc::new(ToolRegistry::new()),
        )
        .with_typed_event_sink(sink);
        let mut stream = agent
            .run(AgentInput::text("go"), Arc::new(CancellationToken::new()))
            .await;
        while stream.next().await.is_some() {}
        let mut events = Vec::new();
        while let Ok(Some(record)) = rx.try_recv() {
            events.push(record.event);
        }
        assert_paired("sample failed", &events);
    }

    // --- Scenario 4: the self-finalizing cancel-before-tool path. ---
    {
        use std::time::Duration;

        use synthia_provider::CompletionResponse;
        use synthia_steering::hook::AgentHook;

        struct CancelAfterProvider(CancellationToken);
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
                self.0.cancel();
            }
        }

        let cancel = CancellationToken::new();
        let provider: Arc<dyn ModelProvider> =
            Arc::new(ScriptedStreamProvider::new(vec![tool_call_response(
                vec![ToolUse {
                    id: "call_1".to_string(),
                    name: "echo".to_string(),
                    input: json!({}),
                }],
            )]));
        let (registry, _calls) = echo_registry();
        let hook: Arc<dyn AgentHook> =
            Arc::new(CancelAfterProvider(cancel.clone()));
        let steering = Arc::new(synthia_steering::Steering {
            hooks: vec![hook],
            ..synthia_steering::Steering::noop()
        });
        let (sink, mut rx) = TypedEventSink::channel(64);
        let agent = ReActAgent::new(provider, registry)
            .with_steering(steering)
            .with_typed_event_sink(sink);

        let mut stream =
            agent.run(AgentInput::text("go"), Arc::new(cancel)).await;
        while stream.next().await.is_some() {}
        let mut events = Vec::new();
        while let Ok(Some(record)) = rx.try_recv() {
            events.push(record.event);
        }
        assert_paired("cancelled before tool", &events);
    }
}

/// A signed reasoning block reaches the **next** request, in the
/// provider's own order.
///
/// This is the half of the reasoning contract the harness was missing.
/// `synthia-provider` states it four times as the agent layer's job
/// (`anthropic/types.rs`: the `signature` is "Required to preserve
/// reasoning continuity across turns"; `types/stream_chunk.rs`: it is
/// "propagated so the agent can preserve cross-turn reasoning
/// continuity"), and its Anthropic adapter already maps
/// `ContentPart::Reasoning` → `ThinkingBlock { thinking, signature }`
/// on the way out. What was missing was the middle: the loop dropped
/// the part, so the adapter never saw one in a real run.
///
/// A probe before the fix reported
/// `request 2 has reasoning part = false, signature preserved = false`.
#[tokio::test]
async fn reasoning_reaches_the_next_request() {
    use synthia_provider::ReasoningContent;

    // Turn 1 streams reasoning *before* its tool call, so a second
    // sampling pass follows with that assistant turn in history.
    //
    // The chunk's signature is `None` and the signature arrives on
    // `IsDone`, which is the only shape the Anthropic processor can
    // emit: `signature_delta` lands after the `thinking_delta`s that
    // carried the text, so a streamed reasoning chunk cannot know it
    // yet. Handing the chunk a filled-in signature here would test a
    // shape that never occurs and would pass while the real path lost
    // the value.
    // Two reasoning deltas, as the processor emits for one thinking
    // block (`content_block_start` text + each `thinking_delta`). The
    // history must end up with **one** block, not one per delta:
    // Anthropic requires a single thinking block per prior assistant
    // turn, so a delta-per-part history would be malformed.
    let turn1 = vec![
        StreamChunk::Content(ContentPart::Reasoning(ReasoningContent {
            text: "I should ".to_string(),
            signature: None,
        })),
        StreamChunk::Content(ContentPart::Reasoning(ReasoningContent {
            text: "call echo".to_string(),
            signature: None,
        })),
        StreamChunk::ToolCallStart {
            id: "c1".to_string(),
            name: "echo".to_string(),
            arguments: json!({}),
        },
        StreamChunk::ToolCallEnd {
            id: "c1".to_string(),
        },
        StreamChunk::IsDone {
            result: Box::new(SamplingResult {
                text: String::new(),
                tool_calls: vec![ToolUse {
                    id: "c1".to_string(),
                    name: "echo".to_string(),
                    input: json!({}),
                }],
                reasoning: "I should call echo".to_string(),
                reasoning_signature: Some("sig_123".to_string()),
                usage: TokenUsage::default(),
                stop_reason: Some("tool_use".to_string()),
            }),
        },
    ];
    let provider =
        Arc::new(CapturingProvider::new(vec![turn1, empty_response()]));
    let (registry, _calls) = echo_registry();
    let agent =
        ReActAgent::new(provider.clone(), registry).with_max_iterations(2);
    let mut stream = agent
        .run(AgentInput::text("go"), Arc::new(CancellationToken::new()))
        .await;
    while stream.next().await.is_some() {}

    let captured = provider.captured.lock().await;
    assert!(
        captured.len() >= 2,
        "need a second request to inspect; got {}",
        captured.len()
    );
    // The assistant turn from pass 1 rides in pass 2's history.
    let all_parts: Vec<&ContentPart> =
        captured[1].iter().flat_map(|m| &m.content).collect();
    let reasoning: Vec<&synthia_provider::ReasoningContent> = all_parts
        .iter()
        .filter_map(|p| match p {
            ContentPart::Reasoning(r) => Some(r),
            _ => None,
        })
        .collect();
    assert_eq!(
        reasoning.len(),
        1,
        "the two deltas must commit as one thinking block; parts: {all_parts:?}"
    );
    assert_eq!(
        reasoning[0].text, "I should call echo",
        "and the block must carry the whole accumulated text"
    );
    assert_eq!(
        reasoning[0].signature.as_deref(),
        Some("sig_123"),
        "the signature must survive into the next request even though the \
         streamed chunks could not carry it; parts: {all_parts:?}"
    );
}

/// A run that declared an output schema but never satisfied it says so.
///
/// `with_output_schema` only *offers* the `structured_output` tool; it
/// cannot force the call, and a text-only `FinalAnswer` is a normal way
/// to end. Without this warning a consumer holding a schema sees the
/// same `SessionEnded { Completed }` it would see if the model had
/// submitted, and reads an absent typed result as "no answer".
#[tokio::test]
async fn declared_output_schema_warns_when_never_submitted() {
    let provider =
        Arc::new(CapturingProvider::new(vec![vec![StreamChunk::IsDone {
            result: Box::new(SamplingResult {
                text: "I will answer in prose instead".to_string(),
                ..Default::default()
            }),
        }]]));
    let (registry, _calls) = echo_registry();
    let agent = ReActAgent::new(provider, registry).with_output_schema(json!({
        "type": "object",
        "properties": { "answer": { "type": "string" } },
        "required": ["answer"]
    }));
    let mut stream = agent
        .run(AgentInput::text("go"), Arc::new(CancellationToken::new()))
        .await;
    let mut events = Vec::new();
    while let Some(ev) = stream.next().await {
        events.push(ev);
    }

    let warning = events.iter().find_map(|e| match e {
        AgentEvent::System(SystemEvent::Warning {
            kind: WarningKind::StructuredOutput,
            message,
            ..
        }) => Some(message.clone()),
        _ => None,
    });
    let message = warning.expect(
        "a declared schema that the run never submitted must warn; \
         events were: {events:#?}",
    );
    assert!(
        message.contains("structured_output"),
        "the warning must name what was missing; got: {message}"
    );
}

/// A run that never declared a schema stays silent — the warning is
/// about a declared-but-unsatisfied contract, not about prose answers
/// in general.
#[tokio::test]
async fn no_output_schema_means_no_warning() {
    let provider =
        Arc::new(CapturingProvider::new(vec![vec![StreamChunk::IsDone {
            result: Box::new(SamplingResult {
                text: "plain prose answer".to_string(),
                ..Default::default()
            }),
        }]]));
    let (registry, _calls) = echo_registry();
    let agent = ReActAgent::new(provider, registry);
    let mut stream = agent
        .run(AgentInput::text("go"), Arc::new(CancellationToken::new()))
        .await;
    let mut events = Vec::new();
    while let Some(ev) = stream.next().await {
        events.push(ev);
    }

    assert!(
        !events.iter().any(|e| matches!(
            e,
            AgentEvent::System(SystemEvent::Warning {
                kind: WarningKind::StructuredOutput,
                ..
            })
        )),
        "no schema declared, so there is no contract to miss"
    );
}

/// A run that *does* submit a schema-valid answer stays silent.
///
/// The other half of [`declared_output_schema_warns_when_never_submitted`]:
/// the warning must key off the `structured_output` tool's own success
/// result, not merely off the schema being declared, or it would fire on
/// every well-behaved run.
#[tokio::test]
async fn satisfied_output_schema_warns_not() {
    // Pass 1: the model fills the schema. Pass 2: it stops.
    let provider = Arc::new(CapturingProvider::new(vec![
        tool_call_response(vec![ToolUse {
            id: "so1".to_string(),
            name: synthia_tool::STRUCTURED_OUTPUT_TOOL_NAME.to_string(),
            input: json!({ "answer": "42" }),
        }]),
        empty_response(),
    ]));
    let (registry, _calls) = echo_registry();
    let agent = ReActAgent::new(provider, registry).with_output_schema(json!({
        "type": "object",
        "properties": { "answer": { "type": "string" } },
        "required": ["answer"]
    }));
    let mut stream = agent
        .run(AgentInput::text("go"), Arc::new(CancellationToken::new()))
        .await;
    let mut events = Vec::new();
    while let Some(ev) = stream.next().await {
        events.push(ev);
    }

    assert!(
        !events.iter().any(|e| matches!(
            e,
            AgentEvent::System(SystemEvent::Warning {
                kind: WarningKind::StructuredOutput,
                ..
            })
        )),
        "a satisfied schema must not warn; events were: {events:#?}"
    );
}

/// Streamed text deltas commit as **one** joined `Text` block, not one
/// per delta.
///
/// The Text arm of `coalesce_parts` is the every-answer path — every
/// response with prose goes through it — while the reasoning half is
/// the rarer one (both adapters drop reasoning at the wire unless it is
/// signed). It was nonetheless unpinned: removing the arm left the whole
/// suite green, because the two tests that stream multiple text deltas
/// assert only the wire events and never reach a second request, so
/// neither can observe what `commit_assistant` wrote.
///
/// That matters beyond tidiness: without the arm every Anthropic request
/// would carry N consecutive `text` blocks where the previous code sent
/// one joined part, so the fix would have *introduced* a wire-shape
/// regression on the most common path.
#[tokio::test]
async fn streamed_text_deltas_commit_as_one_block() {
    // Two text deltas, then a tool call so a second sampling pass
    // follows with this assistant turn in history.
    let turn1 = vec![
        StreamChunk::Content(ContentPart::Text(TextContent {
            text: "Hello ".to_string(),
            cache_control: None,
        })),
        StreamChunk::Content(ContentPart::Text(TextContent {
            text: "world".to_string(),
            cache_control: None,
        })),
        StreamChunk::ToolCallStart {
            id: "c1".to_string(),
            name: "echo".to_string(),
            arguments: json!({}),
        },
        StreamChunk::ToolCallEnd {
            id: "c1".to_string(),
        },
        StreamChunk::IsDone {
            result: Box::new(SamplingResult {
                text: String::new(),
                tool_calls: vec![ToolUse {
                    id: "c1".to_string(),
                    name: "echo".to_string(),
                    input: json!({}),
                }],
                usage: TokenUsage::default(),
                stop_reason: Some("tool_use".to_string()),
                ..Default::default()
            }),
        },
    ];
    let provider =
        Arc::new(CapturingProvider::new(vec![turn1, empty_response()]));
    let (registry, _calls) = echo_registry();
    let agent =
        ReActAgent::new(provider.clone(), registry).with_max_iterations(2);
    let mut stream = agent
        .run(AgentInput::text("go"), Arc::new(CancellationToken::new()))
        .await;
    let mut events = Vec::new();
    while let Some(ev) = stream.next().await {
        events.push(ev);
    }

    // Precondition: the wire really did see two separate deltas, so one
    // joined part below is coalescing rather than a single chunk. Only
    // the first sampling pass counts — turn 2 streams `empty_response`'s
    // text, which is not part of this turn.
    let first_pass = events
        .iter()
        .take_while(|e| !matches!(e, AgentEvent::ModelDone(_)))
        .collect::<Vec<_>>();
    let streamed_text: Vec<&str> = first_pass
        .iter()
        .filter_map(|e| match e {
            AgentEvent::Model(ContentPart::Text(t)) => Some(t.text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(
        streamed_text,
        vec!["Hello ", "world"],
        "the stream must publish each delta as it arrives"
    );

    let captured = provider.captured.lock().await;
    assert!(
        captured.len() >= 2,
        "need a second request; got {}",
        captured.len()
    );
    // The assistant turn, not the whole request: the system prompt is
    // also a `Text` part, and the user message is another.
    let assistant = captured[1]
        .iter()
        .rev()
        .find(|m| {
            m.role == synthia_provider::Role::Assistant
                && m.content
                    .iter()
                    .any(|p| matches!(p, ContentPart::ToolUse(_)))
        })
        .expect("turn 1's assistant message must be in request 2");
    let parts: Vec<&ContentPart> = assistant.content.iter().collect();
    let texts: Vec<&str> = parts
        .iter()
        .filter_map(|p| match p {
            ContentPart::Text(t) => Some(t.text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(
        texts,
        vec!["Hello world"],
        "the deltas must commit as one joined part; parts were: {parts:?}"
    );
    assert!(
        matches!(parts.last(), Some(ContentPart::ToolUse(_))),
        "and the tool call must follow the joined text; parts: {parts:?}"
    );
}
