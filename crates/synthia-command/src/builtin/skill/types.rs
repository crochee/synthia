//! `#[derive(Serialize)]` output structs for each skill action.
//!
//! These are the JSON shapes produced by `--json` mode and are
//! the only stable contract between the command and downstream
//! tooling. Field names match the user-visible `/skill
//! <action> --json` output 1:1.

use serde::Serialize;

#[derive(Serialize)]
pub(crate) struct SkillListEntry {
    pub(crate) name: String,
    pub(crate) source: String,
    pub(crate) state: String,
    pub(crate) token_count: usize,
}

#[derive(Serialize)]
pub(crate) struct SkillInfoOutput {
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) source: String,
    pub(crate) state: String,
    pub(crate) level: String,
    pub(crate) token_count_level0: usize,
    pub(crate) token_count_level1: usize,
    pub(crate) version: Option<String>,
    pub(crate) license: Option<String>,
    pub(crate) tags: Vec<String>,
    pub(crate) triggers: Vec<String>,
    pub(crate) allowed_tools: Vec<String>,
    pub(crate) priority: i32,
    pub(crate) has_exec_scripts: bool,
}

#[derive(Serialize)]
pub(crate) struct ValidateResult {
    pub(crate) path: String,
    pub(crate) valid: bool,
    pub(crate) errors: Vec<String>,
    pub(crate) warnings: Vec<String>,
}

#[derive(Serialize)]
pub(crate) struct SkillStatsOutput {
    pub(crate) loaded_skills: usize,
    pub(crate) total_token_usage: usize,
    pub(crate) match_failure_rate: f64,
}

#[derive(Serialize)]
pub(crate) struct SkillReportOutput {
    pub(crate) skill_name: String,
    pub(crate) match_count: usize,
    pub(crate) activation_count: usize,
    pub(crate) estimated_token_cost: usize,
    pub(crate) last_matched: Option<String>,
    pub(crate) last_activated: Option<String>,
}
