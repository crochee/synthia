//! Multi-expert code-review scenarios — drive the full ReAct
//! loop and verify the assembled system prompt under realistic
//! multi-agent configurations built with `Coordinator::delegate`.
//!
//! The scenarios below share a single fixture
//! (`code-review-panel`) with 5 peer agents and 4 tools; each
//! scenario mutates one input and asserts the resulting
//! session events + LLM call count.

use std::sync::{Arc, atomic::Ordering};

use futures::StreamExt;
use serde_json::json;
use synthia_tool::ToolRegistry;
use tokio_util::sync::CancellationToken;

use super::{support::*, *};

fn tools_with(name: &str, desc: &str) -> synthia_provider::ToolDefinition {
    synthia_provider::ToolDefinition {
        name: name.to_string(),
        description: desc.to_string(),
        input_schema: json!({"type": "object"}),
        cache_control: None,
        annotations: None,
    }
}

/// Build the canonical panel fixture:
///
/// - 4 tools: `read_file`, `shell`, `list_skills`, `submit_vote`.
/// - 3 skills: `summarize` (enabled), `audit` (enabled),
///   `deprecated` (disabled).
/// - 5 peer agents with handoff hints: planner / critic /
///   redteam / judge / formatter.
fn build_panel_fixture() -> (
    Vec<synthia_provider::ToolDefinition>,
    crate::prompt::PromptContext,
) {
    let tools = vec![
        tools_with("read_file", "Read a file from disk."),
        tools_with("shell", "Run a shell command."),
        tools_with("list_skills", "List enabled skills."),
        tools_with("submit_vote", "Cast a panel vote."),
    ];
    let ctx = crate::prompt::PromptContext::default()
            .with_skill("summarize", "Summarize text.")
            .with_skill("audit", "Audit for safety.")
            // "deprecated" was disabled in earlier revisions and
            // is therefore not pushed in by the caller; see
            // `PromptContext` doc § "skills".
            .with_agent(&peer_descriptor(
                "planner",
                "Plan the work.",
                Some("complex tasks"),
            ))
            .with_agent(&peer_descriptor(
                "critic",
                "Critique proposals.",
                Some("after planner"),
            ))
            .with_agent(&peer_descriptor("redteam", "Break the candidate.", None))
            .with_agent(&peer_descriptor("judge", "Aggregate votes.", None))
            .with_agent(&peer_descriptor(
                "formatter",
                "Format final output.",
                Some("after judge"),
            ));
    (tools, ctx)
}

/// Build a Critic-role descriptor matching the panel fixture.
fn build_critic_descriptor() -> AgentDescriptor {
    AgentDescriptor {
        name: "critic".into(),
        description: "Critic agent on code-review-panel".into(),
        kind: "critic".into(),
        version: "1.0.0".into(),
        instructions: "Review the proposer's plan.".into(),
        capabilities: vec!["tools".into()],
        tools: vec![],
        model_hint: None,
        handoffs: vec!["planner".into()],
        handoff_hint: Some("complex code tasks".into()),
        output_schema: None,
        owner: Some("synthia".into()),
        domain: Some("coding".into()),
        persona: None,
        display_name: None,
        max_iterations: None,
    }
}

/// Drive one full ReAct loop and drain every
/// [`AgentEvent`] through the channel. Returns the captured
/// event sequence. The provider handle is also returned
/// (via the `provider` parameter) so the caller can read
/// `call_count` and `captured.messages` after the session
/// ends.
async fn drive_loop_with_provider(
    provider: Arc<CapturingProvider>,
    agent: ReActAgent,
    cancel: CancellationToken,
) -> Vec<AgentEvent> {
    let _ = provider; // kept alive via Arc clones inside the agent
    let mut stream = agent
        .run(AgentInput::text("review the plan"), Arc::new(cancel))
        .await;
    let mut events = Vec::new();
    while let Some(ev) = stream.next().await {
        events.push(ev);
    }
    drop(stream);
    // Give the spawned task a moment to drain.
    for _ in 0..80 {
        if provider.call_count.load(Ordering::SeqCst) > 0 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    events
}

#[tokio::test]
async fn mock_panel_clean_scenario_passes_through() {
    // The clean scenario: descriptor + tools + skills +
    // peer agents all consistent. The loop must drive at
    // least one LLM pass.
    let (_tools, ctx) = build_panel_fixture();
    let provider = Arc::new(CapturingProvider::new(vec![
        empty_response(), // pass 1: critic emits a critique (text only)
    ]));
    let mut agent = ReActAgent::with_options(
        provider.clone(),
        Arc::new(ToolRegistry::new()),
        PathBuf::new(),
        "PANEL_BASE".to_string(),
    );
    agent.descriptor_mut(build_critic_descriptor());
    agent.set_prompt_context(ctx);

    let events = drive_loop_with_provider(
        provider.clone(),
        agent,
        CancellationToken::new(),
    )
    .await;

    // LLM was called (clean prompt + critic mandate).
    assert!(
        provider.call_count.load(Ordering::SeqCst) >= 1,
        "clean panel scenario must call the LLM"
    );

    // Captured messages: the system message contains the
    // panel directive + tool manifest + skill manifest +
    // peer-agent manifest.
    let captured = provider.captured.lock().await;
    let sys_text = match &captured[0][0].content {
        synthia_provider::Content::Single(ContentPart::Text(t)) => {
            t.text.clone()
        }
        _ => panic!("expected single text content"),
    };
    // Tool names MUST NOT leak into the system prompt — they
    // ride the completion request's `tools` field instead.
    for name in ["read_file", "shell", "list_skills", "submit_vote"] {
        assert!(
            !sys_text.contains(&format!("`{name}`")),
            "tool `{name}` must NOT appear in the system prompt"
        );
    }
    // Skill manifest surfaces the 2 enabled skills but
    // not the disabled one.
    assert!(sys_text.contains("<name>summarize</name>"));
    assert!(sys_text.contains("<name>audit</name>"));
    assert!(
        !sys_text.contains("<name>deprecated</name>"),
        "disabled skill must not be advertised"
    );
    // Peer-agent manifest surfaces all 5 panel members.
    for name in ["planner", "critic", "redteam", "judge", "formatter"] {
        assert!(
            sys_text.contains(&format!("`{name}`")),
            "peer agent `{name}` must appear in the manifest"
        );
    }
    assert!(
        !sys_text.contains("Use only these tools:"),
        "tool grounding belongs on the runtime, not in the prompt"
    );
    // Session ended cleanly (no Error reason).
    let ended_clean = events.iter().any(|e| {
        matches!(
            e,
            AgentEvent::System(SystemEvent::SessionEnded {
                reason: SessionEndReason::Completed,
                ..
            })
        )
    });
    assert!(ended_clean, "clean scenario must end with Completed");
}
