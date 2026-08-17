//! Built-in skill surface.
//!
//! Currently hosts the agent-facing `skill` tool
//! ([`skill_tool`]) — the only built-in the MVP runtime needs
//! to expose skills to the LLM.

pub mod skill_tool;

pub use skill_tool::{SKILL_TOOL_NAME, SkillTool, register_skill_tool};
