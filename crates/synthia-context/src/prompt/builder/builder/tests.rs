//! Unit tests for [`PromptBuilder`].
//!
//! Coverage map (19 tests):
//!
//! - Construction: 3 tests
//!   ([`PromptBuilder::new`] empty, [`PromptBuilder::add_section`] appends,
//!   [`PromptBuilder::default_with_sections`] includes identity + system).
//! - [`super::resolve::resolve`]: 6 tests
//!   (empty builder, Cached section → static, Volatile → dynamic,
//!   SessionCached → dynamic, Uncached → dynamic and not stored in state,
//!   empty-content sections are skipped, mixed Cached+Volatile).
//! - [`super::resolve::validate_prefix_stability`]: 4 tests
//!   (no previous → true, exact match, mismatch → false, ignores non-Cached).
//! - Getters: 2 tests
//!   ([`PromptBuilder::get_static_sections`] returns Cached only,
//!   [`PromptBuilder::get_dynamic_sections`] returns the rest).
//! - [`super::effective::build_effective_prompt`]: 5 tests
//!   (override, coordinator_mode, normal flow, agent+custom prepends,
//!   append only).
//! - Debug: 1 test.

use super::{
    super::{
        config::EffectivePromptConfig,
        state::PromptState,
        test_support::{MockSection, make_test_context},
    },
    *,
};
use crate::prompt::section_trait::SectionCaching;

// =============================================================================
// Construction Tests
// =============================================================================

#[test]
fn test_prompt_builder_new() {
    let builder = PromptBuilder::new();
    assert!(builder.section_names().is_empty());
}

#[test]
fn test_prompt_builder_add_section() {
    let builder = PromptBuilder::new().add_section(Box::new(MockSection::new(
        "test",
        SectionCaching::Cached,
        "content",
    )));
    assert_eq!(builder.section_names(), vec!["test"]);
}

#[test]
fn test_prompt_builder_default_with_sections() {
    let builder = PromptBuilder::default_with_sections();
    let names = builder.section_names();
    assert!(names.contains(&"identity"));
    assert!(names.contains(&"system"));
}

// =============================================================================
// resolve() Tests
// =============================================================================

#[test]
fn test_prompt_builder_resolve_empty() {
    let builder = PromptBuilder::new();
    let ctx = make_test_context();
    let mut state = PromptState::new();
    let resolved = builder.resolve(&ctx, &mut state).unwrap();
    assert!(resolved.static_content.is_empty());
    assert!(resolved.dynamic_content.is_empty());
    assert!(resolved.sections_used.is_empty());
}

#[test]
fn test_prompt_builder_resolve_cached_section() {
    let builder = PromptBuilder::new().add_section(Box::new(MockSection::new(
        "s1",
        SectionCaching::Cached,
        "cached-content",
    )));
    let ctx = make_test_context();
    let mut state = PromptState::new();
    let resolved = builder.resolve(&ctx, &mut state).unwrap();
    assert!(resolved.static_content.contains("cached-content"));
    assert!(resolved.dynamic_content.is_empty());
    assert_eq!(resolved.sections_used, vec!["s1".to_string()]);
}

#[test]
fn test_prompt_builder_resolve_volatile_section() {
    let builder = PromptBuilder::new().add_section(Box::new(MockSection::new(
        "v1",
        SectionCaching::Volatile,
        "volatile-content",
    )));
    let ctx = make_test_context();
    let mut state = PromptState::new();
    let resolved = builder.resolve(&ctx, &mut state).unwrap();
    assert!(resolved.dynamic_content.contains("volatile-content"));
    assert!(resolved.static_content.is_empty());
}

#[test]
fn test_prompt_builder_resolve_session_cached_section() {
    let builder = PromptBuilder::new().add_section(Box::new(MockSection::new(
        "sc1",
        SectionCaching::SessionCached,
        "sc-content",
    )));
    let ctx = make_test_context();
    let mut state = PromptState::new();
    let resolved = builder.resolve(&ctx, &mut state).unwrap();
    assert!(resolved.dynamic_content.contains("sc-content"));
}

