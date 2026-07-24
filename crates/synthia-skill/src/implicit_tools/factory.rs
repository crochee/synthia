use std::sync::Arc;

use synthia_core::Error;

use super::{load::LoadSkillTool, unload::UnloadSkillTool};
use crate::registry::SkillRegistry;

type Result<T> = std::result::Result<T, Error>;

// ── Helper: create both tools ready for registration ──────────────────────

/// Create both implicit skill tools, ready to be registered as hidden tools.
pub fn create_implicit_tools(
    registry: Arc<SkillRegistry>,
) -> (LoadSkillTool, UnloadSkillTool) {
    (
        LoadSkillTool::new(Arc::clone(&registry)),
        UnloadSkillTool::new(registry),
    )
}

// ── Re-export legacy execution functions for backward compatibility ───────

pub async fn execute_load_skill(
    registry: &SkillRegistry,
    name: &str,
) -> Result<String> {
    let skill = registry.activate_skill(name)?;
    Ok(format!(
        "Loaded skill: {} ({} tokens added to context)",
        name, skill.token_count.level1
    ))
}

pub async fn execute_unload_skill(
    registry: &SkillRegistry,
    name: &str,
) -> Result<String> {
    registry.deactivate_skill(name)?;
    Ok(format!("Unloaded skill: {}", name))
}
