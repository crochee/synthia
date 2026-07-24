use serde::{Deserialize, Serialize};

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
}
