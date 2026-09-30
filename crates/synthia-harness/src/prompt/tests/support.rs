//! Shared fixtures for the prompt-assembler test suite.
//!
//! Kept in one place so the four test submodules can share
//! them without duplication. The fixtures are deliberately
//! small and self-contained: they build a default-looking
//! `AgentDescriptor` and a peer `AgentDescriptor` so the
//! `assemble` contract is tested against the canonical
//! "Synthia has skills and peer agents" shape the model
//! sees in production.

use crate::{agent::AgentDescriptor, prompt::runtime_context::RuntimeContext};

/// A descriptor populated with the canonical "Synthia has
/// skills and peer agents" shape the model sees in
/// production. Tests mutate the returned value to exercise
/// edge cases (empty instructions, missing display_name,
/// `Some("")` persona, …).
pub(super) fn descriptor_with_instructions(
    instructions: &str,
) -> AgentDescriptor {
    AgentDescriptor {
        name: "agent".into(),
        description: "ReAct loop".into(),
        kind: "react".into(),
        version: "1.0.0".into(),
        instructions: instructions.into(),
        capabilities: vec!["tools".into(), "streaming".into()],
        tools: vec!["read_file".into()],
        model_hint: None,
        handoffs: vec!["planner".into()],
        handoff_hint: Some("Use for code-editing tasks".into()),
        output_schema: None,
        owner: Some("synthia".into()),
        domain: Some("coding".into()),
        persona: Some("You are a pragmatic senior engineer.".into()),
        display_name: None,
        max_iterations: None,
    }
}

/// A bare peer-agent descriptor. Tests pass the resulting
/// value through `PromptContext::with_agent` to exercise the
/// `<available_agents>` renderer.
pub(super) fn peer_descriptor(name: &str) -> AgentDescriptor {
    AgentDescriptor {
        name: name.into(),
        description: "High-level planner.".into(),
        kind: "planner".into(),
        version: "1.0.0".into(),
        instructions: "".into(),
        capabilities: Vec::new(),
        tools: Vec::new(),
        model_hint: None,
        handoffs: Vec::new(),
        handoff_hint: Some("Use for complex tasks.".into()),
        output_schema: None,
        owner: None,
        domain: None,
        persona: None,
        display_name: None,
        max_iterations: None,
    }
}

/// A `RuntimeContext` populated with values that exercise
/// every render branch (git status `Some(true)`, all
/// facts non-empty, model id set). Tests mutate fields to
/// cover the `None` / `Some(false)` paths.
pub(super) fn fixed_runtime_context() -> RuntimeContext {
    RuntimeContext {
        cwd: "/tmp/cwd".into(),
        worktree: "/tmp/worktree".into(),
        is_git_repo: Some(true),
        platform: "linux".into(),
        today: "Mon Jan 1 2026".into(),
        model_id: Some("anthropic/claude-4.6".into()),
    }
}
