//! Read-only skill queries — `list_skills` + `info_skill`.
//!
//! Both actions gracefully degrade when the registry is
//! `None` (empty list / "not found (no registry loaded)").
//! The `--json` mode uses the [`SkillListEntry`] /
//! [`SkillInfoOutput`] shapes from [`super::types`].

use synthia_core::Error;

use super::{
    construct::SkillCommand,
    format::{format_skill_level, format_skill_source, format_skill_state},
    types::{SkillInfoOutput, SkillListEntry},
};
use crate::types::CommandResult;

impl SkillCommand {
    pub(super) fn list_skills(
        &self,
        json_mode: bool,
    ) -> Result<CommandResult, Error> {
        match &self.registry {
            Some(registry) => {
                let skills = registry.list_skills();
                if skills.is_empty() {
                    if json_mode {
                        Ok(CommandResult::new("[]"))
                    } else {
                        Ok(CommandResult::new(
                            "No skills loaded. Place SKILL.md files in .agents/skills/ to add skills.",
                        ))
                    }
                } else {
                    let entries: Vec<SkillListEntry> = {
                        let skills_map = registry.get_skill_map();
                        skills_map
                            .iter()
                            .map(|(name, skill)| SkillListEntry {
                                name: name.clone(),
                                source: format_skill_source(&skill.source),
                                state: format_skill_state(&skill.state),
                                token_count: skill.token_count.level0
                                    + skill.token_count.level1,
                            })
                            .collect()
                    };

                    if json_mode {
                        serde_json::to_string_pretty(&entries)
                            .map(CommandResult::new)
                            .map_err(|e| {
                                Error::ToolExecution(format!(
                                    "JSON serialization failed: {}",
                                    e
                                ))
                            })
                    } else {
                        let mut output = String::from("Available skills:\n");
                        for entry in &entries {
                            output.push_str(&format!(
                                "  - {} [{}] ({}) - {} tokens\n",
                                entry.name,
                                entry.source,
                                entry.state,
                                entry.token_count
                            ));
                        }
                        Ok(CommandResult::new(output))
                    }
                }
            }
            None => {
                if json_mode {
                    Ok(CommandResult::new("[]"))
                } else {
                    Ok(CommandResult::new(
                        "Available skills:\n\
                         (No skills loaded yet. Place SKILL.md files in .agents/skills/ to add skills.)",
                    ))
                }
            }
        }
    }

    pub(super) fn info_skill(
        &self,
        name: &str,
        json_mode: bool,
    ) -> Result<CommandResult, Error> {
        match &self.registry {
            Some(registry) => match registry.get_skill_sync(name) {
                Ok(skill) => {
                    let info = SkillInfoOutput {
                        name: skill.metadata.name.clone(),
                        description: skill.metadata.description.clone(),
                        source: format_skill_source(&skill.source),
                        state: format_skill_state(&skill.state),
                        level: format_skill_level(&skill.level),
                        token_count_level0: skill.token_count.level0,
                        token_count_level1: skill.token_count.level1,
                        version: skill.metadata.version.clone(),
                        license: skill.metadata.license.clone(),
                        tags: skill.metadata.tags.clone(),
                        triggers: skill.metadata.triggers.clone(),
                        allowed_tools: skill.metadata.allowed_tools.clone(),
                        priority: skill.metadata.priority,
                        has_exec_scripts: skill.metadata.exec.is_some()
                            && !skill
                                .metadata
                                .exec
                                .as_ref()
                                .unwrap()
                                .is_empty(),
                    };

                    if json_mode {
                        serde_json::to_string_pretty(&info)
                            .map(CommandResult::new)
                            .map_err(|e| {
                                Error::ToolExecution(format!(
                                    "JSON serialization failed: {}",
                                    e
                                ))
                            })
                    } else {
                        let mut output = String::new();
                        output.push_str(&format!("Skill: {}\n", info.name));
                        output.push_str(&format!(
                            "Description: {}\n",
                            info.description
                        ));
                        output.push_str(&format!("Source: {}\n", info.source));
                        output.push_str(&format!("State: {}\n", info.state));
                        output.push_str(&format!("Level: {}\n", info.level));
                        output.push_str(&format!(
                            "Token count (L0/L1): {} / {}\n",
                            info.token_count_level0, info.token_count_level1
                        ));
                        if let Some(v) = &info.version {
                            output.push_str(&format!("Version: {}\n", v));
                        }
                        if let Some(l) = &info.license {
                            output.push_str(&format!("License: {}\n", l));
                        }
                        if !info.triggers.is_empty() {
                            output.push_str(&format!(
                                "Triggers: {}\n",
                                info.triggers.join(", ")
                            ));
                        }
                        if !info.tags.is_empty() {
                            output.push_str(&format!(
                                "Tags: {}\n",
                                info.tags.join(", ")
                            ));
                        }
                        if !info.allowed_tools.is_empty() {
                            output.push_str(&format!(
                                "Allowed tools: {}\n",
                                info.allowed_tools.join(", ")
                            ));
                        }
                        output.push_str(&format!(
                            "Priority: {}\n",
                            info.priority
                        ));
                        output.push_str(&format!(
                            "Has exec scripts: {}\n",
                            info.has_exec_scripts
                        ));
                        Ok(CommandResult::new(output))
                    }
                }
                Err(_) => Ok(CommandResult::new(format!(
                    "Skill '{}' not found.",
                    name
                ))),
            },
            None => Ok(CommandResult::new(format!(
                "Skill '{}' not found (no registry loaded).",
                name
            ))),
        }
    }
}
