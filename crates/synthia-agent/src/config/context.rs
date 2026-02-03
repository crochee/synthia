//! Context management configuration
//!
//! Simplified configuration for context compression and management.
//! All values have sensible defaults, so no configuration is required.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Tool importance level for differentiated processing
#[derive(
    Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq,
)]
#[serde(rename_all = "snake_case")]
pub enum ToolImportance {
    /// Critical tools are never removed (e.g., skill, system)
    Critical,
    /// High importance tools are prioritized for retention
    High,
    /// Normal tools can be pruned
    #[default]
    Normal,
    /// Low priority tools can be cleared
    Low,
}

/// Simplified context management configuration
///
/// Example TOML configuration:
/// ```toml
/// [context]
/// trigger_threshold = 0.8       # Trigger compaction at 80% usage
/// reserved_tokens = 20000       # Reserve 20k tokens for response
/// trigger_ratio = 0.85          # Trigger ratio for dual-condition
/// min_buffer_tokens = 5000      # Minimum buffer for dual-condition
/// soft_threshold = 0.5          # Soft pruning threshold
/// hard_threshold = 0.75         # Hard clearing threshold
/// critical_threshold = 0.9      # Emergency truncation threshold
/// keep_recent_turns = 3         # Keep recent N turns
/// quality_check_enabled = true  # Enable quality check
/// micro_threshold = 0.6         # Micro compact threshold
/// target_ratio = 0.5            # Target ratio after compression
/// ```
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ContextConfig {
    /// Usage ratio threshold to trigger compaction (default: 0.8 = 80%)
    #[serde(default = "default_trigger_threshold")]
    pub trigger_threshold: f64,

    /// Reserved token space for model response (default: 20000)
    #[serde(default = "default_reserved_tokens")]
    pub reserved_tokens: usize,

    /// Trigger ratio for dual-condition triggering (default: 0.85)
    #[serde(default = "default_trigger_ratio")]
    pub trigger_ratio: f64,

    /// Minimum buffer tokens for dual-condition triggering (default: 5000)
    #[serde(default = "default_min_buffer_tokens")]
    pub min_buffer_tokens: usize,

    /// Soft pruning threshold (default: 0.5)
    #[serde(default = "default_soft_threshold")]
    pub soft_threshold: f64,

    /// Hard clearing threshold (default: 0.75)
    #[serde(default = "default_hard_threshold")]
    pub hard_threshold: f64,

    /// Critical threshold for emergency truncation (default: 0.9)
    #[serde(default = "default_critical_threshold")]
    pub critical_threshold: f64,

    /// Number of recent turns to preserve (default: 3)
    #[serde(default = "default_keep_recent_turns")]
    pub keep_recent_turns: usize,

    /// Enable quality check for summaries (default: true)
    #[serde(default = "default_quality_check_enabled")]
    pub quality_check_enabled: bool,

    /// Maximum tokens for summary generation (default: 4096)
    #[serde(default = "default_summary_max_tokens")]
    pub summary_max_tokens: usize,

    /// Tool importance mapping (optional, uses defaults if not specified)
    #[serde(default)]
    pub tool_importance: HashMap<String, ToolImportance>,

    /// Whether to preserve user messages during summarization (default: true)
    #[serde(default = "default_preserve_user_messages")]
    pub preserve_user_messages: bool,

    /// Whether to preserve critical tool results during compression (default: true)
    #[serde(default = "default_preserve_critical_tools")]
    pub preserve_critical_tools: bool,

    /// Threshold to trigger micro compaction (default: 0.6)
    #[serde(default = "default_micro_threshold")]
    pub micro_threshold: f64,

    /// Target ratio for compression (default: 0.5)
    #[serde(default = "default_target_ratio")]
    pub target_ratio: f64,

    /// List of critical tool names that should be preserved (default: ["read", "write", "edit"])
    #[serde(default = "default_critical_tools")]
    pub critical_tools: Vec<String>,
}

impl Default for ContextConfig {
    fn default() -> Self {
        Self {
            trigger_threshold: default_trigger_threshold(),
            reserved_tokens: default_reserved_tokens(),
            trigger_ratio: default_trigger_ratio(),
            min_buffer_tokens: default_min_buffer_tokens(),
            soft_threshold: default_soft_threshold(),
            hard_threshold: default_hard_threshold(),
            critical_threshold: default_critical_threshold(),
            keep_recent_turns: default_keep_recent_turns(),
            quality_check_enabled: default_quality_check_enabled(),
            summary_max_tokens: default_summary_max_tokens(),
            tool_importance: default_tool_importance(),
            preserve_user_messages: default_preserve_user_messages(),
            preserve_critical_tools: default_preserve_critical_tools(),
            micro_threshold: default_micro_threshold(),
            target_ratio: default_target_ratio(),
            critical_tools: default_critical_tools(),
        }
    }
}

