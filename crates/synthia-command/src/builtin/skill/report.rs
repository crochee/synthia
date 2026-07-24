//! `skill_report` — per-skill usage record.
//!
//! Reads from the usage tracker (NOT the registry). When the
//! tracker is `None` or the named skill has no record, returns
//! a friendly "no data" message rather than an error.
//!
//! JSON mode uses [`SkillReportOutput`].

use synthia_core::Error;

use super::{construct::SkillCommand, types::SkillReportOutput};
use crate::types::CommandResult;

impl SkillCommand {
    pub(super) fn skill_report(
        &self,
        name: &str,
        json_mode: bool,
    ) -> Result<CommandResult, Error> {
        match &self.usage_tracker {
            Some(tracker) => match tracker.get_stats(name) {
                Some(record) => {
                    let report = SkillReportOutput {
                        skill_name: record.skill_name.clone(),
                        match_count: record.match_count,
                        activation_count: record.activation_count,
                        estimated_token_cost: record.estimated_token_cost,
                        last_matched: record
                            .last_matched
                            .map(|dt| dt.to_rfc3339()),
                        last_activated: record
                            .last_activated
                            .map(|dt| dt.to_rfc3339()),
                    };

                    if json_mode {
                        serde_json::to_string_pretty(&report)
                            .map(CommandResult::new)
                            .map_err(|e| {
                                Error::ToolExecution(format!(
                                    "JSON serialization failed: {}",
                                    e
                                ))
                            })
                    } else {
                        let mut output = String::new();
                        output.push_str(&format!(
                            "Usage Report: {}\n",
                            report.skill_name
                        ));
                        output.push_str(&format!(
                            "  Match count: {}\n",
                            report.match_count
                        ));
                        output.push_str(&format!(
                            "  Activation count: {}\n",
                            report.activation_count
                        ));
                        output.push_str(&format!(
                            "  Estimated token cost: {}\n",
                            report.estimated_token_cost
                        ));
                        if let Some(dt) = &report.last_matched {
                            output
                                .push_str(&format!("  Last matched: {}\n", dt));
                        }
                        if let Some(dt) = &report.last_activated {
                            output.push_str(&format!(
                                "  Last activated: {}\n",
                                dt
                            ));
                        }
                        Ok(CommandResult::new(output))
                    }
                }
                None => Ok(CommandResult::new(format!(
                    "No usage data found for skill '{}'.",
                    name
                ))),
            },
            None => Ok(CommandResult::new(format!(
                "No usage tracker available. Cannot report on skill '{}'.",
                name
            ))),
        }
    }
}
