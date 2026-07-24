//! Frontmatter parsing for agent files.

use regex::Regex;
use serde::{Deserialize, Serialize};
use synthia_permission::Permission;

/// YAML frontmatter for a file-based Agent definition.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct FileAgentFrontmatter {
    pub model: Option<String>,
    #[serde(default)]
    pub permission_rules: Vec<PermissionRule>,
    #[serde(default)]
    pub permission_default: Option<Permission>,
    #[serde(default)]
    pub tools: Option<Vec<String>>,
    #[serde(default)]
    pub denied_tools: Option<Vec<String>>,
    #[serde(default)]
    pub extends: Option<String>,
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub hidden: Option<bool>,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub steps: Option<u32>,
    #[serde(default)]
    pub options: Option<serde_yaml::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PermissionRule {
    pub pattern: String,
    pub action: Permission,
    #[serde(default)]
    pub forced: bool,
}

/// Valid Agent ID pattern: `[a-z0-9][a-z0-9_-]{0,63}`.
pub const ID_PATTERN: &str = r"^[a-z0-9][a-z0-9_-]{0,63}$";

/// Validate that `id` conforms to [`ID_PATTERN`].
///
/// Returns `Ok(())` on success, or a human-readable error describing why the
/// value is not a valid Agent ID.
pub fn validate_id(id: &str) -> Result<(), String> {
    let re = Regex::new(ID_PATTERN).map_err(|e| e.to_string())?;
    if re.is_match(id) {
        Ok(())
    } else {
        Err(format!(
            "Invalid Agent ID '{}': must match {}",
            id, ID_PATTERN
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::{ID_PATTERN, validate_id};

    #[test]
    fn id_pattern_is_anchored() {
        assert!(ID_PATTERN.starts_with('^'));
        assert!(ID_PATTERN.ends_with('$'));
    }

    #[test]
    fn validate_id_accepts_valid_ids() {
        let max_len: String = "a".repeat(64);
        for id in [
            "a",
            "0",
            "abc",
            "a_b",
            "a-b",
            "a1b2",
            "agent_1",
            "agent-1",
            "a_b-c_d",
            max_len.as_str(),
        ] {
            assert!(validate_id(id).is_ok(), "expected {id:?} to be valid");
        }
    }

    #[test]
    fn validate_id_rejects_invalid_ids() {
        let too_long: String = "a".repeat(65);
        for id in [
            "",
            "A",
            "_a",
            "-a",
            ".a",
            "a b",
            "a/b",
            "a:b",
            "a*b",
            "agent.1",
            "agent/1",
            "!agent",
            "agent!",
            too_long.as_str(),
        ] {
            assert!(validate_id(id).is_err(), "expected {id:?} to be rejected");
        }
    }

    #[test]
    fn validate_id_error_message_includes_value_and_pattern() {
        let err = validate_id("Bad-ID").unwrap_err();
        assert!(err.contains("Bad-ID"));
        assert!(err.contains(ID_PATTERN));
    }
}

#[cfg(test)]
mod yaml_parse_tests {
    use super::{FileAgentFrontmatter, Permission};

    #[test]
    fn parses_minimal_frontmatter() {
        let yaml = "model: claude-opus-4-7\n";
        let fm: FileAgentFrontmatter = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(fm.model.as_deref(), Some("claude-opus-4-7"));
        assert!(fm.permission_rules.is_empty());
        assert!(fm.permission_default.is_none());
        assert!(fm.tools.is_none());
        assert!(fm.extends.is_none());
        assert!(fm.hidden.is_none());
        assert!(fm.steps.is_none());
        assert!(fm.options.is_none());
    }

    #[test]
    fn parses_full_frontmatter() {
        let yaml = r##"
model: claude-opus-4-7
extends: base-agent
mode: architect
hidden: true
color: "#ff00ff"
steps: 5
tools: [read_file, grep]
denied_tools: [bash]
permission_default: require_confirm
permission_rules:
  - pattern: "write_file:*"
    action: auto_approve
    forced: true
  - pattern: "bash"
    action: block
options:
  temperature: 0.2
  max_tokens: 4096
"##;
        let fm: FileAgentFrontmatter = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(fm.model.as_deref(), Some("claude-opus-4-7"));
        assert_eq!(fm.extends.as_deref(), Some("base-agent"));
        assert_eq!(fm.mode.as_deref(), Some("architect"));
        assert_eq!(fm.hidden, Some(true));
        assert_eq!(fm.color.as_deref(), Some("#ff00ff"));
        assert_eq!(fm.steps, Some(5));
        assert_eq!(
            fm.tools.as_deref(),
            Some(&["read_file".to_string(), "grep".to_string()][..])
        );
        assert_eq!(fm.denied_tools.as_deref(), Some(&["bash".to_string()][..]));
        assert_eq!(fm.permission_default, Some(Permission::RequireConfirm));
        assert_eq!(fm.permission_rules.len(), 2);

        let first = &fm.permission_rules[0];
        assert_eq!(first.pattern, "write_file:*");
        assert_eq!(first.action, Permission::AutoApprove);
        assert!(first.forced);

        let second = &fm.permission_rules[1];
        assert_eq!(second.pattern, "bash");
        assert_eq!(second.action, Permission::Block);
        assert!(!second.forced);

        let opts = fm.options.expect("options should be present");
        let opts_map = opts.as_mapping().expect("options should be a mapping");
        assert!(opts_map.contains_key("temperature"));
        assert!(opts_map.contains_key("max_tokens"));
    }
}