impl ContextConfig {
    /// Get the importance level for a tool
    pub fn get_tool_importance(&self, tool_name: &str) -> ToolImportance {
        self.tool_importance
            .get(tool_name)
            .copied()
            .unwrap_or_else(|| classify_tool_default(tool_name))
    }

    /// Check if a tool is in the critical tools list
    pub fn is_critical_tool(&self, tool_name: &str) -> bool {
        self.critical_tools.iter().any(|t| t == tool_name)
    }
}

fn default_trigger_threshold() -> f64 {
    0.8
}

fn default_reserved_tokens() -> usize {
    20_000
}

fn default_trigger_ratio() -> f64 {
    0.85
}

fn default_min_buffer_tokens() -> usize {
    5_000
}

fn default_soft_threshold() -> f64 {
    0.5
}

fn default_hard_threshold() -> f64 {
    0.75
}

fn default_critical_threshold() -> f64 {
    0.9
}

fn default_keep_recent_turns() -> usize {
    3
}

fn default_quality_check_enabled() -> bool {
    true
}

fn default_summary_max_tokens() -> usize {
    4096
}

fn default_tool_importance() -> HashMap<String, ToolImportance> {
    let mut map = HashMap::new();
    map.insert("skill".to_string(), ToolImportance::Critical);
    map.insert("system".to_string(), ToolImportance::Critical);
    map.insert("config".to_string(), ToolImportance::Critical);
    map.insert("read".to_string(), ToolImportance::High);
    map.insert("write".to_string(), ToolImportance::High);
    map.insert("edit".to_string(), ToolImportance::High);
    map
}

fn default_preserve_user_messages() -> bool {
    true
}

fn default_preserve_critical_tools() -> bool {
    true
}

fn default_micro_threshold() -> f64 {
    0.6
}

fn default_target_ratio() -> f64 {
    0.5
}

fn default_critical_tools() -> Vec<String> {
    vec!["read".to_string(), "write".to_string(), "edit".to_string()]
}

