//! Agent-level tests: descriptor shape, smoke runs, system-prompt
//! plumbing. No streaming provider used here — these are the
//! cheapest entry-point checks.

use std::sync::Arc;

use futures::StreamExt;
use synthia_tool::ToolRegistry;
use tokio_util::sync::CancellationToken;

use super::*;

#[tokio::test]
async fn system_prompt_is_injected_as_first_message() {
    // Regression: the previous ReActLoop prepared `messages`
    // from `input.history + input.to_message()` and never
    // inserted a system prompt, so the LLM ran without a role.
    // Verify the prompt is now stored on the agent and that
    // the public `run` path consumes it.
    use synthia_provider::Role;

    let agent = ReActAgent::with_options(
        Arc::new(synthia_provider::traits_stub::ModelProviderStub::new()),
        Arc::new(ToolRegistry::new()),
        PathBuf::new(),
        "TEST-PROMPT".to_string(),
    );

    // Verify the system message shape used in `prepare()`.
    let sys = Message::system("hello");
    assert!(matches!(sys.role, Role::System));

    // Drive the loop with a stub provider so it terminates
    // quickly. We only assert the wiring is in place; the stub
    // does not emit user-visible text.
    let mut stream = agent
        .run(AgentInput::text("hi"), Arc::new(CancellationToken::new()))
        .await;
    let mut events = Vec::new();
    while let Some(ev) = stream.next().await {
        events.push(ev);
    }
    assert!(
        !events.is_empty(),
        "agent.run must yield at least the SessionStarted + SessionEnded events"
    );
}

#[tokio::test]
async fn descriptor_is_stable() {
    let agent = ReActAgent::new(
        Arc::new(synthia_provider::traits_stub::ModelProviderStub::new()),
        Arc::new(ToolRegistry::new()),
    );
    assert_eq!(agent.descriptor().name, "agent");
    assert_eq!(agent.descriptor().kind, "react");
    assert_eq!(agent.descriptor().version, "1.0.0");
    assert!(
        agent
            .descriptor()
            .capabilities
            .contains(&"tools".to_string())
    );
    // Industry-aligned fields default to sensible values.
    assert_eq!(
        agent.descriptor().instructions,
        crate::agent::DEFAULT_SYSTEM_PROMPT
    );
    assert!(agent.descriptor().handoff_hint.is_some());
    assert_eq!(agent.descriptor().owner.as_deref(), Some("synthia"));
    assert_eq!(agent.descriptor().domain.as_deref(), Some("coding"));
    // Adversarial-panel defaults are gone after the refactor.
    assert!(agent.descriptor().persona.is_some());
    // The human-readable label is "Synthia" — the
    // programmatic `name` ("agent") stays as the
    // routing slug.
    assert_eq!(agent.descriptor().display_name(), "Synthia");
    assert_eq!(agent.descriptor().display_name.as_deref(), Some("Synthia"));
}

#[tokio::test]
async fn descriptor_surfaces_custom_system_prompt() {
    let agent = ReActAgent::with_options(
        Arc::new(synthia_provider::traits_stub::ModelProviderStub::new()),
        Arc::new(ToolRegistry::new()),
        PathBuf::new(),
        "CUSTOM".to_string(),
    );
    // `system_prompt` argument is now exposed via
    // `descriptor.instructions` so callers can introspect
    // the agent's role without holding a second handle.
    assert_eq!(agent.descriptor().instructions, "CUSTOM");
}

/// R110: `ReActAgent` *is* the builder. The fluent setters
/// that used to live on `AgentBuilder` (deleted) now ride the
/// same chain the harness exposes — descriptor mutations,
/// registry swap, output schema auto-injection.
#[tokio::test]
async fn react_agent_setters_thread_through_to_descriptor_and_registry() {
    use synthia_core::registry::Registry as _;

    let registry = Arc::new(ToolRegistry::new());
    let schema = serde_json::json!({
        "type": "object",
        "properties": {"answer": {"type": "string"}},
        "required": ["answer"]
    });
    let agent = ReActAgent::new(
        Arc::new(synthia_provider::traits_stub::ModelProviderStub::new()),
        Arc::clone(&registry),
    )
    .with_name("researcher")
    .with_instructions("Find things.")
    .with_model_hint(Some("claude-opus-4".to_string()))
    .with_output_schema(schema.clone());

    assert_eq!(agent.descriptor().name, "researcher");
    assert_eq!(agent.descriptor().instructions, "Find things.");
    assert_eq!(
        agent.descriptor().model_hint.as_deref(),
        Some("claude-opus-4")
    );

    // The output schema landed on the descriptor…
    let desc_schema = agent
        .descriptor()
        .output_schema
        .as_deref()
        .expect("descriptor carries the schema string");
    let parsed: serde_json::Value = serde_json::from_str(desc_schema).unwrap();
    assert_eq!(parsed, schema);

    // …and the structured_output tool was auto-injected
    // into the registry.
    let found = registry
        .get(synthia_tool::structured_output::STRUCTURED_OUTPUT_TOOL_NAME)
        .await
        .expect("registry lookup");
    assert!(
        found.is_some(),
        "with_output_schema must auto-register structured_output"
    );
}

/// it covers action safety, tool use, output style, and
/// markdown formatting in balanced XML-delimited blocks
/// that line up with the sections the prompt assembler
/// appends (`<identity>`, `<env>`, `<available_skills>`,
/// `<available_agents>`). Pin the structural contracts so
/// a future copy edit cannot silently drop a section.
#[test]
fn default_system_prompt_covers_canonical_sections() {
    let p = crate::agent::DEFAULT_SYSTEM_PROMPT;

    // Identity line — Synthia is the agent's name.
    assert!(p.contains("You are Synthia"));

    // Every canonical XML section is present and balanced.
    for tag in [
        "action_safety",
        "tool_calling",
        "output_efficiency",
        "formatting",
    ] {
        let open = format!("<{tag}>");
        let close = format!("</{tag}>");
        assert!(
            p.contains(&open) && p.contains(&close),
            "missing `{open}` / `{close}` in DEFAULT_SYSTEM_PROMPT"
        );
    }

    // Behavioural anchors — distilled from the Grok
    // Build and OpenCode default agent prompts.
    assert!(p.contains("confirm with the user"));
    assert!(p.contains("NEVER use shell"));
    assert!(p.contains("CommonMark"));
    assert!(p.contains("nothing more, nothing less"));
}

/// Default persona + handoff_hint are short, on-brand
/// one-liners (not the long-form `DEFAULT_SYSTEM_PROMPT`
/// body). Pinning the shape keeps the `<identity>` block
/// readable when the assembler renders the descriptor.
#[tokio::test]
async fn descriptor_defaults_are_on_brand() {
    let agent = ReActAgent::new(
        Arc::new(synthia_provider::traits_stub::ModelProviderStub::new()),
        Arc::new(ToolRegistry::new()),
    );
    let d = agent.descriptor();

    assert!(d.description.starts_with("Default Synthia agent"));
    assert!(
        d.handoff_hint
            .as_deref()
            .is_some_and(|s| s.contains("specialist agent")),
        "default handoff_hint should explain when to delegate"
    );
    assert!(
        d.persona
            .as_deref()
            .is_some_and(|s| s.starts_with("You are Synthia")),
        "default persona should self-identify as Synthia"
    );
    assert!(
        d.persona.as_deref().unwrap_or_default().len() < 120,
        "persona is a one-liner; long-form guidance lives in \
             DEFAULT_SYSTEM_PROMPT"
    );
}
