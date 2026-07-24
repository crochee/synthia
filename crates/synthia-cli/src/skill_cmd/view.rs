//! Read-only skill display commands.
//!
//! Three public entry points live here:
//!
//! - [`list_skills`]: tabular or JSON list of every
//!   skill discovered in the workspace (combines
//!   builtin / project / user).
//! - [`show_skill_info`]: detailed single-skill view
//!   (frontmatter fields + L0/L1 token counts).
//! - [`list_installed_skills`]: tabular or JSON list of
//!   the **installed** user skills (the subset that
//!   lives in `.agents/skills/` and was put there by
//!   `install_skill`, not the wider union of
//!   builtin+project+user that [`list_skills`] shows).
//!
//! The split is intentional: [`list_skills`] is
//! workspace-wide (cheap — no registry/installer
//! build), while [`list_installed_skills`] constructs
//! a real [`SkillRegistry`] and [`SkillInstaller`] to
//! read install records (heavier, but reflects the
//! `install` / `uninstall` history).

use std::{path::Path, sync::Arc};

use anyhow::Result;
use synthia_skill::{
    installer::SkillInstaller,
    registry::SkillRegistry,
    types::SkillPaths,
};

use super::discover::load_all_skills;

/// List all loaded skills from the workspace. Renders
/// as a fixed-width table by default, or as a JSON
/// array when `as_json = true`. An empty workspace
/// prints `"No skills loaded."` and returns `Ok(())`.
pub fn list_skills(workspace_root: &Path, as_json: bool) -> Result<()> {
    let skills = load_all_skills(workspace_root)?;

    if as_json {
        let items: Vec<serde_json::Value> = skills
            .iter()
            .map(|s| {
                serde_json::json!({
                    "name": s.metadata.name,
                    "source": s.source,
                    "state": "loaded",
                    "token_count": {
                        "level0": s.token_count.level0,
                        "level1": s.token_count.level1,
                    }
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&items)?);
    } else {
        if skills.is_empty() {
            println!("No skills loaded.");
            return Ok(());
        }

        println!(
            "{:<20} {:<10} {:<12} {:>6} {:>6}",
            "Name", "Source", "State", "L0", "L1"
        );
        println!("{}", "-".repeat(58));

        for s in &skills {
            println!(
                "{:<20} {:<10} {:<12} {:>6} {:>6}",
                s.metadata.name,
                s.source,
                "loaded",
                s.token_count.level0,
                s.token_count.level1
            );
        }
    }

    Ok(())
}

/// Show detailed metadata for a single skill. Returns
/// `Err` if the skill is not present in the workspace.
pub fn show_skill_info(
    workspace_root: &Path,
    name: &str,
    as_json: bool,
) -> Result<()> {
    let skills = load_all_skills(workspace_root)?;
    let skill = skills
        .into_iter()
        .find(|s| s.metadata.name == name)
        .ok_or_else(|| anyhow::anyhow!("Skill not found: {}", name))?;

    if as_json {
        let info = serde_json::json!({
            "name": skill.metadata.name,
            "description": skill.metadata.description,
            "source": skill.source,
            "state": "loaded",
            "version": skill.metadata.version,
            "license": skill.metadata.license,
            "triggers": skill.metadata.triggers,
            "tags": skill.metadata.tags,
            "priority": skill.metadata.priority,
            "allowed_tools": skill.metadata.allowed_tools,
            "token_count": {
                "level0": skill.token_count.level0,
                "level1": skill.token_count.level1,
            },
            "body_length": skill.body_length,
        });
        println!("{}", serde_json::to_string_pretty(&info)?);
    } else {
        println!("Skill: {}", skill.metadata.name);
        println!("Description: {}", skill.metadata.description);
        println!("Source: {}", skill.source);
        println!("State: loaded");

        if let Some(ref version) = skill.metadata.version {
            println!("Version: {}", version);
        }
        if let Some(ref license) = skill.metadata.license {
            println!("License: {}", license);
        }

        println!("Priority: {}", skill.metadata.priority);

        if !skill.metadata.triggers.is_empty() {
            println!("Triggers: {}", skill.metadata.triggers.join(", "));
        }
        if !skill.metadata.tags.is_empty() {
            println!("Tags: {}", skill.metadata.tags.join(", "));
        }
        if !skill.metadata.allowed_tools.is_empty() {
            println!(
                "Allowed tools: {}",
                skill.metadata.allowed_tools.join(", ")
            );
        }

        println!(
            "Token count (L0/L1): {} / {}",
            skill.token_count.level0, skill.token_count.level1
        );
        println!("Body length: {} chars", skill.body_length);
    }

    Ok(())
}

/// List installed user skills (the records produced by
/// the `install` command and stored in
/// `.agents/skills/`). Unlike [`list_skills`], this
/// command does NOT walk the union of
/// builtin / project / user — it only reflects what
/// `install_skill` put there.
pub fn list_installed_skills(
    workspace_root: &Path,
    as_json: bool,
) -> Result<()> {
    let skills_dir = workspace_root.join(".agents/skills");

    let project_dir = workspace_root.join(".synthia/skills");
    let builtin_dir =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../skills");

    let paths = SkillPaths {
        user_dir: skills_dir.clone(),
        project_dir,
        builtin_dir,
    };

    let registry = Arc::new(SkillRegistry::new(paths));
    let installer = SkillInstaller::new(skills_dir, Arc::clone(&registry));

    let installed = installer.list_installed()?;

    if as_json {
        let items: Vec<serde_json::Value> = installed
            .iter()
            .map(|s| {
                serde_json::json!({
                    "name": s.name,
                    "version": s.version,
                    "description": s.description,
                    "path": s.path.to_string_lossy(),
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&items)?);
    } else {
        if installed.is_empty() {
            println!("No skills installed.");
            return Ok(());
        }

        println!("{:<20} {:<10} Description", "Name", "Version");
        println!("{}", "-".repeat(58));

        for s in &installed {
            let version = s.version.as_deref().unwrap_or("-");
            println!("{:<20} {:<10} {}", s.name, version, s.description);
        }
    }

    Ok(())
}
