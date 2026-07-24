//! Skill install / uninstall — the only **write**
//! operations in the `skill_cmd` family.
//!
//! Two public entry points:
//!
//! - [`install_skill`]: extract a `.skill` ZIP archive
//!   into the user skills directory (`.agents/skills/`).
//!   Bails early with `"Archive not found: ..."` if
//!   the supplied `archive_path` does not exist.
//! - [`uninstall_skill`]: remove an installed user skill
//!   by name. The `SkillRegistry` is built but only the
//!   [`SkillInstaller::uninstall`] path is exercised —
//!   the installer's uninstall is responsible for
//!   also cleaning the registry's view of the skill.
//!
//! Both commands build a fresh [`SkillRegistry`] +
//! [`SkillInstaller`] pair on every call. This is
//! intentional: the CLI is a one-shot tool, not a
//! long-running daemon, and paying the registry
//! construction cost on each invocation keeps the
//! state model trivial.

use std::{path::Path, sync::Arc};

use anyhow::{Result, bail};
use synthia_skill::{
    installer::SkillInstaller,
    registry::SkillRegistry,
    types::SkillPaths,
};

/// Install a skill from a `.skill` ZIP archive into
/// the user skills directory.
pub fn install_skill(
    workspace_root: &Path,
    archive_path: &Path,
    hash: Option<&str>,
) -> Result<()> {
    if !archive_path.exists() {
        bail!("Archive not found: {}", archive_path.display());
    }

    let skills_dir = workspace_root.join(".agents/skills");
    std::fs::create_dir_all(&skills_dir)?;

    // Ensure project/builtin dirs exist for SkillPaths
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

    let skill_name = installer.install(archive_path, hash)?;
    println!("Skill '{}' installed successfully.", skill_name);

    Ok(())
}

/// Uninstall a skill by name from the user skills
/// directory.
pub fn uninstall_skill(workspace_root: &Path, name: &str) -> Result<()> {
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

    installer.uninstall(name)?;
    println!("Skill '{}' uninstalled successfully.", name);

    Ok(())
}
