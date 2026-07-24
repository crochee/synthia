//! Subagent permission inheritance.
//!
//! Child agents inherit only the parent's `Deny` rules, and always
//! default-deny the recursive `task` and `todowrite` tools unless the
//! subagent type explicitly opts out.

use synthia_permission::{PermissionAction, PermissionRule};

/// Derive a child-agent permission set from the parent's rules.
///
/// - Only `Deny` rules are inherited; `Allow`/`Ask` rules are dropped so
///   the child earns capabilities through its own type configuration.
/// - `task` and `todowrite` are added as forced `Deny` rules unless the
///   corresponding `subagent_allows_*` flag is `true`.
pub fn derive_subagent_permission(
    parent_permission: &[PermissionRule],
    subagent_allows_task: bool,
    subagent_allows_todowrite: bool,
) -> Vec<PermissionRule> {
    let mut rules: Vec<PermissionRule> = parent_permission
        .iter()
        .filter(|r| r.action == PermissionAction::Deny)
        .cloned()
        .collect();

    if !subagent_allows_task {
        rules.push(PermissionRule {
            pattern: "task".to_string(),
            action: PermissionAction::Deny,
            forced: true,
        });
    }

    if !subagent_allows_todowrite {
        rules.push(PermissionRule {
            pattern: "todowrite".to_string(),
            action: PermissionAction::Deny,
            forced: true,
        });
    }

    rules
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inherits_deny_rules_only() {
        let parent = vec![
            PermissionRule {
                pattern: "*.env".to_string(),
                action: PermissionAction::Deny,
                forced: false,
            },
            PermissionRule {
                pattern: "bash".to_string(),
                action: PermissionAction::Allow,
                forced: false,
            },
        ];
        let derived = derive_subagent_permission(&parent, false, false);
        assert!(derived.iter().any(|r| r.pattern == "*.env"));
        assert!(!derived.iter().any(|r| {
            r.pattern == "bash" && r.action == PermissionAction::Allow
        }));
    }

    #[test]
    fn defaults_deny_task_and_todowrite() {
        let derived = derive_subagent_permission(&[], false, false);
        assert!(derived.iter().any(|r| {
            r.pattern == "task" && r.action == PermissionAction::Deny
        }));
        assert!(derived.iter().any(|r| {
            r.pattern == "todowrite" && r.action == PermissionAction::Deny
        }));
    }

    #[test]
    fn can_opt_out_of_default_denies() {
        let derived = derive_subagent_permission(&[], true, true);
        assert!(!derived.iter().any(|r| r.pattern == "task"));
        assert!(!derived.iter().any(|r| r.pattern == "todowrite"));
    }
}
