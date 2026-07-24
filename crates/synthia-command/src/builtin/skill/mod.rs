//! Skill command — `/skill <list|info|validate|stats|report|enable|disable|install|uninstall>`.
//!
//! Submodule layout:
//!
//! - [`types`]: the 5 `#[derive(Serialize)]` output structs
//!   ([`SkillListEntry`], [`SkillInfoOutput`], [`ValidateResult`],
//!   [`SkillStatsOutput`], [`SkillReportOutput`]).
//! - [`construct`]: the [`SkillCommand`] struct, its `new` /
//!   `with_*` / `Default` impls.
//! - [`dispatch`]: the `CommandHandler` impl that parses the
//!   raw args string and routes to one of the action modules.
//! - [`view`]: read-only queries — `list_skills` and
//!   `info_skill`.
//! - [`validate`]: `validate_skill` — checks a SKILL.md file
//!   against `SkillLoader::parse_frontmatter`.
//! - [`stats`]: `global_stats` — aggregate statistics from the
//!   registry + usage tracker.
//! - [`report`]: `skill_report` — per-skill usage record.
//! - [`lifecycle`]: state-mutating actions — `enable_skill`,
//!   `disable_skill`, `install_skill`, `uninstall_skill`.
//! - [`format`]: free formatting helpers used by [`view`],
//!   [`validate`], and [`stats`] (the `format_skill_source` /
//!   `format_skill_state` / `format_skill_level` /
//!   `format_validate_output` family).
//!
//! Unit tests live in [`tests`].

mod construct;
mod dispatch;
mod format;
mod lifecycle;
mod report;
mod stats;
mod types;
mod validate;
mod view;

#[cfg(test)]
mod tests;

pub use construct::SkillCommand;
