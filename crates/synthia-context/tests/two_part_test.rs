//! Integration tests for `synthia_context::prompt::TwoPartPrompt`.
//!
//! Covers the scenarios from `specs/two-part-prompt/spec.md`:
//! - Single / TwoPart message form assembly
//! - blake3 hash computation
//! - header drift detection across `finalize` calls

use synthia_context::prompt::{
    ModelFamily,
    SystemMessageForm,
    TwoPartPrompt,
    estimate_header_tokens,
};
use synthia_provider::Role;

#[test]
fn build_computes_blake3_hash_and_carries_family() {
    let p = TwoPartPrompt::build("h", "b", ModelFamily::Anthropic);
    let expected_hash = blake3::hash(b"h");
    let expected: [u8; 32] = *expected_hash.as_bytes();
    assert_eq!(p.header_hash, expected);
    assert_eq!(p.model_family, ModelFamily::Anthropic);
    assert_eq!(p.header, "h");
    assert_eq!(p.body, "b");
}

#[test]
fn single_form_produces_one_system_message_with_combined_text() {
    let p = TwoPartPrompt::build("header", "body", ModelFamily::Generic);
    let d = p.finalize(None, SystemMessageForm::Single);
    assert_eq!(d.final_messages.len(), 1);
    assert_eq!(d.final_messages[0].role, Role::System);
    let text = d.final_messages[0].content.extract_text().unwrap();
    assert!(text.contains("header"));
    assert!(text.contains("body"));
}

#[test]
fn two_part_form_produces_two_system_messages() {
    let p = TwoPartPrompt::build("header", "body", ModelFamily::OpenAI);
    let d = p.finalize(None, SystemMessageForm::TwoPart);
    assert_eq!(d.final_messages.len(), 2);
    assert_eq!(d.final_messages[0].role, Role::System);
    assert_eq!(d.final_messages[1].role, Role::System);
    assert_eq!(
        d.final_messages[0].content.extract_text().unwrap(),
        "header"
    );
    assert_eq!(d.final_messages[1].content.extract_text().unwrap(), "body");
}

#[test]
fn first_call_has_no_cache_hit_and_no_unstable_reason() {
    let p = TwoPartPrompt::build("h", "b", ModelFamily::Generic);
    let d = p.finalize(None, SystemMessageForm::TwoPart);
    assert!(!d.cache_hit_expected);
    assert!(d.header_unstable_reason.is_none());
}

#[test]
fn identical_header_across_turns_yields_cache_hit() {
    let p1 = TwoPartPrompt::build("h", "b1", ModelFamily::Generic);
    let d1 = p1.finalize(None, SystemMessageForm::TwoPart);
    let p2 = TwoPartPrompt::build("h", "b2", ModelFamily::Generic);
    let d2 = p2.finalize(Some(d1.header_hash), SystemMessageForm::TwoPart);
    assert!(d2.cache_hit_expected);
    assert!(d2.header_unstable_reason.is_none());
}

#[test]
fn header_drift_signals_unstable() {
    let p1 = TwoPartPrompt::build("h1", "b", ModelFamily::Generic);
    let d1 = p1.finalize(None, SystemMessageForm::TwoPart);
    let p2 = TwoPartPrompt::build("h2", "b", ModelFamily::Generic);
    let d2 = p2.finalize(Some(d1.header_hash), SystemMessageForm::TwoPart);
    assert!(!d2.cache_hit_expected);
    assert!(d2.header_unstable_reason.is_some());
}

#[test]
fn estimate_header_tokens_uses_chars_over_3_5() {
    assert_eq!(estimate_header_tokens(""), 0);
    assert_eq!(estimate_header_tokens(&"x".repeat(7)), 2);
    assert_eq!(estimate_header_tokens(&"x".repeat(7000)), 2000);
}
