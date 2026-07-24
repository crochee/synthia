//! Build a [`MergedPolicy`] from an [`AgentDefinition`].
//!
//! This bridges the file-based agent frontmatter fields
//! (`permission_rules`, `permission_default`, `denied_tools`)
//! into the three-layer `MergedPolicy` system.

use synthia_permission::{merged_policy::MergedPolicy, rule::PermissionRule};

use crate::registry::types::AgentDefinition;

/// Build a [`MergedPolicy`] from an agent definition.
///
/// - `permission_rules` from the agent definition become Agent-layer rules
/// - `denied_tools` become forced Deny rules at the Agent layer
pub fn build_merged_policy(def: &AgentDefinition) -> MergedPolicy {
    let agent_rules: Vec<PermissionRule> = def.permission_rules.clone();

    let mut policy = MergedPolicy::new(&[], &agent_rules, &[]);

    // Add forced Deny for each denied tool
    if let Some(denied) = &def.denied_tools {
        for tool in denied {
            policy.add_forced_deny_rule(tool);
        }
    }

    policy
}

/// Build an allowed-tools list from an agent definition.
///
/// When `tools` is `Some`, only those tool names are allowed.
/// When `tools` is `None`, all tools in the registry are allowed
/// (subject to `denied_tools` filtering).
pub fn build_allowed_tools(def: &AgentDefinition) -> Option<Vec<String>> {
    def.tools.clone()
}

/// Check whether a tool name is allowed by the agent's `tools` allowlist.
///
/// Returns `true` when:
/// - `allowed_tools` is `None` (no allowlist -> all allowed), or
/// - `tool_name` appears in the allowlist.
pub fn is_tool_allowed(
    allowed_tools: Option<&[String]>,
    tool_name: &str,
) -> bool {
    match allowed_tools {
        None => true,
        Some(list) => list.iter().any(|t| t == tool_name),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use chrono::Utc;
    use synthia_permission::rule::{PermissionAction, PermissionRule};

    use super::*;

    fn make_def(
        permission_rules: Vec<PermissionRule>,
        denied_tools: Option<Vec<String>>,
        tools: Option<Vec<String>>,
    ) -> AgentDefinition {
        AgentDefinition {
            id: "test-agent".to_string(),
            name: "Test Agent".to_string(),
            description: String::new(),
            capabilities: vec![],
            when_to_use: vec![],
            constraints: vec![],
            system_prompt: String::new(),
            source_path: PathBuf::from("/tmp/test.md"),
            file_hash: "abc".to_string(),
            loaded_at: Utc::now(),
            enabled: true,
            permission_rules,
            permission_default: None,
            tools,
            denied_tools,
            extends: None,
            mode: None,
        }
    }

    #[test]
    fn build_policy_empty_returns_empty_policy() {
        let def = make_def(vec![], None, None);
        let policy = build_merged_policy(&def);
        assert!(policy.is_empty());
        // Fail-closed: unknown patterns default to Ask.
        assert_eq!(policy.evaluate("bash"), PermissionAction::Ask);
    }

    #[test]
    fn build_policy_includes_permission_rules_as_agent_layer() {
        let def = make_def(
            vec![PermissionRule {
                pattern: "bash".to_string(),
                action: PermissionAction::Deny,
                forced: false,
            }],
            None,
            None,
        );
        let policy = build_merged_policy(&def);
        assert_eq!(policy.evaluate("bash"), PermissionAction::Deny);
    }

    #[test]
    fn build_policy_includes_denied_tools_as_forced_deny() {
        let def = make_def(
            vec![],
            Some(vec!["rm".to_string(), "sudo".to_string()]),
            None,
        );
        let policy = build_merged_policy(&def);
        assert_eq!(policy.evaluate("rm"), PermissionAction::Deny);
        assert_eq!(policy.evaluate("sudo"), PermissionAction::Deny);
        // Fail-closed: tools not in the denied list still require confirmation.
        assert_eq!(policy.evaluate("ls"), PermissionAction::Ask);
    }

    #[test]
    fn build_policy_forced_deny_overrides_allow() {
        let def = make_def(
            vec![PermissionRule {
                pattern: "bash".to_string(),
                action: PermissionAction::Allow,
                forced: false,
            }],
            Some(vec!["bash".to_string()]),
            None,
        );
        let policy = build_merged_policy(&def);
        assert_eq!(policy.evaluate("bash"), PermissionAction::Deny);
    }

    #[test]
    fn build_allowed_tools_returns_none_when_no_allowlist() {
        let def = make_def(vec![], None, None);
        assert!(build_allowed_tools(&def).is_none());
    }

    #[test]
    fn build_allowed_tools_returns_some_when_allowlist_set() {
        let def = make_def(
            vec![],
            None,
            Some(vec!["read_file".to_string(), "grep".to_string()]),
        );
        let allowed = build_allowed_tools(&def).unwrap();
        assert_eq!(allowed, vec!["read_file", "grep"]);
    }

    #[test]
    fn is_tool_allowed_none_allows_everything() {
        assert!(is_tool_allowed(None, "anything"));
    }

    #[test]
    fn is_tool_allowed_some_checks_list() {
        let list = vec!["read_file".to_string(), "grep".to_string()];
        assert!(is_tool_allowed(Some(&list), "read_file"));
        assert!(!is_tool_allowed(Some(&list), "bash"));
    }
}
