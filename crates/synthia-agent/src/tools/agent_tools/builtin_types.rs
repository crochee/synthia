//! Built-in subagent type definitions.
//!
//! Provides the canonical `general` and `explore` subagent types that are
//! advertised by the `task` tool, plus helpers for recognising reserved
//! identifiers and resolving their permission/configuration sets.

use synthia_permission::{PermissionAction, PermissionRule};

/// Stable identifiers for built-in subagent types.
pub const BUILTIN_SUBAGENT_TYPES: &[&str] = &["general", "explore"];

/// Returns `true` if `name` matches a reserved built-in subagent type.
pub fn is_builtin_subagent_type(name: &str) -> bool {
    BUILTIN_SUBAGENT_TYPES.contains(&name)
}

/// Configuration for a subagent type.
///
/// Describes the tool surface and recursive permissions that a subagent of
/// this type should receive. `allowed_tools` is the inclusive allow-list;
/// `denied_tools` is an explicit deny-list used to produce permission rules.
pub struct SubagentTypeConfig {
    pub description: &'static str,
    pub allowed_tools: Vec<&'static str>,
    pub denied_tools: Vec<&'static str>,
    pub allow_task: bool,
    pub allow_todowrite: bool,
}

/// Resolve the configuration for a built-in subagent type.
pub fn get_builtin_config(name: &str) -> Option<SubagentTypeConfig> {
    match name {
        "general" => Some(SubagentTypeConfig {
            description: "General-purpose subagent for multi-step tasks",
            allowed_tools: vec![
                "read",
                "write",
                "glob",
                "grep",
                "apply_patch",
                "bash",
                "web_fetch",
            ],
            denied_tools: vec![],
            allow_task: false,
            allow_todowrite: false,
        }),
        "explore" => Some(SubagentTypeConfig {
            description: "Read-only subagent for codebase exploration",
            allowed_tools: vec!["read", "glob", "grep", "web_fetch"],
            denied_tools: vec!["write", "apply_patch", "bash"],
            allow_task: false,
            allow_todowrite: false,
        }),
        _ => None,
    }
}

/// Build a list of explicit [`PermissionRule`] denials for the denied tools
/// declared by a built-in subagent type.
///
/// Returns an empty vector for unknown or custom types.
pub fn builtin_denied_tool_rules(name: &str) -> Vec<PermissionRule> {
    get_builtin_config(name)
        .map(|cfg| {
            cfg.denied_tools
                .into_iter()
                .map(|tool| PermissionRule {
                    pattern: tool.to_string(),
                    action: PermissionAction::Deny,
                    forced: true,
                })
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_types_are_reserved() {
        assert!(is_builtin_subagent_type("general"));
        assert!(is_builtin_subagent_type("explore"));
        assert!(!is_builtin_subagent_type("custom"));
    }

    #[test]
    fn general_config_allows_broad_tools() {
        let cfg = get_builtin_config("general").unwrap();
        assert!(cfg.allowed_tools.contains(&"bash"));
        assert!(cfg.allowed_tools.contains(&"write"));
        assert!(!cfg.allow_task);
        assert!(!cfg.allow_todowrite);
    }

    #[test]
    fn explore_config_is_read_only() {
        let cfg = get_builtin_config("explore").unwrap();
        assert!(cfg.allowed_tools.contains(&"read"));
        assert!(cfg.denied_tools.contains(&"write"));
        assert!(cfg.denied_tools.contains(&"apply_patch"));
        assert!(cfg.denied_tools.contains(&"bash"));
        assert!(!cfg.allow_task);
        assert!(!cfg.allow_todowrite);
    }

    #[test]
    fn denied_tool_rules_match_config() {
        let rules = builtin_denied_tool_rules("explore");
        assert!(rules.iter().any(|r| r.pattern == "write"));
        assert!(rules.iter().any(|r| r.pattern == "apply_patch"));
        assert!(rules.iter().any(|r| r.pattern == "bash"));
    }
}
