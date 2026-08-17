//! MVP integration tests for the minimal ReAct loop, exercising
//! the `ReActAgent` Stream API.

use std::sync::Arc;

use futures::StreamExt;
use synthia_agent::{
    Agent,
    AgentInput,
    ReActAgent,
    events::{AgentEvent, SessionEndReason, SystemEvent},
};
use synthia_provider::{Content, ContentPart, TextContent, ToolUse};
use synthia_tool::registry::{ToolEntry, ToolRegistry};
use tokio_util::sync::CancellationToken;

mod test_support;
use test_support::{FakeProvider, FakeTool, make_react_agent};

fn registry_with(tools: Vec<FakeTool>) -> Arc<ToolRegistry> {
    let reg = Arc::new(ToolRegistry::new());
    for tool in tools {
        reg.register_entry(ToolEntry::new(Arc::new(tool)));
    }
    reg
}

fn tool(name: &str, output: &str) -> FakeTool {
    FakeTool::new(name, output)
}

/// Collect the entire stream into a `Vec<AgentEvent>`.
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

/// Extract the final assistant text from the event stream.
///
/// Mirrors the historical `output.final_message` field that the
/// pre-Trait `run()` returned: the last `Model(Text)` event before
/// `SessionEnded(Completed)`.
fn final_message(events: &[AgentEvent]) -> Option<String> {
    events.iter().rev().find_map(|e| match e {
        AgentEvent::Model(ContentPart::Text(t)) => Some(t.text.clone()),
        _ => None,
    })
}

#[tokio::test]
async fn text_only_response_completes_session() {
    let provider = Arc::new(FakeProvider::new(vec!["hello back".into()]));
    let registry = registry_with(vec![tool("weather", "ignored")]);
    let (agent, input, cancel) = make_react_agent(
        provider,
        registry,
        "t1".into(),
        AgentInput::text("hi"),
        CancellationToken::new(),
        None,
    );

    let events = drain(&agent, input, cancel).await;
    assert!(matches!(
        events.first(),
        Some(AgentEvent::System(SystemEvent::SessionStarted { .. }))
    ));
    assert!(matches!(
        events.last(),
        Some(AgentEvent::System(SystemEvent::SessionEnded {
            reason: SessionEndReason::Completed
        }))
    ));
    assert_eq!(final_message(&events).as_deref(), Some("hello back"));
}

#[tokio::test]
async fn tool_call_then_final_answer() {
    let provider = Arc::new(FakeProvider::new_content(vec![
        Content::parts(vec![
            ContentPart::Text(TextContent {
                text: "checking weather".into(),
                cache_control: None,
            }),
            ContentPart::ToolUse(ToolUse {
                id: "call_1".into(),
                name: "weather".into(),
                input: serde_json::json!({"city": "PDX"}),
            }),
        ]),
        Content::text("the weather is sunny"),
    ]));
    let registry = registry_with(vec![tool("weather", "sunny, 72F")]);
    let (agent, input, cancel) = make_react_agent(
        provider,
        registry,
        "t2".into(),
        AgentInput::text("weather?"),
        CancellationToken::new(),
        None,
    );

    let events = drain(&agent, input, cancel).await;
    let tool_calls: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            AgentEvent::Model(ContentPart::ToolUse(t)) => Some(t.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(tool_calls.len(), 1, "expected exactly one ToolUse event");
    assert_eq!(tool_calls[0].name, "weather");

    let tool_results: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            AgentEvent::Model(ContentPart::ToolResult(_)) => Some(()),
            _ => None,
        })
        .collect();
    assert_eq!(tool_results.len(), 1);

    assert!(matches!(
        events.last(),
        Some(AgentEvent::System(SystemEvent::SessionEnded {
            reason: SessionEndReason::Completed
        }))
    ));
    assert_eq!(
        final_message(&events).as_deref(),
        Some("the weather is sunny")
    );
}

