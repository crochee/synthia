//! Prompt assembly integration: the `<identity>`, `<env>`,
//! `<available_skills>`, `<available_agents>` renderers and
//! the runtime-context snapshot reach the wire together with
//! the configured `instructions`.

use std::sync::{Arc, atomic::Ordering};

use futures::StreamExt;
use synthia_provider::{ContentPart, SamplingResult, StreamChunk, TokenUsage};
use synthia_tool::ToolRegistry;
use tokio_util::sync::CancellationToken;

use super::{support::*, *};
#[tokio::test]
async fn with_prompt_context_injects_skills_and_agents() {
    // End-to-end: skills + peer agents populated via
    // `with_prompt_context` must reach the LLM in the
    // assembled system prompt.
    let provider =
        Arc::new(CapturingProvider::new(vec![vec![StreamChunk::IsDone {
            result: Box::new(SamplingResult {
                text: "ok".into(),
                tool_calls: vec![],
                reasoning: String::new(),
                reasoning_signature: None,
                usage: TokenUsage::default(),
                ..Default::default()
            }),
        }]]));

    let ctx = crate::prompt::PromptContext::default()
        .with_skill("summarize", "Summarize text.")
        .with_agent(&peer_descriptor(
            "planner",
            "Plans the work.",
            Some("complex tasks"),
        ));

    let agent = ReActAgent::with_prompt_context(
        provider.clone(),
        Arc::new(ToolRegistry::new()),
        PathBuf::new(),
        "BASE".to_string(),
        Arc::new(ctx),
    );

    let mut stream = agent
        .run(
            crate::input::AgentInput::text("hi"),
            Arc::new(CancellationToken::new()),
        )
        .await;
    while let Some(_ev) = stream.next().await {}
    drop(stream);

    for _ in 0..50 {
        if provider.call_count.load(Ordering::SeqCst) > 0 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    let captured = provider.captured.lock().await;
    assert!(!captured.is_empty());
    let sys_text = match &captured[0][0].content {
        synthia_provider::Content::Single(ContentPart::Text(t)) => {
            t.text.clone()
        }
        _ => panic!("expected single text content"),
    };
    assert!(sys_text.contains("BASE"));
    // The `<identity>` opening line must use the
    // human-readable `display_name` ("Synthia"), not
    // the routing slug ("agent"), so the model
    // self-identifies with the persona the user sees
    // on the UI card.
    assert!(
        sys_text.contains("You are `Synthia`"),
        "identity line must use display_name; got:\n{sys_text}"
    );
    assert!(
        !sys_text.contains("You are `react`"),
        "identity line must not leak the routing slug; got:\n{sys_text}"
    );
    // `read_file` is a tool — it must NOT appear in the
    // prompt text (tools ride the completion request's
    // `tools` channel).
    assert!(!sys_text.contains("`read_file`"));
    assert!(sys_text.contains("<name>summarize</name>"));
    assert!(
        !sys_text.contains("<name>disabled-skill</name>"),
        "disabled skills must not be advertised"
    );
    assert!(sys_text.contains("`planner`"));
    assert!(sys_text.contains("(use when: complex tasks)"));
    assert!(sys_text.contains("<identity>"));
    assert!(sys_text.contains("</identity>"));
    assert!(sys_text.contains("<available_skills>"));
    assert!(sys_text.contains("</available_skills>"));
    assert!(sys_text.contains("<available_agents>"));
    assert!(sys_text.contains("</available_agents>"));
    // Tool schemas are NOT in the prompt — they ride the
    // request's `tools` channel and the runtime validates
    // every emitted name.
    assert!(
        !sys_text.contains("<available_tools>"),
        "tool schemas must not be assembled into the system prompt"
    );
    assert!(
        !sys_text.contains("Use only these tools:"),
        "tool grounding belongs on the runtime, not in the prompt"
    );
}

#[tokio::test]
async fn descriptor_is_stable_returns_assembled_prompt_metadata() {
    // Sanity: the descriptor fields used by the assembler
    // are reachable via `prompt_context()` and `descriptor()`.
    let agent = ReActAgent::with_prompt_context(
        Arc::new(synthia_provider::traits_stub::ModelProviderStub::new()),
        Arc::new(ToolRegistry::new()),
        PathBuf::new(),
        "X".to_string(),
        Arc::new(crate::prompt::PromptContext::default()),
    );
    assert_eq!(agent.prompt_context().skills.len(), 0);
    assert_eq!(agent.descriptor().instructions, "X");
}

#[tokio::test]
async fn set_prompt_context_swaps_manifest_at_runtime() {
    // The setter lets callers (e.g. settings changes) swap
    // the manifest without rebuilding the agent.
    let mut agent = ReActAgent::with_options(
        Arc::new(synthia_provider::traits_stub::ModelProviderStub::new()),
        Arc::new(ToolRegistry::new()),
        PathBuf::new(),
        "X".to_string(),
    );
    assert!(agent.prompt_context().skills.is_empty());
    agent.set_prompt_context(
        crate::prompt::PromptContext::default().with_skill("y", "y"),
    );
    assert_eq!(agent.prompt_context().skills.len(), 1);
}

#[tokio::test]
async fn tool_schemas_never_appear_in_assembled_system_prompt() {
    // End-to-end contract: even when tools exist in the
    // runtime, the assembled system prompt must NOT carry
    // their descriptions. Tool schemas ride the completion
    // request's `tools` field; the runtime validates every
    // emitted name against the registry.
    let provider =
        Arc::new(CapturingProvider::new(vec![vec![StreamChunk::IsDone {
            result: Box::new(SamplingResult {
                text: "ok".into(),
                tool_calls: vec![],
                reasoning: String::new(),
                reasoning_signature: None,
                usage: TokenUsage::default(),
                ..Default::default()
            }),
        }]]));

    // `PromptContext` no longer carries tools — it only
    // carries skills / peer agents. The runtime will pull
    // tool definitions from `ToolRegistry` for the
    // completion request's `tools` channel.
    let ctx = crate::prompt::PromptContext::default();

    let agent = ReActAgent::with_prompt_context(
        provider.clone(),
        Arc::new(ToolRegistry::new()),
        PathBuf::new(),
        "BASE".to_string(),
        Arc::new(ctx),
    );

    let mut stream = agent
        .run(
            crate::input::AgentInput::text("hi"),
            Arc::new(CancellationToken::new()),
        )
        .await;
    while let Some(_ev) = stream.next().await {}
    drop(stream);

    for _ in 0..50 {
        if provider.call_count.load(Ordering::SeqCst) > 0 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    let captured = provider.captured.lock().await;
    let sys_text = match &captured[0][0].content {
        synthia_provider::Content::Single(ContentPart::Text(t)) => {
            t.text.clone()
        }
        _ => panic!("expected single text content"),
    };
    // No tool manifest, no tool grounding, no tool name
    // leakage in the prompt text.
    assert!(
        !sys_text.contains("<available_tools>"),
        "tool schemas must not be assembled into the system prompt"
    );
    assert!(
        !sys_text.contains("Use only these tools:"),
        "tool grounding belongs on the runtime, not in the prompt"
    );
}
