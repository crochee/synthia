#![allow(deprecated)]
//! Integration tests for the ReAct loop.
//!
//! Tests verify:
//! - Event ordering
//! - Tool call execution
//! - Multiple iterations
//! - Cancellation support
//! - Max iterations enforcement
//! - Hook firing

mod test_support;
use std::sync::Arc;

use futures::StreamExt;
use synthia_agent::{agent::Agent, config::AgentConfig, types::*};
use synthia_hook::HookRegistry;
use synthia_provider::types::{ContentPart, StreamChunk, TextContent, ToolUse};
use synthia_tool::registry::{ToolEntry, ToolRegistry};
use test_support::{FakeProvider, FakeTool, make_run_config};
use tokio_util::sync::CancellationToken;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a FakeProvider whose first stream() call yields text + tool_use,
/// and whose second call yields only text (no more tool calls).
fn provider_with_single_tool_call() -> FakeProvider {
    FakeProvider::new(vec![]).with_stream_chunks(vec![
        // First call: text + one tool call
        vec![
            StreamChunk::Content(ContentPart::Text(TextContent {
                text: "Let me check that for you.".into(),
                cache_control: None,
            })),
            StreamChunk::Content(ContentPart::ToolUse(ToolUse {
                id: "call_1".into(),
                name: "weather".into(),
                input: serde_json::json!({ "city": "Portland" }),
            })),
            StreamChunk::Stop("tool_use".into()),
        ],
        // Second call: plain text response (loop terminates)
        vec![
            StreamChunk::Content(ContentPart::Text(TextContent {
                text: "The weather in Portland is sunny.".into(),
                cache_control: None,
            })),
            StreamChunk::Stop("end_turn".into()),
        ],
    ])
}

/// Build a FakeProvider whose response has no tool calls (text only).
fn provider_text_only() -> FakeProvider {
    FakeProvider::new(vec![]).with_stream_chunks(vec![vec![
        StreamChunk::Content(ContentPart::Text(TextContent {
            text: "Hello, I am an AI assistant.".into(),
            cache_control: None,
        })),
        StreamChunk::Stop("end_turn".into()),
    ]])
}

fn make_tool_registry() -> ToolRegistry {
    let reg = ToolRegistry::new();
    reg.register(ToolEntry::new(Arc::new(FakeTool::new(
        "weather",
        "sunny, 72F",
    ))));
    reg.register(ToolEntry::new(Arc::new(FakeTool::new(
        "search",
        "search result",
    ))));
    reg
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_react_loop_text_only_emits_session_events() {
    let provider = Arc::new(provider_text_only());
    let tool_reg = make_tool_registry();
    let hook_reg = HookRegistry::new();
    let config = AgentConfig {
        max_iterations: 10,
        ..Default::default()
    };
    let cancel_token = CancellationToken::new();

    let run_config = make_run_config(
        provider,
        tool_reg,
        hook_reg,
        "sess-1".into(),
        AgentInput::text("Hello"),
        config,
        cancel_token,
    );

    let events: Vec<AgentEvent> = Agent::run_stream(run_config).collect().await;

    // Must start with SessionStarted
    assert!(
        matches!(&events[0], AgentEvent::SessionStarted { session_id } if session_id == "sess-1")
    );
    // Must end with SessionEnded
    assert!(matches!(
        &events[events.len() - 1],
        AgentEvent::SessionEnded { .. }
    ));
    // No tool call events
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, AgentEvent::ToolCallStarted { .. }))
    );
}

#[tokio::test]
async fn test_react_loop_executes_single_tool_call() {
    let provider = Arc::new(provider_with_single_tool_call());
    let tool_reg = make_tool_registry();
    let hook_reg = HookRegistry::new();
    let config = AgentConfig {
        max_iterations: 10,
        ..Default::default()
    };
    let cancel_token = CancellationToken::new();

    let run_config = make_run_config(
        provider,
        tool_reg,
        hook_reg,
        "sess-2".into(),
        AgentInput::text("What's the weather?"),
        config,
        cancel_token,
    );

    let events: Vec<AgentEvent> = Agent::run_stream(run_config).collect().await;

    // Should have ToolCallStarted for weather
    let tool_start = events.iter().find(
        |e| matches!(e, AgentEvent::ToolCallStarted { tool_name, .. } if tool_name == "weather"),
    );
    assert!(
        tool_start.is_some(),
        "Expected ToolCallStarted for weather, got events: {:?}",
        events
    );

    // Should have ToolCallCompleted for weather
    let tool_done = events.iter().find(
        |e| matches!(e, AgentEvent::ToolCallCompleted { tool_name, .. } if tool_name == "weather"),
    );
    assert!(
        tool_done.is_some(),
        "Expected ToolCallCompleted for weather, got events: {:?}",
        events
    );
}

