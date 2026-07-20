use serde::Serialize;

use crate::level::Permission;

/// Tool category for routing and permission decisions.
///
/// Mirrors `synthia_core::tool::descriptor::ToolCategory` so that
/// the permission crate can reference a category without pulling in the
/// full unified tool infrastructure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolCategory {
    Filesystem,
    Search,
    Shell,
    Edit,
    Memory,
    Agent,
    Skill,
    Network,
    Utility,
    Custom,
}

impl ToolCategory {
    /// Returns the category name used in `category:X` patterns.
    ///
    /// Pattern syntax uses PascalCase (e.g. `category:Shell`), matching
    /// the Rust enum variant names. This is distinct from the
    /// `snake_case` serde serialization.
    pub fn as_pattern_name(&self) -> &'static str {
        match self {
            ToolCategory::Filesystem => "Filesystem",
            ToolCategory::Search => "Search",
            ToolCategory::Shell => "Shell",
            ToolCategory::Edit => "Edit",
            ToolCategory::Memory => "Memory",
            ToolCategory::Agent => "Agent",
            ToolCategory::Skill => "Skill",
            ToolCategory::Network => "Network",
            ToolCategory::Utility => "Utility",
            ToolCategory::Custom => "Custom",
        }
    }
}

#[derive(Debug, Clone)]
pub struct PermissionRequest {
    pub tool_name: String,
    pub input: serde_json::Value,
    pub requires_permission: bool,
    pub tool_category: Option<ToolCategory>,
}

impl PermissionRequest {
    pub fn new(
        tool_name: String,
        input: serde_json::Value,
        requires_permission: bool,
    ) -> Self {
        Self {
            tool_name,
            input,
            requires_permission,
            tool_category: None,
        }
    }

    pub fn with_category(mut self, category: ToolCategory) -> Self {
        self.tool_category = Some(category);
        self
    }

    pub fn outcome(&self, outcome: Permission) -> PermissionOutcome {
        PermissionOutcome {
            tool_name: self.tool_name.clone(),
            outcome,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct PermissionOutcome {
    pub tool_name: String,
    pub outcome: Permission,
}
