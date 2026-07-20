use serde::{Deserialize, Serialize};

use crate::types::ToolCategory;

/// Parse a `category:` prefix from a pattern string.
///
/// Returns `Some("Shell")` for `"category:Shell"`, `None` for regular
/// patterns. The category name is the substring after the `"category:"`
/// prefix.
pub fn parse_category_pattern(pattern: &str) -> Option<&str> {
    const PREFIX: &str = "category:";
    pattern.strip_prefix(PREFIX)
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "lowercase")]
pub enum PermissionAction {
    #[default]
    Allow,
    Deny,
    Ask,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PermissionRule {
    pub pattern: String,
    pub action: PermissionAction,
    #[serde(default)]
    pub forced: bool,
}

impl PermissionRule {
    /// Check if this rule matches the given tool name and optional
    /// category.
    ///
    /// When the pattern starts with `category:`, it matches against the
    /// tool's category instead of its name. When no category is provided
    /// but the pattern uses `category:` prefix, it doesn't match
    /// (fail-closed).
    pub fn matches(
        &self,
        tool_name: &str,
        tool_category: Option<ToolCategory>,
    ) -> bool {
        if let Some(category_name) = parse_category_pattern(&self.pattern) {
            match tool_category {
                Some(category) => category.as_pattern_name() == category_name,
                None => false,
            }
        } else {
            self.pattern == tool_name
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_permission_rule_serialization() {
        let rule = PermissionRule {
            pattern: "bash:rm*".into(),
            action: PermissionAction::Deny,
            forced: true,
        };
        let yaml = serde_yaml::to_string(&rule).unwrap();
        assert!(yaml.contains("bash:rm*"));
        assert!(yaml.contains("deny"));
        assert!(yaml.contains("forced: true"));
    }

    #[test]
    fn test_permission_action_allow() {
        let action = PermissionAction::Allow;
        let yaml = serde_yaml::to_string(&action).unwrap();
        assert!(yaml.contains("allow"));
    }

    #[test]
    fn test_permission_action_deny() {
        let action = PermissionAction::Deny;
        let yaml = serde_yaml::to_string(&action).unwrap();
        assert!(yaml.contains("deny"));
    }

    #[test]
    fn test_permission_action_ask() {
        let action = PermissionAction::Ask;
        let yaml = serde_yaml::to_string(&action).unwrap();
        assert!(yaml.contains("ask"));
    }

    #[test]
    fn test_parse_category_pattern_with_prefix() {
        assert_eq!(parse_category_pattern("category:Shell"), Some("Shell"));
        assert_eq!(
            parse_category_pattern("category:Filesystem"),
            Some("Filesystem")
        );
    }

    #[test]
    fn test_parse_category_pattern_without_prefix() {
        assert_eq!(parse_category_pattern("bash"), None);
        assert_eq!(parse_category_pattern("bash:rm*"), None);
    }

    #[test]
    fn test_rule_matches_name_pattern() {
        let rule = PermissionRule {
            pattern: "bash".into(),
            action: PermissionAction::Allow,
            forced: false,
        };
        assert!(rule.matches("bash", None));
        assert!(rule.matches("bash", Some(ToolCategory::Shell)));
        assert!(!rule.matches("read_file", None));
    }

    #[test]
    fn test_rule_matches_category_pattern() {
        let rule = PermissionRule {
            pattern: "category:Shell".into(),
            action: PermissionAction::Deny,
            forced: false,
        };
        assert!(rule.matches("any_tool", Some(ToolCategory::Shell)));
        assert!(!rule.matches("any_tool", Some(ToolCategory::Filesystem)));
    }

    #[test]
    fn test_rule_matches_category_pattern_no_category_fail_closed() {
        let rule = PermissionRule {
            pattern: "category:Shell".into(),
            action: PermissionAction::Allow,
            forced: false,
        };
        // Fail-closed: category pattern with no category → no match
        assert!(!rule.matches("bash", None));
    }
}
