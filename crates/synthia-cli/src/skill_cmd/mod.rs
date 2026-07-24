//! Skill CLI subcommands.
//!
//! The original 807-line `skill_cmd.rs` was split into
//! focused submodules by responsibility:
//!
//! - [`types`]: the internal [`types::LoadedSkill`]
//!   data carrier (private to the `skill_cmd` module).
//! - [`discover`]: skill directory discovery +
//!   frontmatter-only loading. Used by `view` and
//!   `report`.
//! - [`view`]: read-only display commands
//!   ([`view::list_skills`], [`view::show_skill_info`],
//!   [`view::list_installed_skills`]).
//! - [`validate`]: the `validate` subcommand
//!   ([`validate::validate_skill`]).
//! - [`lifecycle`]: the only write paths —
//!   [`lifecycle::install_skill`] and
//!   [`lifecycle::uninstall_skill`].
//! - [`report`]: usage statistics —
//!   [`report::show_skill_stats`] and
//!   [`report::show_skill_report`].
//!
//! The 19 unit tests live in [`tests`].

mod discover;
mod lifecycle;
mod report;
mod types;
mod validate;
mod view;

#[allow(clippy::module_inception)]
#[cfg(test)]
mod tests;

pub use lifecycle::{install_skill, uninstall_skill};
pub use report::{show_skill_report, show_skill_stats};
pub use validate::validate_skill;
pub use view::{list_installed_skills, list_skills, show_skill_info};
