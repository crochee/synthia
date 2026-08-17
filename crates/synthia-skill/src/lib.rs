//! Skill loader + seeder for the Synthia MVP.
//!
//! A *skill* is a `SKILL.md` file: a YAML frontmatter (required
//! `name`, recommended `description`, optional `metadata` map)
//! followed by a markdown body describing a workflow the agent
//! can apply. Aligned with the Anthropic Agent Skills open
//! standard ([`agentskills.io`](https://agentskills.io/specification))
//! and the OpenCode / Grok Build reference implementations.
//!
//! ## Surfaces
//!
//! - **Discovery** ([`discovery`]): walks
//!   `<workspace>/.agents/skills/**/SKILL.md` (project) +
//!   `$HOME/{.claude,.agents}/skills/**/SKILL.md` (user) and
//!   returns a deduplicated list of [`skill::Skill`] values.
//!   Project skills win when names collide. Mirrors
//!   `opencode/packages/opencode/src/skill/index.ts`.
//! - **Loader** ([`loader`]): parses one SKILL.md into
//!   frontmatter (`types::SkillMetadata`) + body. `name` is
//!   required and must equal the parent directory name;
//!   `description` is optional.
//! - **Canonical value** ([`skill::Skill`]): `name +
//!   description? + location + content` — what the runtime
//!   passes to prompt assembly and the `skill` tool.
//! - **Seeding** ([`seed`]): writes the built-in workflow
//!   skills (code-review / bug-investigation / test-planning)
//!   to `<workspace>/.agents/skills/` on first boot. Never
//!   overwrites user-edited skills.
//! - **HTTP API** (`synthia-server`): CRUD over the
//!   `<workspace>/.agents/skills/` tree, kept independent of
//!   the discovery module because the API has different
//!   caching + reload semantics.
//! - **Agent-facing tool** ([`builtin::skill_tool::SkillTool`]):
//!   single-purpose `{"name": "..."}` tool that returns the
//!   skill body wrapped in the opencode `<skill_content>` XML
//!   envelope. The LLM-facing surface; only this is registered
//!   into [`synthia_tool::ToolRegistry`].

pub mod builtin;
pub mod discovery;
pub mod loader;
pub mod seed;
pub mod skill;
pub mod types;

pub use builtin::skill_tool::{
    SKILL_TOOL_NAME,
    SkillTool,
    register_skill_tool,
};
pub use discovery::{PROJECT_SKILLS_DIR, discover_skills};
pub use loader::SkillLoader;
pub use seed::seed_default_skills;
pub use skill::{Skill, format_skill_content};
pub use types::SkillMetadata;