#[test]
fn test_prompt_builder_resolve_uncached_section() {
    let builder = PromptBuilder::new().add_section(Box::new(MockSection::new(
        "u1",
        SectionCaching::Uncached,
        "u-content",
    )));
    let ctx = make_test_context();
    let mut state = PromptState::new();
    let resolved = builder.resolve(&ctx, &mut state).unwrap();
    assert!(resolved.dynamic_content.contains("u-content"));
    // Uncached must not be stored in state
    assert_eq!(state.get("u1", SectionCaching::Uncached), None);
}

#[test]
fn test_prompt_builder_resolve_empty_section_skipped() {
    let builder = PromptBuilder::new()
        .add_section(Box::new(MockSection::new(
            "empty",
            SectionCaching::Cached,
            "   ",
        )))
        .add_section(Box::new(MockSection::new(
            "real",
            SectionCaching::Cached,
            "real-content",
        )));
    let ctx = make_test_context();
    let mut state = PromptState::new();
    let resolved = builder.resolve(&ctx, &mut state).unwrap();
    assert!(!resolved.sections_used.contains(&"empty".to_string()));
    assert!(resolved.sections_used.contains(&"real".to_string()));
}

#[test]
fn test_prompt_builder_resolve_multiple_cached_sections() {
    let builder = PromptBuilder::new()
        .add_section(Box::new(MockSection::new(
            "a",
            SectionCaching::Cached,
            "A",
        )))
        .add_section(Box::new(MockSection::new(
            "b",
            SectionCaching::Cached,
            "B",
        )));
    let ctx = make_test_context();
    let mut state = PromptState::new();
    let resolved = builder.resolve(&ctx, &mut state).unwrap();
    assert!(resolved.static_content.contains("A"));
    assert!(resolved.static_content.contains("B"));
}

#[test]
fn test_prompt_builder_resolve_mixed_sections() {
    let builder = PromptBuilder::new()
        .add_section(Box::new(MockSection::new(
            "static",
            SectionCaching::Cached,
            "S",
        )))
        .add_section(Box::new(MockSection::new(
            "dynamic",
            SectionCaching::Volatile,
            "D",
        )));
    let ctx = make_test_context();
    let mut state = PromptState::new();
    let resolved = builder.resolve(&ctx, &mut state).unwrap();
    assert!(resolved.static_content.contains("S"));
    assert!(resolved.dynamic_content.contains("D"));
}

// =============================================================================
// validate_prefix_stability() Tests
// =============================================================================

#[test]
fn test_prompt_builder_validate_prefix_stability_no_previous() {
    let builder = PromptBuilder::new();
    let ctx = make_test_context();
    let state = PromptState::new();
    let stable = builder
        .validate_prefix_stability(&ctx, &state, None)
        .unwrap();
    assert!(stable);
}

#[test]
fn test_prompt_builder_validate_prefix_stability_match() {
    let builder = PromptBuilder::new().add_section(Box::new(MockSection::new(
        "s",
        SectionCaching::Cached,
        "content",
    )));
    let ctx = make_test_context();
    let mut state = PromptState::new();
    let resolved = builder.resolve(&ctx, &mut state).unwrap();
    let stable = builder
        .validate_prefix_stability(&ctx, &state, Some(&resolved.static_hash))
        .unwrap();
    assert!(stable);
}

#[test]
fn test_prompt_builder_validate_prefix_stability_mismatch() {
    let builder = PromptBuilder::new().add_section(Box::new(MockSection::new(
        "s",
        SectionCaching::Cached,
        "content",
    )));
    let ctx = make_test_context();
    let state = PromptState::new();
    let stable = builder
        .validate_prefix_stability(&ctx, &state, Some("bogus-hash"))
        .unwrap();
    assert!(!stable);
}

#[test]
fn test_prompt_builder_validate_prefix_stability_ignores_non_cached() {
    let builder = PromptBuilder::new()
        .add_section(Box::new(MockSection::new(
            "cached",
            SectionCaching::Cached,
            "c",
        )))
        .add_section(Box::new(MockSection::new(
            "volatile",
            SectionCaching::Volatile,
            "v",
        )));
    let ctx = make_test_context();
    let state = PromptState::new();
    // Compute a hash that matches only the cached part
    use std::{
        collections::hash_map::DefaultHasher,
        hash::{Hash, Hasher},
    };
    let mut hasher = DefaultHasher::new();
    "c".hash(&mut hasher);
    let hash = format!("{:x}", hasher.finish());
    let stable = builder
        .validate_prefix_stability(&ctx, &state, Some(&hash))
        .unwrap();
    assert!(stable);
}

