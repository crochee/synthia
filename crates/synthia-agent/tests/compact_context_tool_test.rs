#![allow(deprecated)]
//! Integration tests for the LLM-callable `compact_context` tool.
//!
//! These tests exercise the three pathways defined by the
//! `borrow-best-from-production-agents` change (task 5.2):
//!
//! 1. **Registration** — the tool is discoverable in the registry.
//! 2. **LLM-driven dispatch** — the LLM invokes `compact_context`; the
//!    facade tool acknowledges, the main loop runs the real compaction,
//!    and `ContextCompacted` + `CompactionAnalyticsAttempt(trigger=ToolCall)`
//!    are emitted.
//! 3. **Auto-trigger at 80%** — when the context ratio exceeds 80% and
//!    the LLM did not request compaction, the main loop auto-triggers
//!    compaction (`trigger=AutoThreshold`).
//! 4. **Dedup** — when the LLM calls `compact_context` in the same
//!    iteration that the auto-trigger would fire, only one compaction
//!    runs (the LLM-driven one).

mod test_support;

use std::sync::Arc;

use futures::StreamExt;
use synthia_agent::{
    agent::Agent,
    config::AgentConfig,
    events::{SystemEvent, WarningKind},
    tools::CompactContextTool,
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
use synthia_session::types::TokenBudget;
use synthia_tool::registry::{ToolEntry, ToolRegistry};
use test_support::{FakeProvider, FakeTool, make_run_config};
use tokio_util::sync::CancellationToken;

// ---- helpers ----

/// Build a tool registry with both a `noop` fake tool and the
/// `CompactContextTool`, mirroring the self_reflect test scaffolding.
fn make_tool_registry() -> ToolRegistry {
    let reg = ToolRegistry::new();
    reg.register(ToolEntry::new(Arc::new(FakeTool::new("noop", "ok"))));
    reg.register(ToolEntry::new(Arc::new(CompactContextTool)));
    reg
}

/// Build a tool registry with a `noop` tool whose output is `output`.
fn make_tool_registry_with_noop_output(output: &str) -> ToolRegistry {
    let reg = ToolRegistry::new();
    reg.register(ToolEntry::new(Arc::new(FakeTool::new("noop", output))));
    reg.register(ToolEntry::new(Arc::new(CompactContextTool)));
    reg
}

fn chunk_with_tool_call(name: &str) -> Vec<StreamChunk> {
    // NOTE: No `StreamChunk::Stop` is emitted here. The FakeProvider's
    // `complete_with_stream` already emits a trailing `IsDone` after the
    // canned chunks. If `Stop` were included, the `StepSample` loop would
    // break on `Stop` (see `ChunkOutcome::Stop`) before consuming
    // `IsDone`, triggering `synchronous_fallback` → `provider.complete()`.
    // Because `FakeProvider` shares a single `call_count` between
    // `complete` and `complete_with_stream`, the fallback would shift
    // the stream-chunk index and break multi-turn tests. Relying on
    // `IsDone` as the sole end-of-stream signal avoids the fallback
    // entirely.
    vec![StreamChunk::Content(ContentPart::ToolUse(ToolUse {
        id: format!("call-{name}"),
        name: name.to_string(),
        input: serde_json::json!({}),
    }))]
}

fn chunk_with_tool_call_and_input(
    name: &str,
    input: serde_json::Value,
) -> Vec<StreamChunk> {
    vec![StreamChunk::Content(ContentPart::ToolUse(ToolUse {
        id: format!("call-{name}"),
        name: name.to_string(),
        input,
    }))]
}

fn chunk_text_only(text: &str) -> Vec<StreamChunk> {
    vec![StreamChunk::Content(ContentPart::Text(TextContent {
        text: text.into(),
        cache_control: None,
    }))]
}

/// Build an `AgentConfig` with `hard_limit` and `max_iterations`.
fn make_config(hard_limit: usize, max_iterations: usize) -> AgentConfig {
    AgentConfig {
        max_iterations,
        context_token_budget: Some(TokenBudget::new(hard_limit)),
        ..Default::default()
    }
}

// ---- 1. Registration ----

#[tokio::test]
async fn compact_context_tool_is_registered() {
    let reg = ToolRegistry::new();
    reg.register(ToolEntry::new(Arc::new(CompactContextTool)));

    let entries = reg.list(None).await.unwrap();
    assert!(
        entries.iter().any(|e| e.name() == "compact_context"),
        "expected compact_context in the registry, got: {:?}",
        entries
            .iter()
            .map(|e| e.name().to_string())
            .collect::<Vec<_>>()
    );
}

// ---- 2. LLM-driven dispatch ----

#[tokio::test]
async fn compact_context_llm_call_dispatches_through_tool_path() {
    // hard_limit=100 → soft_limit (compact_messages) = 85.
    // 340-char input → HIGHER estimate ≈ 89 tokens (ratio 0.89).
    // After compact_context tool result (80 chars, ≈24 tokens) is added:
    // HIGHER ≈ 113 > 85 → compaction fires with trigger=ToolCall.
    let provider =
        Arc::new(FakeProvider::new(vec![]).with_stream_chunks(vec![
            chunk_with_tool_call("compact_context"),
            chunk_text_only("done"),
        ]));
    let tool_reg = make_tool_registry();
    let hook_reg = HookRegistry::new();
    let config = make_config(100, 2);
    let cancel_token = CancellationToken::new();

    let run_config = make_run_config(
        provider,
        tool_reg,
        hook_reg,
        "compact-llm".into(),
        AgentInput::text("x".repeat(340)),
        config,
        cancel_token,
    );

    let events: Vec<AgentEvent> = Agent::run_stream(run_config).collect().await;

    let started = events.iter().any(|e| {
        matches!(
            e,
            AgentEvent::Model(ContentPart::ToolUse(ToolUse { name, .. }))
                if name == "compact_context"
        )
    });
    assert!(started, "expected ToolUse for compact_context");

    let completed = events.iter().find(|e| {
        matches!(
            e,
            AgentEvent::Model(ContentPart::ToolResult(ToolResult {
                tool_use_id,
                ..
            })) if tool_use_id.starts_with("call-compact_context")
        )
    });
    assert!(
        completed.is_some(),
        "expected ToolResult for compact_context"
    );
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
        assert!(
            combined.contains("Compaction requested"),
            "expected facade acknowledgement, got: {combined}"
        );
    }

    // ContextCompacted is no longer its own event in Phase 2 —
    // context compaction is a Warning(kind=ContextCompaction) system event.
    let compacted = events.iter().any(|e| {
        matches!(
            e,
            AgentEvent::System(SystemEvent::Warning {
                kind: WarningKind::ContextCompaction,
                ..
            })
        )
    });
    assert!(
        compacted,
        "expected at least one ContextCompaction warning event, got: {:?}",
        events
    );
}

