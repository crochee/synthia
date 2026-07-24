//! Agent file merge logic.

use crate::agent_file::frontmatter::{FileAgentFrontmatter, PermissionRule};

/// Merge parent and child permission rules with child priority.
///
/// For each child rule, if a rule with the same `pattern` already exists in
/// the parent list it is replaced; otherwise the child rule is appended.
/// Rules in the parent list that are not matched by any child rule are
/// preserved in their original position.
pub fn merge_permission_rules(
    parent: &[PermissionRule],
    child: &[PermissionRule],
) -> Vec<PermissionRule> {
    let mut result: Vec<PermissionRule> = parent.to_vec();
    for child_rule in child {
        if let Some(pos) =
            result.iter().position(|r| r.pattern == child_rule.pattern)
        {
            result[pos] = child_rule.clone();
        } else {
            result.push(child_rule.clone());
        }
    }
    result
}

/// Merge two `FileAgentFrontmatter` values with child priority.
///
/// Scalar/optional fields fall back to the parent when the child leaves them
/// unset. `permission_rules` is merged via [`merge_permission_rules`].
pub fn merge_frontmatter(
    parent: &FileAgentFrontmatter,
    child: &FileAgentFrontmatter,
) -> FileAgentFrontmatter {
    FileAgentFrontmatter {
        model: child.model.clone().or_else(|| parent.model.clone()),
        permission_rules: merge_permission_rules(
            &parent.permission_rules,
            &child.permission_rules,
        ),
        permission_default: child
            .permission_default
            .clone()
            .or_else(|| parent.permission_default.clone()),
        tools: child.tools.clone().or_else(|| parent.tools.clone()),
        denied_tools: child
            .denied_tools
            .clone()
            .or_else(|| parent.denied_tools.clone()),
        extends: child.extends.clone().or_else(|| parent.extends.clone()),
        mode: child.mode.clone().or_else(|| parent.mode.clone()),
        hidden: child.hidden.or(parent.hidden),
        color: child.color.clone().or_else(|| parent.color.clone()),
        steps: child.steps.or(parent.steps),
        options: child.options.clone().or_else(|| parent.options.clone()),
    }
}

#[cfg(test)]
mod tests {
    use synthia_permission::Permission;

    use super::{merge_frontmatter, merge_permission_rules};
    use crate::agent_file::frontmatter::{
        FileAgentFrontmatter,
        PermissionRule,
    };

    fn rule(pattern: &str, action: Permission, forced: bool) -> PermissionRule {
        PermissionRule {
            pattern: pattern.to_string(),
            action,
            forced,
        }
    }

    #[test]
    fn merge_rules_empty_inputs_returns_empty() {
        let merged = merge_permission_rules(&[], &[]);
        assert!(merged.is_empty());
    }

    #[test]
    fn merge_rules_empty_child_preserves_parent() {
        let parent = vec![
            rule("read_*", Permission::AutoApprove, false),
            rule("bash", Permission::Block, true),
        ];
        let merged = merge_permission_rules(&parent, &[]);
        assert_eq!(merged, parent);
    }

    #[test]
    fn merge_rules_empty_parent_returns_child_clone() {
        let child = vec![rule("write_*", Permission::AutoApprove, true)];
        let merged = merge_permission_rules(&[], &child);
        assert_eq!(merged, child);
    }