#[tokio::test]
async fn cancellation_ends_session() {
    // 100 canned responses so the loop has plenty of work; we cancel
    // immediately so iteration 1 never runs.
    let provider = Arc::new(FakeProvider::new(
        (0..100).map(|i| format!("response {i}")).collect(),
    ));
    let registry = registry_with(vec![]);
    let cancel = Arc::new(CancellationToken::new());
    cancel.cancel();

    let agent = ReActAgent::new(provider, registry);
    let events = drain(&agent, AgentInput::text("hi"), cancel).await;
    assert!(matches!(
        events.last(),
        Some(AgentEvent::System(SystemEvent::SessionEnded {
            reason: SessionEndReason::Cancelled
        }))
    ));
}

#[tokio::test]
async fn max_iterations_terminates_loop() {
    let canned: Vec<Content> = (0..100)
        .map(|_| {
            Content::parts(vec![ContentPart::ToolUse(ToolUse {
                id: "loop".into(),
                name: "loop".into(),
                input: serde_json::json!({}),
            })])
        })
        .collect();
    let provider = Arc::new(FakeProvider::new_content(canned));
    let registry = registry_with(vec![tool("loop", "loop")]);

    let (agent, input, cancel) = make_react_agent(
        provider,
        registry,
        "t4".into(),
        AgentInput::text("loop forever"),
        CancellationToken::new(),
        None,
    );

    let events = drain(&agent, input, cancel).await;
    assert!(matches!(
        events.last(),
        Some(AgentEvent::System(SystemEvent::SessionEnded {
            reason: SessionEndReason::MaxIterations
        }))
    ));
}

#[tokio::test]
async fn tool_error_recorded_as_is_error() {
    let provider = Arc::new(FakeProvider::new_content(vec![
        Content::parts(vec![ContentPart::ToolUse(ToolUse {
            id: "call_bad".into(),
            name: "broken".into(),
            input: serde_json::json!({}),
        })]),
        Content::text("recovered"),
    ]));
    let registry = registry_with(vec![FakeTool::failing("broken", "boom")]);

    let (agent, input, cancel) = make_react_agent(
        provider,
        registry,
        "t5".into(),
        AgentInput::text("call broken"),
        CancellationToken::new(),
        None,
    );

    let events = drain(&agent, input, cancel).await;
    let error_event = events
        .iter()
        .find_map(|e| match e {
            AgentEvent::Model(ContentPart::ToolResult(tr))
                if tr.tool_use_id == "call_bad" =>
            {
                let preview: String = tr
                    .content
                    .iter()
                    .filter_map(|p| match p {
                        ContentPart::Text(t) => Some(t.text.clone()),
                        _ => None,
                    })
                    .collect();
                Some((preview, tr.is_error.unwrap_or(false)))
            }
            _ => None,
        })
        .expect("ToolResult for call_bad");
    assert!(error_event.1, "is_error should be true for failing tool");
    assert_eq!(error_event.0, "boom");
    assert_eq!(final_message(&events).as_deref(), Some("recovered"));
}

