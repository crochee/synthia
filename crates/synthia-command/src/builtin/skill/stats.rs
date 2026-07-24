//! `global_stats` — aggregate statistics from the registry
//! (loaded count + total token usage) and the usage tracker
//! (match failure rate = 1 - activation/match).
//!
//! JSON mode uses [`SkillStatsOutput`].

use synthia_core::Error;

use super::{construct::SkillCommand, types::SkillStatsOutput};
use crate::types::CommandResult;

impl SkillCommand {
    pub(super) fn global_stats(
        &self,
        json_mode: bool,
    ) -> Result<CommandResult, Error> {
        match &self.registry {
            Some(registry) => {
                let loaded_skills = registry.list_skills().len();
                let total_token_usage = registry.session_skill_tokens();

                let match_failure_rate = match &self.usage_tracker {
                    Some(tracker) => {
                        let all_stats = tracker.get_all_stats();
                        let total_matches: usize =
                            all_stats.iter().map(|s| s.match_count).sum();
                        let total_activations: usize =
                            all_stats.iter().map(|s| s.activation_count).sum();
                        if total_matches == 0 {
                            0.0
                        } else {
                            1.0 - (total_activations as f64
                                / total_matches as f64)
                        }
                    }
                    None => 0.0,
                };

                let stats = SkillStatsOutput {
                    loaded_skills,
                    total_token_usage,
                    match_failure_rate: (match_failure_rate * 1000.0).round()
                        / 10.0,
                };

                if json_mode {
                    serde_json::to_string_pretty(&stats)
                        .map(CommandResult::new)
                        .map_err(|e| {
                            Error::ToolExecution(format!(
                                "JSON serialization failed: {}",
                                e
                            ))
                        })
                } else {
                    let mut output = String::new();
                    output.push_str("Global Skill Statistics:\n");
                    output.push_str(&format!(
                        "  Loaded skills: {}\n",
                        stats.loaded_skills
                    ));
                    output.push_str(&format!(
                        "  Total token usage: {}\n",
                        stats.total_token_usage
                    ));
                    output.push_str(&format!(
                        "  Match failure rate: {}%\n",
                        stats.match_failure_rate
                    ));
                    Ok(CommandResult::new(output))
                }
            }
            None => {
                let stats = SkillStatsOutput {
                    loaded_skills: 0,
                    total_token_usage: 0,
                    match_failure_rate: 0.0,
                };
                if json_mode {
                    serde_json::to_string_pretty(&stats)
                        .map(CommandResult::new)
                        .map_err(|e| {
                            Error::ToolExecution(format!(
                                "JSON serialization failed: {}",
                                e
                            ))
                        })
                } else {
                    Ok(CommandResult::new(
                        "Global Skill Statistics:\n\
                         (No skills registry loaded)\n\
                           Loaded skills: 0\n\
                           Total token usage: 0\n\
                           Match failure rate: 0.0%",
                    ))
                }
            }
        }
    }
}
