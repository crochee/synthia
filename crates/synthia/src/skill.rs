//! [`synthia_skill`] — `SKILL.md` discovery, parsing,
//! and seeding.
//!
//! A skill is a YAML-frontmatter markdown file the agent can apply:
//! [`SkillRegistry`] holds them, `discovery` walks
//! `<workspace>/.agents/skills/**` and the user-level directories,
//! and the seed module writes the built-in workflow skills on first
//! boot without ever overwriting a user-edited file.

pub use synthia_skill::*;