// ---- 3. LLM-driven dispatch with reason parameter ----

#[tokio::test]
async fn compact_context_llm_call_with_reason() {
    let provider =
        Arc::new(FakeProvider::new(vec![]).with_stream_chunks(vec![
            chunk_with_tool_call_and_input(
                "compact_context",
                serde_json::json!({"reason": "context too long"}),
            ),
            chunk_text_only("done"),
        ]));
    let tool_reg = make_tool_registry();
    let hook_reg = HookRegistry::new();
    let config = make_config(100, 2);
    let cancel_token = CancellationToken::new();

    let run_config = make_run_config(
        provider,
        tool_reg,
        hook_reg,
        "compact-reason".into(),
        AgentInput::text("x".repeat(340)),
        config,
        cancel_token,
    );

    let events: Vec<AgentEvent> = Agent::run_stream(run_config).collect().await;

    let started = events.iter().any(|e| {
        matches!(
            e,
            AgentEvent::Model(ContentPart::ToolUse(ToolUse { name, .. }))
                if name == "compact_context"
        )
    });
    assert!(started, "expected ToolUse for compact_context");

    let compacted = events.iter().any(|e| {
        matches!(
            e,
            AgentEvent::System(SystemEvent::Warning {
                kind: WarningKind::ContextCompaction,
                ..
            })
        )
    });
    assert!(
        compacted,
        "expected ContextCompaction warning when LLM calls compact_context with reason"
    );
}

// ---- 4. Auto-trigger at 80% ----