#[tokio::test]
async fn sequential_tools_abort_round_on_first_error() {
    // When a Sequential tool returns `is_error=true`, the round
    // must stop and the remaining sequential tools must NOT run.
    // The aborted slots are still surfaced to the LLM as a
    // synthetic "cancelled before producing a result" error so
    // the model sees the entire tool-call batch.

    let broken = FakeTool::failing("first", "nope").sequential();
    let second = tool("second", "should-not-run").sequential();

    let provider = Arc::new(FakeProvider::new_content(vec![
        Content::parts(vec![
            ContentPart::ToolUse(ToolUse {
                id: "call_first".into(),
                name: "first".into(),
                input: serde_json::json!({}),
            }),
            ContentPart::ToolUse(ToolUse {
                id: "call_second".into(),
                name: "second".into(),
                input: serde_json::json!({}),
            }),
        ]),
        Content::text("done after error"),
    ]));
    let registry = registry_with(vec![broken, second]);

    let (agent, input, cancel) = make_react_agent(
        provider,
        registry,
        "abort-test".into(),
        AgentInput::text("call both"),
        CancellationToken::new(),
        None,
    );

    let events = drain(&agent, input, cancel).await;

    // Both tool calls must appear in the wire as ToolResult.
    let tool_results: Vec<&str> = events
        .iter()
        .filter_map(|e| match e {
            AgentEvent::Model(ContentPart::ToolResult(tr)) => {
                Some(tr.tool_use_id.as_str())
            }
            _ => None,
        })
        .collect();
    assert!(
        tool_results.contains(&"call_first"),
        "first tool's error result must reach the wire, got {tool_results:?}"
    );
    assert!(
        tool_results.contains(&"call_second"),
        "second tool must surface a synthetic error so the model sees the full batch, got {tool_results:?}"
    );

    // Find the second tool's synthetic error message.
    let synthetic = events.iter().find_map(|e| match e {
        AgentEvent::Model(ContentPart::ToolResult(tr))
            if tr.tool_use_id == "call_second" =>
        {
            let preview: String = tr
                .content
                .iter()
                .filter_map(|p| match p {
                    ContentPart::Text(t) => Some(t.text.clone()),
                    _ => None,
                })
                .collect();
            Some((preview, tr.is_error.unwrap_or(false)))
        }
        _ => None,
    });
    let (synthetic_text, synthetic_is_error) =
        synthetic.expect("missing synthetic error for call_second");
    assert!(synthetic_is_error, "aborted slot must be flagged is_error");
    // The synthetic message must convey "this tool did not
    // run" without leaking the previous tool's failure
    // details. We accept several equivalent wordings
    // (current wording: "tool did not produce a result";
    // past wording: "tool cancelled before producing a
    // result") so an intentional copy-edit on either side
    // can be reviewed consciously instead of breaking
    // silently.
    assert!(
        synthetic_text.contains("did not produce a result")
            || synthetic_text.contains("before producing")
            || synthetic_text.contains("aborted"),
        "synthetic message should hint that the tool did not run; got: {synthetic_text:?}"
    );

    assert_eq!(final_message(&events).as_deref(), Some("done after error"));
}

/// Regression test for the silent information-loss bug
/// where `ToolOutput::truncated_by` and
/// `ToolOutput::metadata` were dropped when converting
/// to the wire `ToolResult`. The agent loop now forwards
/// these fields through `commit_tool_result` so the LLM,
/// A2A clients, and the frontend can see truncation
/// telemetry without parsing the content stream.
#[tokio::test]
async fn tool_output_metadata_and_truncated_by_reach_the_wire() {
    use serde_json::json;
    use synthia_tool::types::TruncatedBy;

    // Build a tool that attaches both a metadata entry
    // and a `TruncatedBy::SpilledTo` reason.
    let truncating = FakeTool::new("trunc", "short result")
        .with_metadata("byte_count", json!(4096))
        .with_truncated_by(TruncatedBy::SpilledTo {
            path: "/tmp/spill.txt".into(),
        });

    let provider = Arc::new(FakeProvider::new_content(vec![
        Content::parts(vec![ContentPart::ToolUse(ToolUse {
            id: "call_trunc".into(),
            name: "trunc".into(),
            input: json!({}),
        })]),
        Content::text("done"),
    ]));
    let registry = registry_with(vec![truncating]);

    let (agent, input, cancel) = make_react_agent(
        provider,
        registry,
        "trunc-meta-test".into(),
        AgentInput::text("run trunc"),
        CancellationToken::new(),
        None,
    );

    let events = drain(&agent, input, cancel).await;

    let tr = events
        .iter()
        .find_map(|e| match e {
            AgentEvent::Model(ContentPart::ToolResult(tr))
                if tr.tool_use_id == "call_trunc" =>
            {
                Some(tr)
            }
            _ => None,
        })
        .expect("tool result for 'call_trunc' must reach the wire");

    // The `byte_count` metadata entry MUST be present.
    assert_eq!(
        tr.metadata.get("byte_count").and_then(|v| v.as_u64()),
        Some(4096),
        "ToolOutput.metadata must reach the wire ToolResult; got {:?}",
        tr.metadata,
    );

    // The truncation reason MUST be present and have
    // the right `kind` discriminator.
    let truncated_by = tr
        .truncated_by
        .as_ref()
        .expect("ToolOutput.truncated_by must reach the wire");
    assert_eq!(
        truncated_by.get("kind").and_then(|v| v.as_str()),
        Some("spilled_to"),
        "truncated_by must preserve the snake_case 'kind' discriminator; got {truncated_by:?}"
    );
    assert_eq!(
        truncated_by.get("path").and_then(|v| v.as_str()),
        Some("/tmp/spill.txt"),
        "truncated_by SpilledTo must carry the spill path"
    );
}
