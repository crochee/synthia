//! Skill directory discovery + light metadata loading.
//!
//! Three responsibilities live here, separated from
//! the read-only display functions in [`super::view`]
//! so the discovery logic can be reused (the `stats`
//! and `report` commands both call `load_all_skills`):
//!
//! - [`load_all_skills`]: walks every discovered skill
//!   directory, parses `SKILL.md` frontmatter, and
//!   returns a [`Vec<LoadedSkill>`] suitable for CLI
//!   rendering. Invalid skills are logged via
//!   `tracing::warn!` and skipped — never panics.
//! - [`collect_skill_dirs`]: discovers the three skill
//!   roots (`builtin` / `project` / `user`) and returns
//!   `(path, SkillSource)` pairs.
//! - [`source_to_string`]: trivial `SkillSource` -> `&str`
//!   formatter used by the view layer.

use std::path::{Path, PathBuf};

use anyhow::Result;
use synthia_skill::{
    loader::SkillLoader,
    types::{SkillSource, SkillTokenCount},
};

use super::types::LoadedSkill;

/// Scan skill directories and load metadata for each
/// skill found. Invalid `SKILL.md` files are logged
/// with `tracing::warn!` and skipped — they do NOT
/// cause the whole `list_skills` / `show_skill_info` /
/// `show_skill_stats` / `show_skill_report` command
/// to fail.
pub(super) fn load_all_skills(
    workspace_root: &Path,
) -> Result<Vec<LoadedSkill>> {
    let dirs = collect_skill_dirs(workspace_root);
    let mut skills = Vec::new();

    for (skill_dir, source) in &dirs {
        let skill_md = skill_dir.join("SKILL.md");
        if !skill_md.exists() {
            continue;
        }

        let metadata = match SkillLoader::parse_frontmatter(&skill_md) {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!(path = ?skill_md, error = ?e, "Skipping invalid skill");
                continue;
            }
        };

        let body = SkillLoader::parse_body(&skill_md).unwrap_or_default();

        let level0_text =
            format!("{}: {}", metadata.name, metadata.description);
        let token_count = SkillTokenCount {
            level0: level0_text.len() / 4,
            level1: body.len() / 4,
        };

        skills.push(LoadedSkill {
            metadata,
            source: source_to_string(source),
            token_count,
            body_length: body.len(),
        });
    }

    Ok(skills)
}

/// Collect all skill directories from builtin, project,
/// and user locations. Each location is only included
/// if the corresponding directory exists — this means
/// `load_all_skills` is safe to call on a fresh
/// workspace that has zero of the three roots present.
pub(super) fn collect_skill_dirs(
    workspace_root: &Path,
) -> Vec<(PathBuf, SkillSource)> {
    let mut dirs = Vec::new();

    // Builtin skills: <crate_root>/../../skills/
    let builtin_dir =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../skills");
    if builtin_dir.is_dir() {
        add_dirs_from_root(&builtin_dir, SkillSource::BuiltIn, &mut dirs);
    }

    // Project skills: <workspace>/.synthia/skills/
    let project_dir = workspace_root.join(".synthia/skills");
    if project_dir.is_dir() {
        add_dirs_from_root(&project_dir, SkillSource::Project, &mut dirs);
    }

    // User skills: <workspace>/.agents/skills/
    let user_dir = workspace_root.join(".agents/skills");
    if user_dir.is_dir() {
        add_dirs_from_root(&user_dir, SkillSource::User, &mut dirs);
    }

    dirs
}

/// Add skill subdirectories from a given root. A
/// subdirectory counts as a "skill" iff it both
/// `is_dir()` and contains a `SKILL.md` file. Other
/// non-skill subdirectories (e.g. an `archive/` folder
/// inside `.agents/skills/`) are silently skipped.
fn add_dirs_from_root(
    root: &Path,
    source: SkillSource,
    out: &mut Vec<(PathBuf, SkillSource)>,
) {
    if let Ok(entries) = std::fs::read_dir(root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() && path.join("SKILL.md").exists() {
                out.push((path, source.clone()));
            }
        }
    }
}

/// Map [`SkillSource`] to the short string the CLI
/// displays. Kept here (not in `view.rs`) because
/// `load_all_skills` needs it when building
/// [`LoadedSkill::source`].
pub(super) fn source_to_string(source: &SkillSource) -> String {
    match source {
        SkillSource::BuiltIn => "builtin".to_string(),
        SkillSource::Project => "project".to_string(),
        SkillSource::User => "user".to_string(),
    }
}