#[tokio::test]
async fn compact_context_auto_triggers_at_80_percent() {
    // hard_limit=100. 340-char input → HIGHER ≈ 89 (ratio 0.89 > 0.8).
    // LLM calls noop (not compact_context), so the auto-trigger path fires.
    // After noop tool result is added, ratio stays > 0.8 and compaction fires.
    let provider =
        Arc::new(FakeProvider::new(vec![]).with_stream_chunks(vec![
            chunk_with_tool_call("noop"),
            chunk_text_only("done"),
        ]));
    let tool_reg = make_tool_registry();
    let hook_reg = HookRegistry::new();
    let config = make_config(100, 2);
    let cancel_token = CancellationToken::new();

    let run_config = make_run_config(
        provider,
        tool_reg,
        hook_reg,
        "compact-auto".into(),
        AgentInput::text("x".repeat(340)),
        config,
        cancel_token,
    );

    let events: Vec<AgentEvent> = Agent::run_stream(run_config).collect().await;

    let compacted = events.iter().any(|e| {
        matches!(
            e,
            AgentEvent::System(SystemEvent::Warning {
                kind: WarningKind::ContextCompaction,
                ..
            })
        )
    });
    assert!(
        compacted,
        "expected auto-triggered ContextCompaction warning at >80% ratio, got: {:?}",
        events
    );
}

// ---- 5. LLM call does not disable auto-trigger ----

#[tokio::test]
async fn compact_context_llm_call_does_not_disable_auto_trigger() {
    // hard_limit=200 → compaction_at (soft_limit for compact_messages) = 170.
    // Iter 1: 400-char input → HIGHER ≈ 104 (ratio 0.52). LLM calls
    // compact_context. LLM-driven compaction returns None (HIGHER ≈ 142
    // <= 170). No ContextCompacted.
    // Iter 2: LLM calls noop with 200-char output. After tool result
    // added: HIGHER ≈ 204 > 160 (0.8*200). Auto-trigger fires.
    let provider =
        Arc::new(FakeProvider::new(vec![]).with_stream_chunks(vec![
            chunk_with_tool_call("compact_context"),
            chunk_with_tool_call("noop"),
            chunk_text_only("done"),
        ]));
    let tool_reg = make_tool_registry_with_noop_output(&"x".repeat(200));
    let hook_reg = HookRegistry::new();
    let config = make_config(200, 3);
    let cancel_token = CancellationToken::new();

    let run_config = make_run_config(
        provider,
        tool_reg,
        hook_reg,
        "compact-no-disable".into(),
        AgentInput::text("x".repeat(400)),
        config,
        cancel_token,
    );

    let events: Vec<AgentEvent> = Agent::run_stream(run_config).collect().await;

    // Iter 1: LLM called compact_context (one ToolUse).
    let compact_starts: Vec<_> = events
        .iter()
        .filter(|e| {
            matches!(
                e,
                AgentEvent::Model(ContentPart::ToolUse(ToolUse {
                    name,
                    ..
                })) if name == "compact_context"
            )
        })
        .collect();
    assert_eq!(
        compact_starts.len(),
        1,
        "expected exactly one compact_context ToolUse"
    );

    // At least one ContextCompaction warning (from iter 2 auto-trigger).
    let compacted: Vec<_> = events
        .iter()
        .filter(|e| {
            matches!(
                e,
                AgentEvent::System(SystemEvent::Warning {
                    kind: WarningKind::ContextCompaction,
                    ..
                })
            )
        })
        .collect();
    assert!(
        !compacted.is_empty(),
        "expected at least one ContextCompaction warning from auto-trigger, got: {:?}",
        events
    );
}

// ---- 6. Same-iteration dedup ----

#[tokio::test]
async fn compact_context_same_iteration_dedup() {
    // hard_limit=100. 340-char input → HIGHER ≈ 89 (ratio 0.89 > 0.8).
    // LLM calls compact_context in the SAME iteration where ratio > 0.8.
    // The auto-trigger is skipped (dedup flag), and the LLM-driven path
    // runs exactly one compaction.
    let provider =
        Arc::new(FakeProvider::new(vec![]).with_stream_chunks(vec![
            chunk_with_tool_call("compact_context"),
            chunk_text_only("done"),
        ]));
    let tool_reg = make_tool_registry();
    let hook_reg = HookRegistry::new();
    let config = make_config(100, 2);
    let cancel_token = CancellationToken::new();

    let run_config = make_run_config(
        provider,
        tool_reg,
        hook_reg,
        "compact-dedup".into(),
        AgentInput::text("x".repeat(340)),
        config,
        cancel_token,
    );

    let events: Vec<AgentEvent> = Agent::run_stream(run_config).collect().await;

    let compacted: Vec<_> = events
        .iter()
        .filter(|e| {
            matches!(
                e,
                AgentEvent::System(SystemEvent::Warning {
                    kind: WarningKind::ContextCompaction,
                    ..
                })
            )
        })
        .collect();
    assert_eq!(
        compacted.len(),
        1,
        "expected exactly one ContextCompaction warning (dedup), got {} events: {:?}",
        compacted.len(),
        events
    );
}
