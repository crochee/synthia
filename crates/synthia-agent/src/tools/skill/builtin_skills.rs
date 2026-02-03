//! Builtin skills module
//!
//! This module provides builtin skills that are embedded in the binary.

/// Returns all builtin skills as static strings.
pub(super) fn get_all_builtin_skills() -> Vec<&'static str> {
    vec![
        include_str!("skills/skill-creator.md"),
        include_str!("skills/find-skills.md"),
        include_str!("skills/self-improvement/SKILL.md"),
    ]
}
