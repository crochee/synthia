#![allow(deprecated)]
//! Integration tests for the Guardian `self_reflect` tool exposed to the LLM.

mod test_support;

use std::sync::Arc;

use futures::StreamExt;
use synthia_agent::{
    agent::Agent,
    config::AgentConfig,
    tools::SelfReflectTool,
    types::*,
};
use synthia_core::{Registry, RegistryItem};
use synthia_hook::HookRegistry;
use synthia_provider::types::{
    ContentPart,
    StreamChunk,
    TextContent,
    ToolResult,
    ToolUse,
};
use synthia_tool::registry::{ToolEntry, ToolRegistry};
use test_support::{FakeProvider, FakeTool, make_run_config};
use tokio_util::sync::CancellationToken;

const REFLECT_RESPONSE: &str = r#"{"summary":"review summary","issues":["issue1"],"suggestions":["suggestion1"]}"#;

fn make_tool_registry_with_self_reflect(
    provider: Arc<dyn synthia_provider::traits::ModelProvider>,
) -> ToolRegistry {
    let reg = ToolRegistry::new();
    // Register multiple noop tools with different names to avoid triggering
    // the loop detector (which fires after 3 consecutive calls to the same
    // tool with the same output).
    reg.register(ToolEntry::new(Arc::new(FakeTool::new(
        "noop1",
        "step 1 done",
    ))));
    reg.register(ToolEntry::new(Arc::new(FakeTool::new(
        "noop2",
        "step 2 done",
    ))));
    reg.register(ToolEntry::new(Arc::new(FakeTool::new(
        "noop3",
        "step 3 done",
    ))));
    reg.register(ToolEntry::new(Arc::new(FakeTool::new(
        "noop4",
        "step 4 done",
    ))));
    reg.register(ToolEntry::new(Arc::new(FakeTool::new(
        "noop5",
        "step 5 done",
    ))));
    reg.register(ToolEntry::new(Arc::new(FakeTool::new(
        "noop6",
        "step 6 done",
    ))));
    reg.register(ToolEntry::new(Arc::new(SelfReflectTool::new(
        provider,
        "fake-model",
    ))));
    reg
}

fn chunk_with_tool_call(name: &str) -> Vec<StreamChunk> {
    // No StreamChunk::Stop — the FakeProvider's complete_with_stream emits
    // IsDone after all chunks, which is the authoritative end-of-stream
    // signal. Including Stop causes the agent loop to break before IsDone,
    // triggering synchronous_fallback → provider.complete(), which consumes
    // an extra FakeProvider index and shifts all subsequent chunks.
    vec![StreamChunk::Content(ContentPart::ToolUse(ToolUse {
        id: format!("call-{name}"),
        name: name.to_string(),
        input: serde_json::json!({}),
    }))]
}

fn chunk_text_only(text: &str) -> Vec<StreamChunk> {
    // No StreamChunk::Stop — see comment in chunk_with_tool_call.
    vec![StreamChunk::Content(ContentPart::Text(TextContent {
        text: text.into(),
        cache_control: None,
    }))]
}

#[tokio::test]
async fn self_reflect_tool_is_registered() {
    let reg = ToolRegistry::new();
    let provider = Arc::new(FakeProvider::new(vec![]));
    reg.register(ToolEntry::new(Arc::new(SelfReflectTool::new(
        provider,
        "fake-model",
    ))));

    let entries = reg.list(None).await.unwrap();
    assert!(entries.iter().any(|e| e.name() == "self_reflect"));
}

