//! Agent and skill configuration types

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AgentConfig {
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub max_steps: Option<u32>,
    #[serde(default)]
    pub allowed_tools: Vec<String>,
    #[serde(default)]
    pub denied_tools: Vec<String>,
    #[serde(default)]
    pub hidden: bool,
    #[serde(default)]
    pub color: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SkillConfig {
    pub name: String,
    pub path: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- AgentConfig 7-field struct ---------------------------------

    /// `AgentConfig` MUST default ALL 7 fields when deserialized
    /// from `{}` (every field has a `#[serde(default)]`).
    #[test]
    fn agent_config_all_seven_fields_default_on_empty() {
        let a: AgentConfig = serde_json::from_str("{}").unwrap();
        assert!(a.description.is_none());
        assert!(a.model.is_none());
        assert!(a.max_steps.is_none());
        assert!(a.allowed_tools.is_empty());
        assert!(a.denied_tools.is_empty());
        assert!(!a.hidden);
        assert!(a.color.is_none());
    }

    /// `AgentConfig` MUST support `#[serde(default)]` independently
    /// on each field — overriding one MUST leave the others at
    /// default.
    #[test]
    fn agent_config_individual_field_overrides_work() {
        let json = r#"{"description": "reviewer", "hidden": true}"#;
        let a: AgentConfig = serde_json::from_str(json).unwrap();
        assert_eq!(a.description, Some("reviewer".to_string()));
        assert!(a.hidden);
        // Untouched fields retain defaults.
        assert!(a.model.is_none());
        assert!(a.max_steps.is_none());
        assert!(a.allowed_tools.is_empty());
        assert!(a.denied_tools.is_empty());
        assert!(a.color.is_none());
    }

    /// `AgentConfig::max_steps` MUST accept `u32::MAX` without panic.
    #[test]
    fn agent_config_max_steps_accepts_u32_max() {
        let json = r#"{"max_steps": 4294967295}"#;
        let a: AgentConfig = serde_json::from_str(json).unwrap();
        assert_eq!(a.max_steps, Some(u32::MAX));
    }

    /// `AgentConfig` MUST NOT derive `Default` (the serde
    /// defaults only apply during deserialization, not for
    /// direct construction). Pin the trait surface by
    /// constructing only via direct field access.
    #[test]
    fn agent_config_must_be_constructed_via_fields() {
        // Only direct field construction works (no
        // AgentConfig::default() because Default is not
        // derived).
        let a = AgentConfig {
            description: None,
            model: None,
            max_steps: None,
            allowed_tools: vec![],
            denied_tools: vec![],
            hidden: false,
            color: None,
        };
        assert!(!a.hidden);
    }

    /// `AgentConfig` MUST serialize ALL 7 fields, with
    /// `Option::None` rendered as JSON `null` and `Vec::new`
    /// as `[]` (no `skip_serializing_if`).
    #[test]
    fn agent_config_serializes_all_seven_fields_with_nulls() {
        let a = AgentConfig {
            description: None,
            model: None,
            max_steps: None,
            allowed_tools: vec![],
            denied_tools: vec![],
            hidden: false,
            color: None,
        };
        let json: serde_json::Value = serde_json::to_value(&a).unwrap();
        let obj = json.as_object().expect("must be object");
        assert_eq!(obj.len(), 7);
        assert!(obj.contains_key("description"));
        assert!(obj.contains_key("model"));
        assert!(obj.contains_key("max_steps"));
        assert!(obj.contains_key("allowed_tools"));
        assert!(obj.contains_key("denied_tools"));
        assert!(obj.contains_key("hidden"));
        assert!(obj.contains_key("color"));
        // None values MUST serialize as null (no skip).
        assert_eq!(json["description"], serde_json::Value::Null);
        // Empty Vec MUST serialize as [].
        assert_eq!(json["allowed_tools"], serde_json::json!([]));
    }

    /// `AgentConfig` MUST round-trip all 7 fields through JSON.
    #[test]
    fn agent_config_round_trips_through_json() {
        let a = AgentConfig {
            description: Some("d".to_string()),
            model: Some("gpt-4o".to_string()),
            max_steps: Some(50),
            allowed_tools: vec!["bash".to_string(), "edit".to_string()],
            denied_tools: vec!["web_search".to_string()],
            hidden: true,
            color: Some("#FF0000".to_string()),
        };
        let json = serde_json::to_string(&a).unwrap();
        let parsed: AgentConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.description, a.description);
        assert_eq!(parsed.model, a.model);
        assert_eq!(parsed.max_steps, a.max_steps);
        assert_eq!(parsed.allowed_tools, a.allowed_tools);
        assert_eq!(parsed.denied_tools, a.denied_tools);
        assert_eq!(parsed.hidden, a.hidden);
        assert_eq!(parsed.color, a.color);
    }

    // -- SkillConfig ------------------------------------------------

    /// `SkillConfig` MUST require both `name` and `path` (no
    /// serde defaults — these are the file identity).
    #[test]
    fn skill_config_minimal_serde() {
        let s = SkillConfig {
            name: "linting".to_string(),
            path: "/etc/skills/lint.md".to_string(),
        };
        let json = serde_json::to_string(&s).unwrap();
        let parsed: SkillConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, "linting");
        assert_eq!(parsed.path, "/etc/skills/lint.md");
    }

    /// `SkillConfig` MUST round-trip all 2 fields through JSON.
    #[test]
    fn skill_config_round_trips_through_json() {
        let s = SkillConfig {
            name: "a".to_string(),
            path: "/b".to_string(),
        };
        let json = serde_json::to_string(&s).unwrap();
        let parsed: SkillConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, "a");
        assert_eq!(parsed.path, "/b");
    }

    /// `SkillConfig` MUST support `Debug + Clone`.
    #[test]
    fn skill_config_supports_debug_and_clone() {
        let s = SkillConfig {
            name: "a".to_string(),
            path: "/b".to_string(),
        };
        let _ = format!("{s:?}");
        let cloned = s.clone();
        assert_eq!(cloned.name, "a");
        assert_eq!(cloned.path, "/b");
    }
}
