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
    /// R16: optional LLM-backed compaction policy for this
    /// agent. Deserialised from `compaction = { … }` on the
    /// agent's TOML/YAML table; forwarded to the run factory's
    /// context-manager selection.
    #[serde(default)]
    pub compaction: Option<synthia::context::CompactionSettings>,
    /// R50: the reasoning loop this agent runs. One of
    /// `react`, `chain-of-thought`, or `best-of-n`
    /// ([`synthia::harness::KNOWN_STRATEGY_NAMES`]);
    /// unset means ReAct. An unrecognised value is logged at boot and
    /// the agent falls back to ReAct rather than refusing to start.
    #[serde(default)]
    pub strategy: Option<String>,
    /// R62: per-agent OS sandbox policy override (Codex `[sandbox]`
    /// agent table parity). Wire form: `"read-only"` /
    /// `"workspace-write"` / `"danger-full-access"`. `None` keeps
    /// the server's boot-time choice. Unknown values are logged at
    /// boot and the agent falls back to `None`.
    #[serde(default)]
    pub sandbox: Option<String>,
}

impl AgentConfig {
    /// Parse the per-agent sandbox wire name (R62). Unknown values
    /// return `None` so a log written by a newer build degrades
    /// gracefully — matching
    /// `synthia_tool_shell::sandbox::ExecutionPolicy::from_wire`
    /// without the server depending on the shell tool crate.
    #[must_use]
    pub fn sandbox_policy_name(&self) -> Option<&'static str> {
        match self.sandbox.as_deref()? {
            "read-only" => Some("read-only"),
            "workspace-write" => Some("workspace-write"),
            "danger-full-access" => Some("danger-full-access"),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- AgentConfig 7-field struct ---------------------------------

    /// `AgentConfig` MUST default ALL 10 fields when deserialized
    /// from `{}` (every field has a `#[serde(default)]`).
    #[test]
    fn agent_config_all_ten_fields_default_on_empty() {
        let a: AgentConfig = serde_json::from_str("{}").unwrap();
        assert!(a.description.is_none());
        assert!(a.model.is_none());
        assert!(a.max_steps.is_none());
        assert!(a.allowed_tools.is_empty());
        assert!(a.denied_tools.is_empty());
        assert!(!a.hidden);
        assert!(a.color.is_none());
        assert!(a.compaction.is_none());
        assert!(a.strategy.is_none());
        assert!(a.sandbox.is_none());
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
            compaction: None,
            strategy: None,
            sandbox: None,
        };
        assert!(!a.hidden);
    }

    /// `AgentConfig` MUST serialize ALL 10 fields, with
    /// `Option::None` rendered as JSON `null` and `Vec::new`
    /// as `[]` (no `skip_serializing_if`).
    #[test]
    fn agent_config_serializes_all_ten_fields_with_nulls() {
        let a = AgentConfig {
            description: None,
            model: None,
            max_steps: None,
            allowed_tools: vec![],
            denied_tools: vec![],
            hidden: false,
            color: None,
            compaction: None,
            strategy: None,
            sandbox: None,
        };
        let json: serde_json::Value = serde_json::to_value(&a).unwrap();
        let obj = json.as_object().expect("must be object");
        assert_eq!(obj.len(), 10);
        assert!(obj.contains_key("description"));
        assert!(obj.contains_key("model"));
        assert!(obj.contains_key("max_steps"));
        assert!(obj.contains_key("allowed_tools"));
        assert!(obj.contains_key("denied_tools"));
        assert!(obj.contains_key("hidden"));
        assert!(obj.contains_key("color"));
        assert!(obj.contains_key("compaction"));
        assert!(obj.contains_key("strategy"));
        assert!(obj.contains_key("sandbox"));
        // None values MUST serialize as null (no skip).
        assert_eq!(json["description"], serde_json::Value::Null);
        // Empty Vec MUST serialize as [].
        assert_eq!(json["allowed_tools"], serde_json::json!([]));
    }

    /// `AgentConfig` MUST round-trip all 10 fields through JSON.
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
            compaction: Some(synthia::context::CompactionSettings {
                reserve_tokens: 8_192,
                ..synthia::context::CompactionSettings::default()
            }),
            strategy: Some("chain-of-thought".to_string()),
            sandbox: Some("read-only".to_string()),
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
        assert_eq!(parsed.compaction, a.compaction);
        assert_eq!(parsed.strategy, a.strategy);
        assert_eq!(parsed.compaction.unwrap().reserve_tokens, 8_192);
    }

    /// R16: the `compaction` table deserialises from the wire
    /// shape the TOML/YAML bridge produces (`{ enabled, … }`).
    #[test]
    fn compaction_table_deserialises_from_toml_shape() {
        let json = r#"{
            "description": null,
            "model": null,
            "max_steps": null,
            "allowed_tools": [],
            "denied_tools": [],
            "hidden": false,
            "color": null,
            "compaction": {
                "enabled": true,
                "reserve_tokens": 8192,
                "keep_recent_tokens": 12000
            }
        }"#;
        let a: AgentConfig = serde_json::from_str(json).unwrap();
        let c = a.compaction.expect("compaction table parsed");
        assert!(c.enabled);
        assert_eq!(c.reserve_tokens, 8_192);
        assert_eq!(c.keep_recent_tokens, 12_000);
        // Defaults fill the omitted throttle.
        assert_eq!(c.min_messages_between_compaction, 4);
    }
}