#[tokio::test]
async fn self_reflect_llm_call_dispatches_through_tool_path() {
    let provider = Arc::new(
        FakeProvider::new(vec![REFLECT_RESPONSE.to_string()])
            .with_separate_complete_counter()
            .with_stream_chunks(vec![
                chunk_with_tool_call("self_reflect"),
                chunk_text_only("done"),
            ]),
    );
    let tool_reg = make_tool_registry_with_self_reflect(provider.clone());
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
        "reflect-llm".into(),
        AgentInput::text("go"),
        config,
        cancel_token,
    );

    let events: Vec<AgentEvent> = Agent::run_stream(run_config).collect().await;

    let started = events.iter().any(|e| {
        matches!(
            e,
            AgentEvent::Model(ContentPart::ToolUse(ToolUse { name, .. }))
                if name == "self_reflect"
        )
    });
    assert!(started, "expected ToolUse for self_reflect");

    let completed = events.iter().find(|e| {
        matches!(
            e,
            AgentEvent::Model(ContentPart::ToolResult(ToolResult {
                tool_use_id,
                ..
            })) if tool_use_id.starts_with("call-self_reflect")
                || tool_use_id.starts_with("self_reflect-auto-")
        )
    });
    assert!(completed.is_some(), "expected ToolResult for self_reflect");
    if let AgentEvent::Model(ContentPart::ToolResult(ToolResult {
        content,
        ..
    })) = completed.unwrap()
    {
        let combined: String = content
            .iter()
            .filter_map(|p| {
                if let ContentPart::Text(TextContent { text, .. }) = p {
                    Some(text.clone())
                } else {
                    None
                }
            })
            .collect();
        assert!(combined.contains("review summary"));
        assert!(combined.contains("issue1"));
        assert!(combined.contains("suggestion1"));
    }
}

#[tokio::test]
async fn self_reflect_auto_triggers_at_iteration_five() {
    // Use different tool names per iteration to avoid the loop detector
    // (which fires after 3 consecutive identical tool calls).
    let provider = Arc::new(
        FakeProvider::new(vec![REFLECT_RESPONSE.to_string()])
            .with_separate_complete_counter()
            .with_stream_chunks(vec![
                chunk_with_tool_call("noop1"),
                chunk_with_tool_call("noop2"),
                chunk_with_tool_call("noop3"),
                chunk_with_tool_call("noop4"),
                chunk_text_only("done"),
            ]),
    );
    let tool_reg = make_tool_registry_with_self_reflect(provider.clone());
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
        "reflect-auto".into(),
        AgentInput::text("go"),
        config,
        cancel_token,
    );

    let events: Vec<AgentEvent> = Agent::run_stream(run_config).collect().await;

    let started = events.iter().any(|e| {
        matches!(
            e,
            AgentEvent::Model(ContentPart::ToolUse(ToolUse { name, .. }))
                if name == "self_reflect"
        )
    });
    assert!(started, "expected auto-triggered self_reflect");

    let completed = events.iter().find(|e| {
        matches!(
            e,
            AgentEvent::Model(ContentPart::ToolResult(ToolResult {
                tool_use_id,
                ..
            })) if tool_use_id.starts_with("call-self_reflect")
                || tool_use_id.starts_with("self_reflect-auto-")
        )
    });
    assert!(completed.is_some());
}

#[tokio::test]
async fn self_reflect_counter_resets_after_llm_call() {
    // Iteration 2: LLM calls self_reflect -> next auto-trigger should be 7.
    // Iterations 3-6 use different noop tools to avoid the loop detector.
    // Iteration 7 is text-only, so the auto-trigger fires there (not at iter 5).
    let provider = Arc::new(
        FakeProvider::new(vec![REFLECT_RESPONSE.to_string()])
            .with_separate_complete_counter()
            .with_stream_chunks(vec![
                chunk_with_tool_call("noop1"),        // iter 1
                chunk_with_tool_call("self_reflect"), // iter 2
                chunk_with_tool_call("noop2"),        // iter 3
                chunk_with_tool_call("noop3"),        // iter 4
                chunk_with_tool_call("noop4"),        // iter 5
                chunk_with_tool_call("noop5"),        // iter 6
                chunk_text_only("done"),              // iter 7
            ]),
    );
    let tool_reg = make_tool_registry_with_self_reflect(provider.clone());
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
        "reflect-reset".into(),
        AgentInput::text("go"),
        config,
        cancel_token,
    );

    let events: Vec<AgentEvent> = Agent::run_stream(run_config).collect().await;

    let self_reflect_starts: Vec<_> = events
        .iter()
        .filter(|e| {
            matches!(
                e,
                AgentEvent::Model(ContentPart::ToolUse(ToolUse { name, .. }))
                    if name == "self_reflect"
            )
        })
        .collect();
    assert_eq!(
        self_reflect_starts.len(),
        2,
        "expected one LLM-driven and one auto-triggered self_reflect, got {:?}",
        events
    );
}

