//! Tests for the `<identity>` renderer.
//!
//! Covers the opening line's `display_name` ↔ `name`
//! fallback, the "instructions empty ⇒ manifest still
//! rendered" path, and the "empty `Some` on persona /
//! domain ⇒ no stray line" path.

use super::{
    super::PromptContext,
    support::{descriptor_with_instructions, peer_descriptor},
};

/// The `<identity>` opening line must use the descriptor's
/// `display_name` (the human-readable label) when set,
/// and fall back to the programmatic `name` slug
/// otherwise. Pins the contract that the model
/// self-identifies with the persona the user sees on
/// the UI card, not the internal routing id.
#[test]
fn identity_line_uses_display_name_when_set() {
    let mut d = descriptor_with_instructions("BASE");
    d.display_name = Some("Synthia".into());
    let out = PromptContext::default().assemble(&d);
    assert!(
        out.contains("You are `Synthia` (react v1.0.0)"),
        "identity line must use display_name; got:\n{out}"
    );
    assert!(
        !out.contains("You are `react`"),
        "identity line must not leak the routing slug; got:\n{out}"
    );
}

/// Without `display_name`, the assembler falls back to
/// the programmatic `name` slug so legacy descriptors
/// keep rendering the same identity they did before the
/// field was added.
#[test]
fn identity_line_falls_back_to_name_when_display_name_missing() {
    let d = descriptor_with_instructions("BASE");
    // `display_name` is `None` on the test fixture.
    let out = PromptContext::default().assemble(&d);
    assert!(
        out.contains("You are `agent` (react v1.0.0)"),
        "missing display_name must fall back to name; got:\n{out}"
    );
}

/// Empty descriptor instructions still injects the
/// manifest. The manifest sections carry the model's
/// skills / peers regardless of whether the running
/// agent's own prose is empty.
#[test]
fn empty_descriptor_instructions_still_injects_manifest() {
    let d = descriptor_with_instructions("");
    let out = PromptContext::default()
        .with_skill("transcribe", "Transcribe audio files.")
        .with_agent(&peer_descriptor("planner"))
        .assemble(&d);
    assert!(out.contains("<identity>"));
    assert!(out.contains("<available_skills>"));
    assert!(out.contains("<available_agents>"));
}

/// `Some("")` persona / domain must NOT render a stray
/// `Domain:` / `Persona:` line.
#[test]
fn identity_skips_empty_optional_fields() {
    let mut d = descriptor_with_instructions("");
    d.persona = Some(String::new());
    d.domain = Some(String::new());
    let out = PromptContext::default().assemble(&d);
    assert!(!out.contains("Domain: "));
    assert!(!out.contains("Persona: "));
}
