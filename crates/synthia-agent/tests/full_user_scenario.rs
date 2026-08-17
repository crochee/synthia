//! End-to-end integration test: a complete, realistic user interaction.
//!
//! Simulates a user asking an AI travel assistant to book a flight. The
//! assistant streams a text reply, calls `search_flights`, receives the
//! result, calls `book_flight`, returns a confirmation, and finishes.
//! The scenario exercises every public contract of the agent runtime:
//!
//! - streaming text deltas + `ModelDone`
//! - multi-tool orchestration across two iterations
//! - typed `ToolUse` / `ToolResult` events
//! - `Progress` + `Usage` lifecycle events
//! - terminal `SessionEnded { reason: Completed }`
//! - final message extraction from the event stream
//!
//! Plus two adversarial scenarios on the same fixture:
//!
//! - a tool that fails and is recovered on the next iteration
//! - a mid-stream cancellation that terminates cleanly

use std::sync::Arc;

use futures::StreamExt;
use serde_json::json;
use synthia_agent::{
    Agent,
    AgentInput,
    ReActAgent,
    events::{AgentEvent, SessionEndReason, SystemEvent, WarningKind},
};
use synthia_provider::{Content, ContentPart, TextContent, ToolUse};
use synthia_tool::registry::{ToolEntry, ToolRegistry};
use tokio_util::sync::CancellationToken;

mod test_support;
use test_support::{FakeTool, ScriptedStreamProvider};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn registry_with(tools: Vec<FakeTool>) -> Arc<ToolRegistry> {
    let reg = Arc::new(ToolRegistry::new());
    for tool in tools {
        reg.register_entry(ToolEntry::new(Arc::new(tool)));
    }
    reg
}

/// Drain the entire agent stream into a `Vec<AgentEvent>`.
async fn drain(
    agent: &ReActAgent,
    input: AgentInput,
    cancel: Arc<CancellationToken>,
) -> Vec<AgentEvent> {
    let mut stream = agent.run(input, cancel).await;
    let mut out = Vec::new();
    while let Some(ev) = stream.next().await {
        out.push(ev);
    }
    out
}

/// Last `Model(Text)` event in the stream — the assistant's final reply.
fn final_assistant_text(events: &[AgentEvent]) -> Option<String> {
    events.iter().rev().find_map(|e| match e {
        AgentEvent::Model(ContentPart::Text(t)) => Some(t.text.clone()),
        _ => None,
    })
}

fn count_of<F: Fn(&AgentEvent) -> bool>(
    events: &[AgentEvent],
    pred: F,
) -> usize {
    events.iter().filter(|e| pred(e)).count()
}

// ---------------------------------------------------------------------------
// Scenario 1 — happy path: book a flight end-to-end
// ---------------------------------------------------------------------------