#[tokio::test]
async fn self_reflect_same_iteration_dedup() {
    // Iteration 5: LLM already calls self_reflect. The auto-trigger fallback
    // must be skipped, so there is exactly one self_reflect execution.
    // Different noop tools per iteration avoid the loop detector.
    let provider = Arc::new(
        FakeProvider::new(vec![REFLECT_RESPONSE.to_string()])
            .with_separate_complete_counter()
            .with_stream_chunks(vec![
                chunk_with_tool_call("noop1"),
                chunk_with_tool_call("noop2"),
                chunk_with_tool_call("noop3"),
                chunk_with_tool_call("noop4"),
                chunk_with_tool_call("self_reflect"),
                chunk_text_only("done"),
            ]),
    );
    let tool_reg = make_tool_registry_with_self_reflect(provider.clone());
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
        "reflect-dedup".into(),
        AgentInput::text("go"),
        config,
        cancel_token,
    );

    let events: Vec<AgentEvent> = Agent::run_stream(run_config).collect().await;

    let self_reflect_starts: Vec<_> = events
        .iter()
        .filter(|e| {
            matches!(
                e,
                AgentEvent::Model(ContentPart::ToolUse(ToolUse { name, .. }))
                    if name == "self_reflect"
            )
        })
        .collect();
    assert_eq!(
        self_reflect_starts.len(),
        1,
        "expected exactly one self_reflect when LLM already called it, got {:?}",
        events
    );
}

#[tokio::test]
async fn self_reflect_llm_and_auto_paths_are_consistent() {
    // First iteration: LLM-driven self_reflect (counter resets to 1+5=6).
    // Sixth iteration: text-only "done" — auto-trigger fires (6 >= 6) before break.
    // Different noop tools per iteration avoid the loop detector.
    let provider = Arc::new(
        FakeProvider::new(vec![REFLECT_RESPONSE.to_string()])
            .with_separate_complete_counter()
            .with_stream_chunks(vec![
                chunk_with_tool_call("self_reflect"),
                chunk_with_tool_call("noop1"),
                chunk_with_tool_call("noop2"),
                chunk_with_tool_call("noop3"),
                chunk_with_tool_call("noop4"),
                chunk_text_only("done"),
            ]),
    );
    let tool_reg = make_tool_registry_with_self_reflect(provider.clone());
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
        "reflect-consistent".into(),
        AgentInput::text("go"),
        config,
        cancel_token,
    );

    let events: Vec<AgentEvent> = Agent::run_stream(run_config).collect().await;

    let outputs: Vec<String> = events
        .iter()
        .filter_map(|e| {
            if let AgentEvent::Model(ContentPart::ToolResult(ToolResult {
                tool_use_id,
                content,
                ..
            })) = e
                && (tool_use_id.starts_with("call-self_reflect")
                    || tool_use_id.starts_with("self_reflect-auto-"))
            {
                let combined: String = content
                    .iter()
                    .filter_map(|p| {
                        if let ContentPart::Text(TextContent { text, .. }) = p {
                            Some(text.clone())
                        } else {
                            None
                        }
                    })
                    .collect();
                return Some(combined);
            }
            None
        })
        .collect();

    assert_eq!(outputs.len(), 2, "expected two self_reflect completions");
    for output in &outputs {
        assert!(output.contains("review summary"));
        assert!(output.contains("issue1"));
        assert!(output.contains("suggestion1"));
    }
}