    #[test]
    fn merge_rules_child_pattern_replaces_parent_entry_in_place() {
        let parent = vec![
            rule("read_*", Permission::AutoApprove, false),
            rule("bash", Permission::Block, false),
        ];
        let child = vec![rule("bash", Permission::RequireConfirm, true)];
        let merged = merge_permission_rules(&parent, &child);
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].pattern, "read_*");
        assert_eq!(merged[0].action, Permission::AutoApprove);
        assert_eq!(merged[1].pattern, "bash");
        assert_eq!(merged[1].action, Permission::RequireConfirm);
        assert!(merged[1].forced);
    }

    #[test]
    fn merge_rules_new_child_pattern_is_appended() {
        let parent = vec![rule("read_*", Permission::AutoApprove, false)];
        let child = vec![rule("bash", Permission::Block, false)];
        let merged = merge_permission_rules(&parent, &child);
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].pattern, "read_*");
        assert_eq!(merged[1].pattern, "bash");
    }

    #[test]
    fn merge_rules_multiple_child_changes_applied() {
        let parent = vec![
            rule("read_*", Permission::AutoApprove, false),
            rule("write_*", Permission::RequireConfirm, false),
            rule("bash", Permission::Block, true),
        ];
        let child = vec![
            rule("write_*", Permission::AutoApprove, true),
            rule(
                "net",
                Permission::Deny {
                    reason: "no network".to_string(),
                },
                false,
            ),
        ];
        let merged = merge_permission_rules(&parent, &child);
        assert_eq!(merged.len(), 4);
        assert_eq!(merged[0].pattern, "read_*");
        assert_eq!(merged[0].action, Permission::AutoApprove);
        assert_eq!(merged[1].pattern, "write_*");
        assert_eq!(merged[1].action, Permission::AutoApprove);
        assert!(merged[1].forced);
        assert_eq!(merged[2].pattern, "bash");
        assert_eq!(merged[2].action, Permission::Block);
        assert!(merged[2].forced);
        assert_eq!(merged[3].pattern, "net");
        assert!(matches!(merged[3].action, Permission::Deny { .. }));
    }

    #[test]
    fn merge_rules_child_does_not_mutate_parent() {
        let parent = vec![rule("bash", Permission::Block, false)];
        let parent_clone = parent.clone();
        let child = vec![rule("bash", Permission::AutoApprove, true)];
        let _ = merge_permission_rules(&parent, &child);
        assert_eq!(parent, parent_clone);
    }

    #[test]
    fn merge_frontmatter_child_scalar_overrides_parent() {
        let parent = FileAgentFrontmatter {
            model: Some("parent-model".to_string()),
            mode: Some("architect".to_string()),
            hidden: Some(true),
            ..Default::default()
        };
        let child = FileAgentFrontmatter {
            model: Some("child-model".to_string()),
            ..Default::default()
        };
        let merged = merge_frontmatter(&parent, &child);
        assert_eq!(merged.model.as_deref(), Some("child-model"));
        assert_eq!(merged.mode.as_deref(), Some("architect"));
        assert_eq!(merged.hidden, Some(true));
    }

    #[test]
    fn merge_frontmatter_parent_scalar_used_when_child_none() {
        let parent = FileAgentFrontmatter {
            model: Some("parent-model".to_string()),
            mode: Some("architect".to_string()),
            steps: Some(7),
            color: Some("#abcdef".to_string()),
            ..Default::default()
        };
        let child = FileAgentFrontmatter::default();
        let merged = merge_frontmatter(&parent, &child);
        assert_eq!(merged.model.as_deref(), Some("parent-model"));
        assert_eq!(merged.mode.as_deref(), Some("architect"));
        assert_eq!(merged.steps, Some(7));
        assert_eq!(merged.color.as_deref(), Some("#abcdef"));
    }

    #[test]
    fn merge_frontmatter_options_falls_back_to_parent() {
        let parent_opts: serde_yaml::Value =
            serde_yaml::from_str("temperature: 0.2").expect("yaml");
        let parent = FileAgentFrontmatter {
            options: Some(parent_opts.clone()),
            ..Default::default()
        };
        let child = FileAgentFrontmatter::default();
        let merged = merge_frontmatter(&parent, &child);
        assert_eq!(merged.options, Some(parent_opts));
    }

    #[test]
    fn merge_frontmatter_combines_permission_rules() {
        let parent = FileAgentFrontmatter {
            permission_rules: vec![rule(
                "read_*",
                Permission::AutoApprove,
                false,
            )],
            ..Default::default()
        };
        let child = FileAgentFrontmatter {
            permission_rules: vec![rule("read_*", Permission::Block, true)],
            ..Default::default()
        };
        let merged = merge_frontmatter(&parent, &child);
        assert_eq!(merged.permission_rules.len(), 1);
        assert_eq!(merged.permission_rules[0].pattern, "read_*");
        assert_eq!(merged.permission_rules[0].action, Permission::Block);
        assert!(merged.permission_rules[0].forced);
    }

    #[test]
    fn merge_frontmatter_preserves_extends_from_parent() {
        let parent = FileAgentFrontmatter {
            extends: Some("base-agent".to_string()),
            ..Default::default()
        };
        let child = FileAgentFrontmatter::default();
        let merged = merge_frontmatter(&parent, &child);
        assert_eq!(merged.extends.as_deref(), Some("base-agent"));
    }
}
