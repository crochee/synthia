//! Skill usage statistics — the `stats` and `report`
//! subcommands.
//!
//! Two public entry points:
//!
//! - [`show_skill_stats`]: workspace-wide aggregates
//!   (total skills, active skills, total token usage,
//!   total matches, total activations, match-to-
//!   activation rate).
//! - [`show_skill_report`]: per-skill breakdown
//!   (description, version, triggers, individual
//!   counts, success rate, last-matched / last-activated
//!   timestamps).
//!
//! Both commands read the
//! `.synthia/.skill-usage.json` file written by the
//! agent runtime via [`synthia_skill::usage::SkillUsageTracker`].
//! A missing file is **not** an error — both commands
//! return an all-zero / "Never" report instead. This
//! is what `tracker.load()`'s `let _ =` swallow in both
//! bodies expresses.

use std::path::Path;

use anyhow::Result;

use super::discover::load_all_skills;

/// Show global skill usage statistics aggregated
/// across every skill discovered in the workspace.
pub fn show_skill_stats(workspace_root: &Path, as_json: bool) -> Result<()> {
    let skills = load_all_skills(workspace_root)?;

    // Collect skill names
    let skill_names: Vec<String> =
        skills.iter().map(|s| s.metadata.name.clone()).collect();

    // Create usage tracker and load stats from storage
    let usage_file = workspace_root.join(".synthia/.skill-usage.json");
    let tracker = synthia_skill::usage::SkillUsageTracker::new()
        .with_storage_path(usage_file.clone());

    // Try to load existing stats
    let _ = tracker.load();

    // Calculate global stats
    let mut total_matches = 0usize;
    let mut total_activations = 0usize;
    let mut total_token_usage = 0usize;

    for name in &skill_names {
        if let Some(record) = tracker.get_stats(name) {
            total_matches += record.match_count;
            total_activations += record.activation_count;
            total_token_usage += record.estimated_token_cost;
        }
    }

    // Count active skills (those that have been activated at least once)
    let mut active_count = 0usize;
    for name in &skill_names {
        if let Some(record) = tracker.get_stats(name)
            && record.activation_count > 0
        {
            active_count += 1;
        }
    }

    let stats = synthia_skill::SkillGlobalStats {
        total_skills: skills.len(),
        active_skills: active_count,
        total_token_usage,
        total_matches,
        total_activations,
    };

    if as_json {
        println!("{}", serde_json::to_string_pretty(&stats)?);
    } else {
        println!("Skill Statistics");
        println!("{}", "=".repeat(40));
        println!("Total registered skills: {}", stats.total_skills);
        println!("Active skills: {}", stats.active_skills);
        println!("Total token usage: {}", stats.total_token_usage);
        println!("Total matches: {}", stats.total_matches);
        println!("Total activations: {}", stats.total_activations);

        if stats.total_matches > 0 {
            let match_rate = (stats.total_activations as f64
                / stats.total_matches as f64)
                * 100.0;
            println!("Match-to-activation rate: {:.1}%", match_rate);
        }
    }

    Ok(())
}

/// Show usage report for a specific skill. Returns
/// `Err` if the skill name is not present in the
/// workspace (so the user can distinguish "skill
/// doesn't exist" from "skill exists but has no usage
/// record yet").
pub fn show_skill_report(
    workspace_root: &Path,
    name: &str,
    as_json: bool,
) -> Result<()> {
    // First check if the skill exists
    let skills = load_all_skills(workspace_root)?;
    let skill = skills
        .iter()
        .find(|s| s.metadata.name == name)
        .ok_or_else(|| anyhow::anyhow!("Skill not found: {}", name))?;

    // Load usage tracker
    let usage_file = workspace_root.join(".synthia/.skill-usage.json");
    let tracker = synthia_skill::usage::SkillUsageTracker::new()
        .with_storage_path(usage_file.clone());

    // Try to load existing stats
    let _ = tracker.load();

    // Get skill-specific stats
    let record = tracker.get_stats(name);

    if as_json {
        let report = serde_json::json!({
            "skill_name": name,
            "description": skill.metadata.description,
            "source": skill.source,
            "version": skill.metadata.version,
            "triggers": skill.metadata.triggers,
            "usage": {
                "match_count": record.as_ref().map(|r| r.match_count).unwrap_or(0),
                "activation_count": record.as_ref().map(|r| r.activation_count).unwrap_or(0),
                "estimated_token_cost": record.as_ref().map(|r| r.estimated_token_cost).unwrap_or(0),
                "last_matched": record.as_ref().and_then(|r| r.last_matched),
                "last_activated": record.as_ref().and_then(|r| r.last_activated),
            },
            "success_rate": if record.as_ref().map(|r| r.match_count).unwrap_or(0) > 0 {
                let matches = record.as_ref().map(|r| r.match_count).unwrap_or(0) as f64;
                let activations = record.as_ref().map(|r| r.activation_count).unwrap_or(0) as f64;
                if matches > 0.0 {
                    Some((activations / matches) * 100.0)
                } else {
                    None
                }
            } else {
                None
            },
            "typical_scenarios": skill.metadata.triggers,
        });
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("Skill Report: {}", name);
        println!("{}", "=".repeat(40));
        println!("Description: {}", skill.metadata.description);
        println!("Source: {}", skill.source);

        if let Some(ref version) = skill.metadata.version {
            println!("Version: {}", version);
        }

        if !skill.metadata.triggers.is_empty() {
            println!("Triggers: {}", skill.metadata.triggers.join(", "));
            println!(
                "Typical scenarios: {}",
                skill.metadata.triggers.join(", ")
            );
        }

        println!("\nUsage Statistics");
        println!("{}", "-".repeat(40));

        let match_count = record.as_ref().map(|r| r.match_count).unwrap_or(0);
        let activation_count =
            record.as_ref().map(|r| r.activation_count).unwrap_or(0);
        let token_cost =
            record.as_ref().map(|r| r.estimated_token_cost).unwrap_or(0);

        println!("Match count: {}", match_count);
        println!("Activation count: {}", activation_count);
        println!("Estimated token cost: {}", token_cost);

        if match_count > 0 {
            let rate = (activation_count as f64 / match_count as f64) * 100.0;
            println!("Success rate: {:.1}%", rate);
        } else {
            println!("Success rate: N/A (no matches)");
        }

        if let Some(ref rec) = record {
            if let Some(last_matched) = rec.last_matched {
                println!(
                    "Last matched: {}",
                    last_matched.format("%Y-%m-%d %H:%M:%S UTC")
                );
            }
            if let Some(last_activated) = rec.last_activated {
                println!(
                    "Last activated: {}",
                    last_activated.format("%Y-%m-%d %H:%M:%S UTC")
                );
            }
        } else {
            println!("Last matched: Never");
            println!("Last activated: Never");
        }
    }

    Ok(())
}
