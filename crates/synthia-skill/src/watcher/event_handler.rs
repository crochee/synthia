use std::{path::Path, sync::Arc};

use notify::{Event, EventKind};
use parking_lot::RwLock;
use synthia_core::Error;

use crate::{loader::SkillLoader, registry::SkillRegistry};

pub(super) fn handle_event(
    event: &Event,
    registry: &Arc<RwLock<SkillRegistry>>,
    _loader: &Arc<SkillLoader>,
) {
    let skill_md_filename = "SKILL.md";

    for path in &event.paths {
        if path.file_name().and_then(|n| n.to_str()) != Some(skill_md_filename)
        {
            continue;
        }

        let skill_dir = match path.parent() {
            Some(dir) => dir,
            None => continue,
        };

        match event.kind {
            EventKind::Create(_) | EventKind::Modify(_) if path.exists() => {
                if let Err(e) = reload_skill(skill_dir, registry) {
                    tracing::warn!(path = ?path, error = ?e, "Failed to reload skill");
                }
            }
            EventKind::Remove(_) => {
                if let Some(skill_name) = extract_skill_name_from_path(path) {
                    let removed = registry.read().unregister(&skill_name);
                    if removed {
                        tracing::info!(skill = %skill_name, "Skill removed from registry");
                    }
                }
            }
            _ => {}
        }
    }
}

fn reload_skill(
    skill_dir: &Path,
    registry: &Arc<RwLock<SkillRegistry>>,
) -> Result<(), Error> {
    let skill_md = skill_dir.join("SKILL.md");
    let metadata = SkillLoader::parse_frontmatter(&skill_md)?;
    // parse_body is called to validate the file format; the body is reconstructed by registry
    let _body = SkillLoader::parse_body(&skill_md)?;

    let skill_name = metadata.name.clone();
    let old_entry_exists = registry.read().unregister(&skill_name);

    let warnings = SkillLoader::validate_optional_fields(&metadata);
    for w in &warnings {
        tracing::warn!(skill = %metadata.name, "{}", w);
    }

    registry.read().register_from_path(skill_dir)?;

    if old_entry_exists {
        tracing::info!(skill = %skill_name, path = ?skill_md, "Skill hot-reloaded");
    } else {
        tracing::info!(skill = %skill_name, path = ?skill_md, "Skill loaded");
    }

    Ok(())
}

fn extract_skill_name_from_path(skill_md_path: &Path) -> Option<String> {
    skill_md_path
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .map(String::from)
}
