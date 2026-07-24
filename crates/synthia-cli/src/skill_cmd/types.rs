//! Data carriers for the skill CLI.
//!
//! Currently a single struct ([`LoadedSkill`]) that
//! combines a [`SkillMetadata`] with the
//! CLI-display-only fields ([`source`] string,
//! [`token_count`], [`body_length`]). The struct is
//! private to the `skill_cmd` module — it is not
//! returned to callers; the public entry points
//! ([`super::list_skills`], [`super::show_skill_info`],
//! etc.) just `println!` it.
//!
//! [`source`]: LoadedSkill::source
//! [`token_count`]: LoadedSkill::token_count
//! [`body_length`]: LoadedSkill::body_length

use synthia_skill::types::{SkillMetadata, SkillTokenCount};

/// Loaded skill with computed metadata for CLI display.
pub(super) struct LoadedSkill {
    /// Parsed YAML frontmatter from `SKILL.md`.
    pub(super) metadata: SkillMetadata,
    /// Human-readable source label (`builtin` / `project`
    /// / `user`).
    pub(super) source: String,
    /// L0/L1 token estimates.
    pub(super) token_count: SkillTokenCount,
    /// Raw `SKILL.md` body length, in characters.
    pub(super) body_length: usize,
}
