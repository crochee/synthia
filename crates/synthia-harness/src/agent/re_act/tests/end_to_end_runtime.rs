//! End-to-end runtime injection: drive the loop and inspect
//! the `CompletionRequest` the loop actually sends to the
//! provider. The first message must always be the assembled
//! system prompt, even when `instructions` is empty.

use std::sync::{Arc, atomic::Ordering};

use futures::StreamExt;
use synthia_provider::{ContentPart, SamplingResult, StreamChunk, TokenUsage};
use synthia_tool::ToolRegistry;
use tokio_util::sync::CancellationToken;

use super::{support::*, *};

#[tokio::test]
async fn system_prompt_is_injected_into_llm_request_first_message() {
    // End-to-end: drive `ReActAgent::run` and inspect the actual
    // `CompletionRequest.messages` that reaches the provider.
    // The very first message MUST be a `Role::System` whose
    // text contains the configured `system_prompt` / descriptor
    // `instructions` followed by the assembled manifest
    // sections. This proves the prompt is not just stored on
    // the agent — it is the first thing the LLM sees, and the
    // manifest sections reach the provider too.
    const PROMPT: &str = "PROBE-SYSTEM-PROMPT-XYZ";

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

    let agent = ReActAgent::with_options(
        provider.clone(),
        Arc::new(ToolRegistry::new()),
        PathBuf::new(),
        PROMPT.to_string(),
    );

    let mut stream = agent
        .run(
            crate::input::AgentInput::text("hi"),
            Arc::new(CancellationToken::new()),
        )
        .await;
    while let Some(_ev) = stream.next().await {}

    // Drain events so the spawned task completes.
    drop(stream);

    // Allow the spawned task a tick to finish (it must complete
    // because the loop terminated on the no-tool-call branch).
    for _ in 0..50 {
        if provider.call_count.load(Ordering::SeqCst) > 0 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    let captured = provider.captured.lock().await;
    assert!(
        !captured.is_empty(),
        "provider should have received at least one CompletionRequest",
    );
    let messages = &captured[0];
    assert!(
        !messages.is_empty(),
        "CompletionRequest.messages must be non-empty",
    );

    use synthia_provider::Role;
    assert_eq!(
        messages[0].role,
        Role::System,
        "first message must be the system prompt",
    );
    let sys_text = match &messages[0].content {
        synthia_provider::Content::Single(ContentPart::Text(t)) => {
            t.text.clone()
        }
        synthia_provider::Content::Multi(parts) => parts
            .iter()
            .filter_map(|p| match p {
                ContentPart::Text(t) => Some(t.text.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(""),
        _ => String::new(),
    };
    assert!(
        sys_text.contains(PROMPT),
        "system prompt text must contain what was configured (got {sys_text:?})",
    );
    assert!(
        sys_text.contains("<identity>"),
        "assembler must inject the identity section",
    );
    // This test does not register any tools, so the
    // `<available_tools>` section is correctly absent (the
    // assembler drops empty manifest sections — see
    // `prompt::tests::empty_manifests_drop_their_section`).
    // The `<identity>` block being present is sufficient
    // evidence the manifest pipeline ran end-to-end.

    // The user prompt must follow the system prompt.
    if messages.len() >= 2 {
        assert_eq!(messages[1].role, Role::User);
    }
}

#[tokio::test]
async fn empty_system_prompt_still_emits_assembled_identity() {
    // Regression guard: the prompt assembler MUST always
    // emit a non-empty system message whenever a descriptor
    // is present (which is every registered agent). Even an
    // empty `descriptor.instructions` is followed by the
    // identity section, which carries the agent's name,
    // kind, and version — so the LLM always sees who it is.
    //
    // The old "no system message when instructions empty"
    // rule was scoped to the bare `descriptor.instructions`
    // path; the assembler generalises it to "the assembled
    // system message must never be empty" while preserving
    // the observability benefit of always knowing the
    // agent's identity.
    use crate::{agent::descriptor::AgentDescriptor, prompt::PromptContext};

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

    let agent = ReActAgent::with_prompt_context(
        provider.clone(),
        Arc::new(ToolRegistry::new()),
        PathBuf::new(),
        // empty system prompt
        String::new(),
        Arc::new(PromptContext::default()),
    );

    // Force the descriptor back to a bare shape so the
    // assembler has nothing to render.
    let bare = AgentDescriptor {
        name: "bare".into(),
        description: String::new(),
        kind: "bare".into(),
        version: "0.0.0".into(),
        instructions: String::new(),
        capabilities: Vec::new(),
        tools: Vec::new(),
        model_hint: None,
        handoffs: Vec::new(),
        handoff_hint: None,
        output_schema: None,
        owner: None,
        domain: None,

        persona: None,
        display_name: None,
        max_iterations: None,
    };
    let mut agent = agent;
    agent.descriptor_mut(bare);

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
    use synthia_provider::Role;
    // Even with empty instructions the assembler emits the
    // identity section (containing the bare descriptor's
    // name/kind/version), so the first message is System.
    assert_eq!(
        captured[0][0].role,
        Role::System,
        "empty instructions + bare descriptor must still emit a non-empty system message via the identity section",
    );
    let sys_text = match &captured[0][0].content {
        synthia_provider::Content::Single(ContentPart::Text(t)) => {
            t.text.clone()
        }
        _ => String::new(),
    };
    assert!(
        sys_text.contains("<identity>"),
        "assembled system message must include the identity section"
    );
    assert!(
        sys_text.contains("`bare`"),
        "identity section must surface the agent's name"
    );
    assert!(!sys_text.is_empty(), "system message must never be empty",);
    assert_eq!(captured[0][1].role, Role::User);
}

#[tokio::test]
async fn run_stream_emits_session_ended_terminal() {
    let agent = ReActAgent::new(
        Arc::new(synthia_provider::traits_stub::ModelProviderStub::text_only(
            "",
        )),
        Arc::new(ToolRegistry::new()),
    );
    let mut stream = agent
        .run(AgentInput::text(""), Arc::new(CancellationToken::new()))
        .await;
    let mut saw_end = false;
    while let Some(ev) = stream.next().await {
        if matches!(
            ev,
            AgentEvent::System(SystemEvent::SessionEnded {
                reason: SessionEndReason::Completed,
            })
        ) {
            saw_end = true;
            break;
        }
    }
    assert!(saw_end, "expected SessionEnded(Completed)");
}