/// Default tool classification
pub fn classify_tool_default(tool_name: &str) -> ToolImportance {
    match tool_name {
        "skill" | "system" | "config" => ToolImportance::Critical,
        "read" | "write" | "edit" => ToolImportance::High,
        "search" | "grep" | "find" => ToolImportance::Normal,
        _ => ToolImportance::Low,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_config_default() {
        let config = ContextConfig::default();
        assert_eq!(config.trigger_threshold, 0.8);
        assert_eq!(config.reserved_tokens, 20_000);
        assert_eq!(config.trigger_ratio, 0.85);
        assert_eq!(config.min_buffer_tokens, 5_000);
        assert_eq!(config.soft_threshold, 0.5);
        assert_eq!(config.hard_threshold, 0.75);
        assert_eq!(config.critical_threshold, 0.9);
        assert_eq!(config.keep_recent_turns, 3);
        assert!(config.quality_check_enabled);
        assert_eq!(config.summary_max_tokens, 4096);
        assert!(config.preserve_user_messages);
        assert!(config.preserve_critical_tools);
        assert_eq!(config.micro_threshold, 0.6);
        assert_eq!(config.target_ratio, 0.5);
        assert_eq!(config.critical_tools, vec!["read", "write", "edit"]);
    }

    #[test]
    fn test_context_config_serialization() {
        let config = ContextConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let parsed: ContextConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.trigger_threshold, config.trigger_threshold);
        assert_eq!(parsed.reserved_tokens, config.reserved_tokens);
    }

    #[test]
    fn test_context_config_deserialization_partial() {
        let json = r#"{"trigger_threshold": 0.75}"#;
        let config: ContextConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.trigger_threshold, 0.75);
        assert_eq!(config.reserved_tokens, 20_000); // default value
    }

    #[test]
    fn test_tool_importance_classification() {
        let config = ContextConfig::default();
        assert_eq!(
            config.get_tool_importance("skill"),
            ToolImportance::Critical
        );
        assert_eq!(config.get_tool_importance("read"), ToolImportance::High);
        assert_eq!(
            config.get_tool_importance("search"),
            ToolImportance::Normal
        );
        assert_eq!(config.get_tool_importance("unknown"), ToolImportance::Low);
    }

    #[test]
    fn test_classify_tool_default() {
        assert_eq!(classify_tool_default("skill"), ToolImportance::Critical);
        assert_eq!(classify_tool_default("write"), ToolImportance::High);
        assert_eq!(classify_tool_default("grep"), ToolImportance::Normal);
        assert_eq!(classify_tool_default("other"), ToolImportance::Low);
    }

    #[test]
    fn test_is_critical_tool() {
        let config = ContextConfig::default();

        // Test default critical tools
        assert!(
            config.is_critical_tool("read"),
            "read should be a critical tool"
        );
        assert!(
            config.is_critical_tool("write"),
            "write should be a critical tool"
        );
        assert!(
            config.is_critical_tool("edit"),
            "edit should be a critical tool"
        );

        // Test non-critical tools
        assert!(
            !config.is_critical_tool("search"),
            "search should not be a critical tool by default"
        );
        assert!(
            !config.is_critical_tool("grep"),
            "grep should not be a critical tool"
        );
        assert!(
            !config.is_critical_tool("unknown"),
            "unknown tool should not be critical"
        );

        // Test with custom critical tools
        let custom_config = ContextConfig {
            critical_tools: vec!["custom1".to_string(), "custom2".to_string()],
            ..ContextConfig::default()
        };

        assert!(
            custom_config.is_critical_tool("custom1"),
            "custom1 should be critical"
        );
        assert!(
            custom_config.is_critical_tool("custom2"),
            "custom2 should be critical"
        );
        assert!(
            !custom_config.is_critical_tool("read"),
            "read should not be critical in custom config"
        );
    }

    #[test]
    fn test_context_config_deserialization_with_new_fields() {
        let json = r#"{
            "preserve_user_messages": false,
            "preserve_critical_tools": false,
            "micro_threshold": 0.5,
            "target_ratio": 0.4,
            "critical_tools": ["read", "search", "grep"]
        }"#;
        let config: ContextConfig = serde_json::from_str(json).unwrap();
        assert!(!config.preserve_user_messages);
        assert!(!config.preserve_critical_tools);
        assert_eq!(config.micro_threshold, 0.5);
        assert_eq!(config.target_ratio, 0.4);
        assert_eq!(config.critical_tools, vec!["read", "search", "grep"]);
        // Verify defaults for other fields
        assert_eq!(config.trigger_threshold, 0.8);
        assert_eq!(config.reserved_tokens, 20_000);
    }

    #[test]
    fn test_new_config_options() {
        // Test default values for new configuration options
        let config = ContextConfig::default();

        // Verify preserve_user_messages default
        assert!(
            config.preserve_user_messages,
            "preserve_user_messages should default to true"
        );

        // Verify preserve_critical_tools default
        assert!(
            config.preserve_critical_tools,
            "preserve_critical_tools should default to true"
        );

        // Verify micro_threshold default
        assert_eq!(
            config.micro_threshold, 0.6,
            "micro_threshold should default to 0.6"
        );

        // Verify target_ratio default
        assert_eq!(
            config.target_ratio, 0.5,
            "target_ratio should default to 0.5"
        );

        // Verify critical_tools default
        assert_eq!(
            config.critical_tools,
            vec!["read", "write", "edit"],
            "critical_tools should default to [read, write, edit]"
        );

        // Test custom values
        let custom_config = ContextConfig {
            preserve_user_messages: false,
            preserve_critical_tools: false,
            micro_threshold: 0.55,
            target_ratio: 0.45,
            critical_tools: vec!["custom_tool".to_string()],
            ..ContextConfig::default()
        };

        assert!(!custom_config.preserve_user_messages);
        assert!(!custom_config.preserve_critical_tools);
        assert_eq!(custom_config.micro_threshold, 0.55);
        assert_eq!(custom_config.target_ratio, 0.45);
        assert_eq!(custom_config.critical_tools, vec!["custom_tool"]);
    }

    #[test]
    fn test_tool_importance_enum_serde() {
        // Test JSON serialization of ToolImportance
        let critical = ToolImportance::Critical;
        let high = ToolImportance::High;
        let normal = ToolImportance::Normal;
        let low = ToolImportance::Low;

        let critical_json = serde_json::to_string(&critical).unwrap();
        let high_json = serde_json::to_string(&high).unwrap();
        let normal_json = serde_json::to_string(&normal).unwrap();
        let low_json = serde_json::to_string(&low).unwrap();

        assert_eq!(critical_json, "\"critical\"");
        assert_eq!(high_json, "\"high\"");
        assert_eq!(normal_json, "\"normal\"");
        assert_eq!(low_json, "\"low\"");

        // Test deserialization
        let parsed_critical: ToolImportance =
            serde_json::from_str(&critical_json).unwrap();
        let parsed_high: ToolImportance =
            serde_json::from_str(&high_json).unwrap();
        let parsed_normal: ToolImportance =
            serde_json::from_str(&normal_json).unwrap();
        let parsed_low: ToolImportance =
            serde_json::from_str(&low_json).unwrap();

        assert_eq!(parsed_critical, ToolImportance::Critical);
        assert_eq!(parsed_high, ToolImportance::High);
        assert_eq!(parsed_normal, ToolImportance::Normal);
        assert_eq!(parsed_low, ToolImportance::Low);
    }

    #[test]
    fn test_tool_importance_equality() {
        assert_eq!(ToolImportance::Critical, ToolImportance::Critical);
        assert_eq!(ToolImportance::High, ToolImportance::High);
        assert_ne!(ToolImportance::Critical, ToolImportance::High);
    }

    #[test]
    fn test_context_config_toml_serialization() {
        let config = ContextConfig {
            trigger_threshold: 0.9,
            reserved_tokens: 30_000,
            soft_threshold: 0.6,
            hard_threshold: 0.8,
            critical_threshold: 0.95,
            ..ContextConfig::default()
        };
        let tom = toml::to_string(&config).unwrap();
        let parsed: ContextConfig = toml::from_str(&tom).unwrap();
        assert_eq!(parsed.trigger_threshold, 0.9);
        assert_eq!(parsed.reserved_tokens, 30_000);
    }

    #[test]
    fn test_get_tool_importance_with_custom_map() {
        let mut tool_importance = HashMap::new();
        tool_importance
            .insert("custom_tool".to_string(), ToolImportance::Critical);
        tool_importance.insert("low_tool".to_string(), ToolImportance::Low);

        let config = ContextConfig {
            tool_importance,
            ..ContextConfig::default()
        };

        // Custom mapped tools
        assert_eq!(
            config.get_tool_importance("custom_tool"),
            ToolImportance::Critical
        );
        assert_eq!(config.get_tool_importance("low_tool"), ToolImportance::Low);

        // Default classification still works for unmapped tools
        assert_eq!(
            config.get_tool_importance("skill"),
            ToolImportance::Critical
        );
        assert_eq!(config.get_tool_importance("unknown"), ToolImportance::Low);
    }

    #[test]
    fn test_context_config_all_default_functions() {
        // Explicitly test all default function values
        assert_eq!(default_trigger_threshold(), 0.8);
        assert_eq!(default_reserved_tokens(), 20_000);
        assert_eq!(default_trigger_ratio(), 0.85);
        assert_eq!(default_min_buffer_tokens(), 5_000);
        assert_eq!(default_soft_threshold(), 0.5);
        assert_eq!(default_hard_threshold(), 0.75);
        assert_eq!(default_critical_threshold(), 0.9);
        assert_eq!(default_keep_recent_turns(), 3);
        assert!(default_quality_check_enabled());
        assert_eq!(default_summary_max_tokens(), 4096);
        assert!(default_preserve_user_messages());
        assert!(default_preserve_critical_tools());
        assert_eq!(default_micro_threshold(), 0.6);
        assert_eq!(default_target_ratio(), 0.5);
        assert_eq!(default_critical_tools(), vec!["read", "write", "edit"]);
    }

    #[test]
    fn test_default_tool_importance_map() {
        let map = default_tool_importance();
        assert_eq!(map.get("skill"), Some(&ToolImportance::Critical));
        assert_eq!(map.get("system"), Some(&ToolImportance::Critical));
        assert_eq!(map.get("config"), Some(&ToolImportance::Critical));
        assert_eq!(map.get("read"), Some(&ToolImportance::High));
        assert_eq!(map.get("write"), Some(&ToolImportance::High));
        assert_eq!(map.get("edit"), Some(&ToolImportance::High));
    }

    #[test]
    fn test_context_config_clone() {
        let config = ContextConfig::default();
        let cloned = config.clone();
        assert_eq!(cloned.trigger_threshold, config.trigger_threshold);
        assert_eq!(cloned.reserved_tokens, config.reserved_tokens);
        assert_eq!(cloned.critical_tools, config.critical_tools);
    }
}