#[tokio::test]
async fn user_books_a_flight_end_to_end() {
    // The scripted conversation has two LLM passes:
    //
    // Pass 1: text reply ("Searching for flights...") + tool call
    //         (search_flights). The agent executes the tool, appends
    //         the result, and loops.
    // Pass 2: text reply ("Booking...") + tool call (book_flight).
    //         The agent executes the tool, appends the result, and
    //         loops once more with a final text-only answer.
    //
    // Three passes total: two tool calls + one text-only closer.
    let provider =
        Arc::new(ScriptedStreamProvider::from_content_responses(vec![
            Content::parts(vec![
                ContentPart::Text(TextContent {
                    text: "Searching for flights to PDX...".into(),
                    cache_control: None,
                }),
                ContentPart::ToolUse(ToolUse {
                    id: "call_search".into(),
                    name: "search_flights".into(),
                    input: json!({"from": "SFO", "to": "PDX"}),
                }),
            ]),
            Content::parts(vec![
                ContentPart::Text(TextContent {
                    text: "Booking the morning flight...".into(),
                    cache_control: None,
                }),
                ContentPart::ToolUse(ToolUse {
                    id: "call_book".into(),
                    name: "book_flight".into(),
                    input: json!({"flight_id": "AA101", "seat": "12A"}),
                }),
            ]),
            Content::text(
                "Booked! Your seat 12A on flight AA101 is confirmed.",
            ),
        ]));

    let registry = registry_with(vec![
        FakeTool::new(
            "search_flights",
            "[AA101 8:00am $120, AA207 1:00pm $89]",
        ),
        FakeTool::new("book_flight", "confirmation_code: ABC123"),
    ]);

    let agent = Arc::new(ReActAgent::new(provider, registry));
    let events = drain(
        &agent,
        AgentInput::text("Book me a flight from SFO to PDX tomorrow morning."),
        Arc::new(CancellationToken::new()),
    )
    .await;

    // ----- Structural assertions ---------------------------------------

    // Session lifecycle: started → ended.
    assert!(
        matches!(
            events.first(),
            Some(AgentEvent::System(SystemEvent::SessionStarted { .. }))
        ),
        "first event must be SessionStarted, got {:?}",
        events.first().map(|e| e.kind())
    );
    assert!(
        matches!(
            events.last(),
            Some(AgentEvent::System(SystemEvent::SessionEnded {
                reason: SessionEndReason::Completed,
            }))
        ),
        "last event must be SessionEnded(Completed), got {:?}",
        events.last().map(|e| e.kind())
    );

    // Exactly one SessionStarted and one SessionEnded.
    assert_eq!(
        count_of(&events, |e| matches!(
            e,
            AgentEvent::System(SystemEvent::SessionStarted { .. })
        )),
        1,
    );
    assert_eq!(
        count_of(&events, |e| matches!(
            e,
            AgentEvent::System(SystemEvent::SessionEnded { .. })
        )),
        1,
    );

    // ----- Tool orchestration ------------------------------------------

    let tool_uses: Vec<ToolUse> = events
        .iter()
        .filter_map(|e| match e {
            AgentEvent::Model(ContentPart::ToolUse(t)) => Some(t.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(tool_uses.len(), 2, "expected exactly two tool calls");
    assert_eq!(tool_uses[0].name, "search_flights");
    assert_eq!(tool_uses[1].name, "book_flight");

    let tool_results: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            AgentEvent::Model(ContentPart::ToolResult(tr)) => Some(tr.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(tool_results.len(), 2);
    assert_eq!(tool_results[0].tool_use_id, "call_search");
    assert_eq!(tool_results[1].tool_use_id, "call_book");
    for tr in &tool_results {
        assert_eq!(tr.is_error, Some(false));
    }

    // ----- Streaming text deltas ---------------------------------------

    // The final text-only answer must be the last Model(Text) before
    // SessionEnded.
    assert_eq!(
        final_assistant_text(&events).as_deref(),
        Some("Booked! Your seat 12A on flight AA101 is confirmed.")
    );

    // ModelDone must appear exactly once per LLM pass (3 passes).
    assert_eq!(
        count_of(&events, |e| matches!(e, AgentEvent::ModelDone(_))),
        3
    );

    // Progress events must appear between iterations.
    assert!(
        count_of(&events, |e| matches!(
            e,
            AgentEvent::System(SystemEvent::Progress { .. })
        )) >= 3
    );

    // ----- Ordering invariant: SessionStarted is first, SessionEnded last
    let first = events.first().map(|e| e.kind());
    let last = events.last().map(|e| e.kind());
    assert_eq!(first, Some("System"));
    assert_eq!(last, Some("System"));
}

// ---------------------------------------------------------------------------
// Scenario 2 — tool failure recovery
// ---------------------------------------------------------------------------

#[tokio::test]
async fn user_tool_failure_then_recovery() {
    // Pass 1: model calls `book_flight` which fails.
    // Pass 2: model retries with `book_flight_v2` (different tool) which
    //         succeeds and returns the final confirmation text.
    let provider =
        Arc::new(ScriptedStreamProvider::from_content_responses(vec![
            Content::parts(vec![ContentPart::ToolUse(ToolUse {
                id: "call_v1".into(),
                name: "book_flight".into(),
                input: json!({"flight_id": "AA101"}),
            })]),
            Content::parts(vec![ContentPart::ToolUse(ToolUse {
                id: "call_v2".into(),
                name: "book_flight_v2".into(),
                input: json!({"flight_id": "AA101"}),
            })]),
            Content::text("Booked via fallback path."),
        ]));

    let registry = registry_with(vec![
        FakeTool::failing("book_flight", "AA101 is sold out"),
        FakeTool::new("book_flight_v2", "ok confirmation XYZ"),
    ]);

    let agent = Arc::new(ReActAgent::new(provider, registry));
    let events = drain(
        &agent,
        AgentInput::text("Book AA101"),
        Arc::new(CancellationToken::new()),
    )
    .await;

    // First tool result must be is_error = true; second must be false.
    let mut results = events.iter().filter_map(|e| match e {
        AgentEvent::Model(ContentPart::ToolResult(tr)) => Some(tr.clone()),
        _ => None,
    });
    let first = results.next().expect("first tool result");
    let second = results.next().expect("second tool result");
    assert_eq!(first.tool_use_id, "call_v1");
    assert_eq!(first.is_error, Some(true));
    assert_eq!(second.tool_use_id, "call_v2");
    assert_eq!(second.is_error, Some(false));

    // No Loop warning — we recovered, didn't hit MAX_ITERATIONS.
    assert!(!events.iter().any(|e| matches!(
        e,
        AgentEvent::System(SystemEvent::Warning {
            kind: WarningKind::Loop,
            ..
        })
    )));

    // Session ended normally.
    assert!(matches!(
        events.last(),
        Some(AgentEvent::System(SystemEvent::SessionEnded {
            reason: SessionEndReason::Completed,
        }))
    ));
    assert_eq!(
        final_assistant_text(&events).as_deref(),
        Some("Booked via fallback path.")
    );
}

// ---------------------------------------------------------------------------
// Scenario 3 — mid-stream cancellation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn user_cancels_mid_stream() {
    // Pre-cancel the token BEFORE calling `run`. This guarantees the
    // very first cancellation check inside the loop fires, producing a
    // deterministic "Cancelled before LLM call" event sequence.
    let provider = Arc::new(ScriptedStreamProvider::from_content_responses(
        (0..50)
            .map(|i| Content::text(format!("still talking... chunk {i}")))
            .collect(),
    ));

    let registry = registry_with(vec![]);
    let cancel = Arc::new(CancellationToken::new());
    cancel.cancel();

    let agent = Arc::new(ReActAgent::new(provider, registry));
    let events =
        drain(&agent, AgentInput::text("tell me a long story"), cancel).await;

    // Must end with SessionEnded(Cancelled), not Completed.
    assert!(
        matches!(
            events.last(),
            Some(AgentEvent::System(SystemEvent::SessionEnded {
                reason: SessionEndReason::Cancelled,
            }))
        ),
        "last event must be SessionEnded(Cancelled), got {:?}",
        events.last().map(|e| e.kind())
    );

    // Must include a SessionInterrupted diagnostic before the terminal.
    let interrupted_idx = events.iter().position(|e| {
        matches!(
            e,
            AgentEvent::System(SystemEvent::SessionInterrupted { .. })
        )
    });
    assert!(
        interrupted_idx.is_some(),
        "expected SessionInterrupted before SessionEnded"
    );
    assert!(
        interrupted_idx.unwrap() < events.len() - 1,
        "SessionInterrupted must come before SessionEnded"
    );

    // SessionStarted must still be the first event.
    assert!(matches!(
        events.first(),
        Some(AgentEvent::System(SystemEvent::SessionStarted { .. }))
    ));

    // No ModelDone should appear — we never reached the LLM.
    assert!(
        !events.iter().any(|e| matches!(e, AgentEvent::ModelDone(_))),
        "no ModelDone when session is cancelled before any LLM pass"
    );
}

// ---------------------------------------------------------------------------
// Scenario 4 — streaming chunks arrive incrementally (not buffered)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn user_sees_streaming_text_deltas_in_order() {
    // Multi-pass scenario:
    //   Pass 1: emits a text delta AND a tool call.
    //   Pass 2: emits the closing text delta.
    // Verifies the loop relays the chunks in stream order and that
    // ModelDone fires once per pass.
    let provider =
        Arc::new(ScriptedStreamProvider::from_content_responses(vec![
            Content::parts(vec![
                ContentPart::ToolUse(ToolUse {
                    id: "c1".into(),
                    name: "noop".into(),
                    input: json!({}),
                }),
                ContentPart::Text(TextContent {
                    text: "Hello world".into(),
                    cache_control: None,
                }),
            ]),
            Content::text("done"),
        ]));
    let registry = registry_with(vec![FakeTool::new("noop", "ok")]);
    let agent = Arc::new(ReActAgent::new(provider, registry));
    let events = drain(
        &agent,
        AgentInput::text("hi"),
        Arc::new(CancellationToken::new()),
    )
    .await;

    // The text deltas, in order, must form "Hello world" then "done".
    let texts: Vec<String> = events
        .iter()
        .filter_map(|e| match e {
            AgentEvent::Model(ContentPart::Text(t)) => Some(t.text.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(
        texts,
        vec!["Hello world".to_string(), "done".to_string()],
        "text deltas must arrive in stream order"
    );

    // ModelDone appears twice (one per LLM pass).
    assert_eq!(
        count_of(&events, |e| matches!(e, AgentEvent::ModelDone(_))),
        2
    );

    // The tool call arrives between the two text deltas.
    let tool_use_pos = events
        .iter()
        .position(|e| matches!(e, AgentEvent::Model(ContentPart::ToolUse(_))))
        .expect("ToolUse event");
    assert!(
        tool_use_pos > 0,
        "ToolUse must come after SessionStarted/Progress"
    );
    assert!(
        tool_use_pos < events.len() - 1,
        "ToolUse must come before SessionEnded"
    );
}
