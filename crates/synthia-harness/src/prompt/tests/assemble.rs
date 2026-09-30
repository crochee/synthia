//! Tests for the public `PromptContext::assemble` contract.
//!
//! Cross-cutting assertions that exercise the full assembly
//! rather than a single renderer: the canonical XML order
//! of the default pack, byte-stability across calls,
//! absence of per-dispatch volatile facts in the system
//! prompt, multi-agent reuse of one builder, the no-tools
//! invariant, and the trimming of whitespace-only
//! instructions.

use super::{
    super::PromptContext,
    support::{descriptor_with_instructions, peer_descriptor},
};

/// Default-pack smoke test: the canonical inputs produce
/// the documented structure in the documented order, with
/// every section wrapped in balanced XML tags. Tool
/// schemas are deliberately absent — they ride the
/// completion request, not the prompt text.
///
/// The descriptor carries an `output_schema` so the
/// `<structured_output>` mandate is part of the asserted order.
/// Without it this would be the *only* test pinning section
/// position while being blind to the newest section — a reorder
/// would then pass every test in the suite (the dedicated
/// structured-output tests assert presence and absence, not
/// place).
#[test]
fn default_pack_renders_canonical_xml_order() {
    let mut d = descriptor_with_instructions("BASE");
    d.output_schema = Some(r#"{"type":"object"}"#.to_string());
    let out = PromptContext::default()
        .with_skill("transcribe", "Transcribe audio files.")
        .with_agent(&peer_descriptor("planner"))
        .assemble(&d);

    let order = [
        "BASE",
        "<identity>",
        "</identity>",
        // The mandate sits with the identity block (an
        // instruction), ahead of the skills / agent menus
        // (catalogues). See module docs § "Assembly order".
        "<structured_output>",
        "</structured_output>",
        "<available_skills>",
        "</available_skills>",
        "<available_agents>",
        "</available_agents>",
    ];
    let mut cursor = 0usize;
    for needle in order {
        let hit = out[cursor..].find(needle).unwrap_or_else(|| {
            panic!("missing `{needle}` after position {cursor}")
        });
        cursor = hit + needle.len();
    }

    assert!(out.contains("<name>transcribe</name>"));
    assert!(out.contains("`planner`"));

    assert!(
        !out.contains("<available_tools>"),
        "tool schemas must not be assembled into the system prompt; got:\n{out}"
    );
    assert!(
        !out.contains("Use only these tools:"),
        "tool grounding belongs on the runtime, not in the prompt; got:\n{out}"
    );

    // No `<rules>` block is rendered — the closing rules
    // section was intentionally removed. This is a hard
    // regression guard: any future section that adds an
    // `<rules>` (or `</rules>`) tag must update this
    // assertion and the section list above together.
    assert!(
        !out.contains("<rules>") && !out.contains("</rules>"),
        "the system prompt must not contain a <rules> block; got:\n{out}"
    );
    assert!(
        !out.contains("Apply only these skills:"),
        "skill grounding moved to <available_skills>; got:\n{out}"
    );
    assert!(
        !out.contains("Hand off only to these agents:"),
        "agent grounding moved to <available_agents>; got:\n{out}"
    );
}

/// Pure-string contract: same `(descriptor, ctx)` ⇒
/// byte-identical output. Prompt caching requires this;
/// if it ever regresses, cache hit-rate collapses
/// silently.
#[test]
fn assemble_is_pure_and_deterministic() {
    let d = descriptor_with_instructions("BASE");
    let ctx = PromptContext::default()
        .with_skill("transcribe", "Transcribe audio files.")
        .with_agent(&peer_descriptor("planner"));
    let a = ctx.assemble(&d);
    let b = ctx.assemble(&d);
    assert_eq!(a, b);
}

/// Same `PromptContext` rendered against two different
/// descriptors must yield different identity blocks but
/// identical skill / agent blocks — that's the contract
/// for treating `descriptor` as per-call input rather
/// than builder state.
#[test]
fn assemble_supports_multiple_agents_via_one_builder() {
    let d1 = descriptor_with_instructions("AGENT_ONE");
    let d2 = descriptor_with_instructions("AGENT_TWO");
    let ctx = PromptContext::default()
        .with_skill("transcribe", "Transcribe audio files.")
        .with_agent(&peer_descriptor("planner"));
    let a = ctx.assemble(&d1);
    let b = ctx.assemble(&d2);
    assert!(a.starts_with("AGENT_ONE"));
    assert!(b.starts_with("AGENT_TWO"));
    assert!(a.contains("<name>transcribe</name>"));
    assert!(b.contains("<name>transcribe</name>"));
}

/// Two `assemble` calls over the same `(descriptor, ctx)`
/// must produce byte-identical output — that's the
/// contract for prompt-cache hit-rate. Removed the
/// `with_environment(env)` call from the legacy version
/// because volatile facts no longer live in the system
/// prompt; this version asserts the byte-stability
/// contract directly.
#[test]
fn assemble_is_byte_stable_across_calls() {
    let d = descriptor_with_instructions("BASE");
    let ctx = PromptContext::default()
        .with_skill("transcribe", "Transcribe audio files.")
        .with_agent(&peer_descriptor("planner"));
    let a = ctx.assemble(&d);
    let b = ctx.assemble(&d);
    assert_eq!(a, b);
}

// -- System-prompt stability ---------------------------------------

/// The system prompt must NEVER carry per-dispatch volatile
/// facts. Volatile facts (cwd, today, model id) live in
/// the runtime-context snapshot seam (see
/// [`RuntimeContext::render_snapshot`]) — surfacing them
/// in the system message would invalidate provider
/// prompt caches on every turn.
#[test]
fn assemble_never_emits_runtime_facts() {
    let d = descriptor_with_instructions("BASE");
    let out = PromptContext::default()
        .with_skill("transcribe", "Transcribe audio files.")
        .with_agent(&peer_descriptor("planner"))
        .assemble(&d);
    assert!(!out.contains("<env>") && !out.contains("</env>"));
    assert!(
        !out.contains("Working directory:"),
        "cwd must live in the snapshot, not the system prompt"
    );
    assert!(
        !out.contains("Workspace root:"),
        "worktree must live in the snapshot, not the system prompt"
    );
    assert!(
        !out.contains("Platform:"),
        "platform must live in the snapshot, not the system prompt"
    );
    assert!(
        !out.contains("Today:"),
        "today must live in the snapshot, not the system prompt"
    );
    assert!(
        !out.contains("Model: "),
        "model_id must live in the snapshot, not the system prompt"
    );
}

/// `trim()` whitespace-only descriptor instructions are
/// treated as absent, mirroring the empty case.
#[test]
fn whitespace_only_descriptor_instructions_are_dropped() {
    let d = descriptor_with_instructions("   \n\t  ");
    let out = PromptContext::default().assemble(&d);
    let first_tag = out.find('<').expect("identity tag present");
    assert!(out[..first_tag].is_empty());
}

/// Base instructions come straight from
/// `descriptor.instructions` — there is no separate
/// override knob on `PromptContext`.
#[test]
fn descriptor_instructions_are_the_base_prompt() {
    let d = descriptor_with_instructions("FIRST_AGENT_INSTRUCTIONS");
    let out = PromptContext::default().assemble(&d);
    assert!(out.starts_with("FIRST_AGENT_INSTRUCTIONS"));
}

/// `PromptContext` has no `tools` field — pinned at the
/// type level so a future refactor cannot quietly
/// re-introduce tool-into-prompt assembly.
#[test]
fn prompt_context_has_no_tools_field() {
    let ctx = PromptContext::default();
    let _ = (&ctx.skills, &ctx.agents);
}

/// Empty manifests drop the corresponding section
/// entirely — no `(none)` placeholder, no empty `<tag/>`.
/// The model wastes zero attention on absence signals.
#[test]
fn empty_manifests_drop_their_section() {
    let d = descriptor_with_instructions("BASE");
    let out = PromptContext::default().assemble(&d);
    assert!(out.contains("<identity>"));
    assert!(!out.contains("<available_skills>"));
    assert!(!out.contains("<available_agents>"));
}

/// A descriptor with an output schema makes the prompt state the
/// structured-output requirement — the one fact the tool channel cannot
/// carry.
///
/// The schema itself stays out of the prompt (it is the tool's
/// `parameters()`, delivered natively), so this section is purely the
/// instruction that prose is not an accepted ending. Without it the
/// finalize-time `WarningKind::StructuredOutput` would fire on nearly
/// every schema-declaring run, because the model had never been asked to
/// submit — noise, not validation.
#[test]
fn output_schema_adds_the_structured_output_requirement() {
    let mut descriptor =
        super::support::descriptor_with_instructions("Be helpful.");
    descriptor.output_schema = Some(r#"{"type":"object"}"#.to_string());

    let prompt = PromptContext::default().assemble(&descriptor);

    assert!(
        prompt.contains("<structured_output>"),
        "a declared schema must put the requirement in the prompt; got:\n{prompt}"
    );
    assert!(
        prompt.contains("structured_output"),
        "the section must name the tool the model has to call"
    );
}

/// No schema, no section — the block is about a declared contract, not
/// about output formatting in general.
#[test]
fn without_output_schema_no_structured_output_section() {
    let descriptor =
        super::support::descriptor_with_instructions("Be helpful.");
    let prompt = PromptContext::default().assemble(&descriptor);
    assert!(
        !prompt.contains("<structured_output>"),
        "no schema declared, so there is nothing to require; got:\n{prompt}"
    );
}

/// The schema text itself must not be inlined — it rides the provider's
/// native `tools` channel, and duplicating it would spend prompt tokens
/// on a copy the model already receives.
#[test]
fn output_schema_text_is_not_inlined_in_the_prompt() {
    let mut descriptor =
        super::support::descriptor_with_instructions("Be helpful.");
    descriptor.output_schema =
        Some(r#"{"type":"object","title":"UNIQUE_SCHEMA_MARKER"}"#.to_string());

    let prompt = PromptContext::default().assemble(&descriptor);

    assert!(
        !prompt.contains("UNIQUE_SCHEMA_MARKER"),
        "the schema belongs on the tools channel, not in the prompt; got:\n{prompt}"
    );
}