// =============================================================================
// Getters Tests
// =============================================================================

#[test]
fn test_prompt_builder_get_static_sections() {
    let builder = PromptBuilder::new()
        .add_section(Box::new(MockSection::new(
            "c",
            SectionCaching::Cached,
            "x",
        )))
        .add_section(Box::new(MockSection::new(
            "v",
            SectionCaching::Volatile,
            "x",
        )));
    let static_sections = builder.get_static_sections();
    assert!(static_sections.contains(&"c"));
    assert!(!static_sections.contains(&"v"));
}

#[test]
fn test_prompt_builder_get_dynamic_sections() {
    let builder = PromptBuilder::new()
        .add_section(Box::new(MockSection::new(
            "c",
            SectionCaching::Cached,
            "x",
        )))
        .add_section(Box::new(MockSection::new(
            "v",
            SectionCaching::Volatile,
            "x",
        )));
    let dynamic_sections = builder.get_dynamic_sections();
    assert!(!dynamic_sections.contains(&"c"));
    assert!(dynamic_sections.contains(&"v"));
}

// =============================================================================
// build_effective_prompt() Tests
// =============================================================================

#[test]
fn test_prompt_builder_build_effective_prompt_override() {
    let builder = PromptBuilder::new();
    let ctx = make_test_context();
    let mut state = PromptState::new();
    let config =
        EffectivePromptConfig::new().with_override("OVERRIDE".to_string());
    let prompt = builder
        .build_effective_prompt(&ctx, &mut state, config)
        .unwrap();
    assert_eq!(prompt, "OVERRIDE");
}

#[test]
fn test_prompt_builder_build_effective_prompt_coordinator_mode() {
    let builder = PromptBuilder::new();
    let ctx = make_test_context();
    let mut state = PromptState::new();
    let config = EffectivePromptConfig::new()
        .with_coordinator("COORD".to_string())
        .with_append("APPEND".to_string())
        .with_coordinator_mode(true);
    let prompt = builder
        .build_effective_prompt(&ctx, &mut state, config)
        .unwrap();
    assert!(prompt.contains("COORD"));
    assert!(prompt.contains("APPEND"));
}

#[test]
fn test_prompt_builder_build_effective_prompt_normal_flow() {
    let builder = PromptBuilder::new().add_section(Box::new(MockSection::new(
        "s",
        SectionCaching::Cached,
        "S",
    )));
    let ctx = make_test_context();
    let mut state = PromptState::new();
    let prompt = builder
        .build_effective_prompt(&ctx, &mut state, EffectivePromptConfig::new())
        .unwrap();
    assert!(prompt.contains("S"));
}

#[test]
fn test_prompt_builder_build_effective_prompt_with_agent_and_custom() {
    let builder = PromptBuilder::new().add_section(Box::new(MockSection::new(
        "s",
        SectionCaching::Cached,
        "S",
    )));
    let ctx = make_test_context();
    let mut state = PromptState::new();
    let config = EffectivePromptConfig::new()
        .with_agent("AGENT".to_string())
        .with_custom("CUSTOM".to_string());
    let prompt = builder
        .build_effective_prompt(&ctx, &mut state, config)
        .unwrap();
    assert!(prompt.contains("AGENT"));
    assert!(prompt.contains("CUSTOM"));
}

#[test]
fn test_prompt_builder_build_effective_prompt_append_only() {
    let builder = PromptBuilder::new().add_section(Box::new(MockSection::new(
        "s",
        SectionCaching::Cached,
        "S",
    )));
    let ctx = make_test_context();
    let mut state = PromptState::new();
    let config = EffectivePromptConfig::new().with_append("APPEND".to_string());
    let prompt = builder
        .build_effective_prompt(&ctx, &mut state, config)
        .unwrap();
    assert!(prompt.contains("S"));
    assert!(prompt.contains("APPEND"));
}

// =============================================================================
// Debug Test
// =============================================================================

#[test]
fn test_prompt_builder_debug_trait() {
    let builder = PromptBuilder::new().add_section(Box::new(MockSection::new(
        "test",
        SectionCaching::Cached,
        "content",
    )));
    let debug = format!("{builder:?}");
    assert!(debug.contains("PromptBuilder"));
    assert!(debug.contains("sections_count"));
}