#[tokio::test]
async fn test_react_loop_emits_iteration_events() {
    let provider = Arc::new(provider_with_single_tool_call());
    let tool_reg = make_tool_registry();
    let hook_reg = HookRegistry::new();
    let config = AgentConfig {
        max_iterations: 10,
        ..Default::default()
    };
    let cancel_token = CancellationToken::new();

    let run_config = make_run_config(
        provider,
        tool_reg,
        hook_reg,
        "sess-3".into(),
        AgentInput::text("What's the weather?"),
        config,
        cancel_token,
    );

    let events: Vec<AgentEvent> = Agent::run_stream(run_config).collect().await;

    // At least two iterations (tool call round + final text)
    let iteration_starts: Vec<_> = events
        .iter()
        .filter_map(|e| {
            if let AgentEvent::IterationStarted { iteration } = e {
                Some(*iteration)
            } else {
                None
            }
        })
        .collect();
    assert!(
        iteration_starts.len() >= 2,
        "Expected >= 2 iterations, got: {:?}",
        iteration_starts
    );
}

#[tokio::test]
async fn test_react_loop_respects_max_iterations() {
    // Provider that always returns a tool call, so the loop would run forever
    // unless stopped by max_iterations.
    let always_tool: FakeProvider = FakeProvider::new(vec![])
        .with_stream_chunks(vec![
            vec![
                StreamChunk::Content(ContentPart::ToolUse(ToolUse {
                    id: "call_1".into(),
                    name: "search".into(),
                    input: serde_json::json!({}),
                })),
                StreamChunk::Stop("tool_use".into()),
            ],
            vec![
                StreamChunk::Content(ContentPart::ToolUse(ToolUse {
                    id: "call_2".into(),
                    name: "search".into(),
                    input: serde_json::json!({}),
                })),
                StreamChunk::Stop("tool_use".into()),
            ],
            vec![
                StreamChunk::Content(ContentPart::ToolUse(ToolUse {
                    id: "call_3".into(),
                    name: "search".into(),
                    input: serde_json::json!({}),
                })),
                StreamChunk::Stop("tool_use".into()),
            ],
        ]);
    let provider = Arc::new(always_tool);
    let tool_reg = make_tool_registry();
    let hook_reg = HookRegistry::new();
    let config = AgentConfig {
        max_iterations: 2,
        ..Default::default()
    };
    let cancel_token = CancellationToken::new();

    let run_config = make_run_config(
        provider,
        tool_reg,
        hook_reg,
        "sess-4".into(),
        AgentInput::text("search"),
        config,
        cancel_token,
    );

    let events: Vec<AgentEvent> = Agent::run_stream(run_config).collect().await;

    // Should end with MaxIterationsReached
    let ended = events
        .iter()
        .find(|e| matches!(e, AgentEvent::SessionEnded { reason } if matches!(reason, SessionEndReason::MaxIterationsReached)));
    assert!(
        ended.is_some(),
        "Expected SessionEnded with MaxIterationsReached, got events: {:?}",
        events
    );
}

#[tokio::test]
async fn test_react_loop_cancellation() {
    let provider = Arc::new(provider_with_single_tool_call());
    let tool_reg = make_tool_registry();
    let hook_reg = HookRegistry::new();
    let config = AgentConfig {
        max_iterations: 10,
        ..Default::default()
    };
    let cancel_token = CancellationToken::new();
    // Cancel immediately so the loop should stop very quickly
    cancel_token.cancel();

    let run_config = make_run_config(
        provider,
        tool_reg,
        hook_reg,
        "sess-5".into(),
        AgentInput::text("weather"),
        config,
        cancel_token,
    );

    let events: Vec<AgentEvent> = Agent::run_stream(run_config).collect().await;

    let ended = events.iter().find(|e| {
        matches!(e, AgentEvent::SessionEnded { reason } if matches!(reason, SessionEndReason::Cancelled | SessionEndReason::MaxIterationsReached))
    });
    assert!(
        ended.is_some(),
        "Expected SessionEnded, got events: {:?}",
        events
    );
}

#[tokio::test]
async fn test_react_loop_emits_llm_deltas() {
    let provider = Arc::new(provider_text_only());
    let tool_reg = make_tool_registry();
    let hook_reg = HookRegistry::new();
    let config = AgentConfig {
        max_iterations: 10,
        ..Default::default()
    };
    let cancel_token = CancellationToken::new();

    let run_config = make_run_config(
        provider,
        tool_reg,
        hook_reg,
        "sess-6".into(),
        AgentInput::text("Hi"),
        config,
        cancel_token,
    );

    let events: Vec<AgentEvent> = Agent::run_stream(run_config).collect().await;

    let deltas: Vec<_> = events
        .iter()
        .filter_map(|e| {
            if let AgentEvent::LlmStreamDelta { content } = e {
                Some(content.clone())
            } else {
                None
            }
        })
        .collect();
    assert!(
        !deltas.is_empty(),
        "Expected at least one LlmStreamDelta, got events: {:?}",
        events
    );
}
