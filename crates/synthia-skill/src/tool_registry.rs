use std::sync::Arc;

use synthia_tool::{ToolEntry, ToolRegistry};

use crate::builtin::skill_tool::SkillTool;

pub fn register_skill_tool(registry: &mut ToolRegistry) {
    registry.register(ToolEntry::new(Arc::new(SkillTool::new())));
}
