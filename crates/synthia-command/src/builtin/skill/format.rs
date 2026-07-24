//! Free formatting helpers shared by [`super::view`],
//! [`super::validate`], and [`super::stats`].
//!
//! Kept `pub(super)` so the action submodules can use them
//! without expanding the public surface.

use super::types::ValidateResult;

pub(super) fn format_skill_source(
    source: &synthia_skill::types::SkillSource,
) -> String {
    match source {
        synthia_skill::types::SkillSource::BuiltIn => "builtin".to_string(),
        synthia_skill::types::SkillSource::Project => "project".to_string(),
        synthia_skill::types::SkillSource::User => "user".to_string(),
    }
}

pub(super) fn format_skill_state(
    state: &synthia_skill::types::SkillState,
) -> String {
    match state {
        synthia_skill::types::SkillState::Loaded => "loaded".to_string(),
        synthia_skill::types::SkillState::Activated => "activated".to_string(),
        synthia_skill::types::SkillState::Disabled => "disabled".to_string(),
    }
}

pub(super) fn format_skill_level(
    level: &synthia_skill::types::SkillLevel,
) -> String {
    match level {
        synthia_skill::types::SkillLevel::Level0 => "0".to_string(),
        synthia_skill::types::SkillLevel::Level1 => "1".to_string(),
        synthia_skill::types::SkillLevel::Level2 => "2".to_string(),
    }
}

pub(super) fn format_validate_output(
    result: &ValidateResult,
    _json_mode: bool,
) -> String {
    let mut output = String::new();
    output.push_str(&format!("Validation: {}\n", result.path));
    output.push_str(&format!("Valid: {}\n", result.valid));

    if !result.errors.is_empty() {
        output.push_str("\nErrors:\n");
        for err in &result.errors {
            output.push_str(&format!("  - {}\n", err));
        }
    }

    if !result.warnings.is_empty() {
        output.push_str("\nWarnings:\n");
        for w in &result.warnings {
            output.push_str(&format!("  - {}\n", w));
        }
    }

    if result.errors.is_empty() && result.warnings.is_empty() {
        output.push_str("No issues found.\n");
    }

    output
}
