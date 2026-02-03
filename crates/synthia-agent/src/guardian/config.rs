//! Guardian 配置
//!
//! Guardian 安全审查系统的配置选项。

use serde::{Deserialize, Serialize};

/// 风险等级
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "lowercase")]
pub enum GuardianRiskLevel {
    Low,
    #[default]
    Medium,
    High,
}

impl GuardianRiskLevel {
    /// 返回风险等级的字符串表示
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }
}

/// Guardian 运行模式
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GuardianMode {
    /// 简单模式：使用基于规则的风险评估
    Simple,
    /// 高级模式：使用 AI 模型进行评估
    Advanced { model_name: String },
}

/// Guardian 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardianConfig {
    /// 是否启用 Guardian
    pub enabled: bool,
    /// 风险阈值（0-100），超过此值将被拒绝
    pub risk_threshold: u8,
    /// 最大重试次数
    pub max_retries: u32,
    /// 运行模式
    pub mode: GuardianMode,
    /// 需要 Guardian 审查的危险工具列表
    pub dangerous_tools: Vec<String>,
    /// 需要 Guardian 审查的危险工具正则模式
    pub dangerous_patterns: Vec<String>,
}

impl Default for GuardianConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            risk_threshold: 80,
            max_retries: 3,
            mode: GuardianMode::Simple,
            dangerous_tools: vec![
                "exec".to_string(),
                "bash".to_string(),
                "shell".to_string(),
                "apply_patch".to_string(),
            ],
            dangerous_patterns: vec![],
        }
    }
}

impl GuardianConfig {
    /// 设置启用状态
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// 设置风险阈值
    pub fn with_risk_threshold(mut self, threshold: u8) -> Self {
        self.risk_threshold = threshold;
        self
    }

    /// 设置运行模式
    pub fn with_mode(mut self, mode: GuardianMode) -> Self {
        self.mode = mode;
        self
    }

    /// 设置最大重试次数
    pub fn with_max_retries(mut self, max_retries: u32) -> Self {
        self.max_retries = max_retries;
        self
    }

    /// 设置危险工具列表
    pub fn with_dangerous_tools(mut self, tools: Vec<String>) -> Self {
        self.dangerous_tools = tools;
        self
    }

    /// 设置危险工具正则模式
    pub fn with_dangerous_patterns(mut self, patterns: Vec<String>) -> Self {
        self.dangerous_patterns = patterns;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_risk_level_default() {
        let level = GuardianRiskLevel::default();
        assert_eq!(level, GuardianRiskLevel::Medium);
    }

    #[test]
    fn test_risk_level_as_str() {
        assert_eq!(GuardianRiskLevel::Low.as_str(), "low");
        assert_eq!(GuardianRiskLevel::Medium.as_str(), "medium");
        assert_eq!(GuardianRiskLevel::High.as_str(), "high");
    }

    #[test]
    fn test_guardian_config_default() {
        let config = GuardianConfig::default();
        assert!(config.enabled); // Guardian enabled by default for security
        assert_eq!(config.risk_threshold, 80);
        assert_eq!(config.max_retries, 3);
        assert!(matches!(config.mode, GuardianMode::Simple));
        // Verify dangerous tools default list
        assert!(config.dangerous_tools.contains(&"exec".to_string()));
        assert!(config.dangerous_tools.contains(&"bash".to_string()));
        assert!(config.dangerous_tools.contains(&"shell".to_string()));
        assert!(config.dangerous_tools.contains(&"apply_patch".to_string()));
        assert!(config.dangerous_patterns.is_empty());
    }

    #[test]
    fn test_guardian_config_builder() {
        let config = GuardianConfig::default()
            .enabled(true)
            .with_risk_threshold(70)
            .with_max_retries(5)
            .with_mode(GuardianMode::Advanced {
                model_name: "gpt-4".to_string(),
            });

        assert!(config.enabled);
        assert_eq!(config.risk_threshold, 70);
        assert_eq!(config.max_retries, 5);
        assert!(matches!(config.mode, GuardianMode::Advanced { .. }));
    }

    #[test]
    fn test_guardian_config_builder_chain() {
        let config = GuardianConfig::default()
            .enabled(false)
            .with_risk_threshold(50)
            .with_max_retries(10)
            .with_dangerous_tools(vec!["custom_tool".to_string()])
            .with_dangerous_patterns(vec![r"rm\s+-rf".to_string()]);

        assert!(!config.enabled);
        assert_eq!(config.risk_threshold, 50);
        assert_eq!(config.max_retries, 10);
        assert_eq!(config.dangerous_tools, vec!["custom_tool".to_string()]);
        assert_eq!(config.dangerous_patterns, vec![r"rm\s+-rf".to_string()]);
    }

    #[test]
    fn test_guardian_mode_simple() {
        let mode = GuardianMode::Simple;
        match mode {
            GuardianMode::Simple => {}
            _ => panic!("Expected Simple mode"),
        }
    }

    #[test]
    fn test_guardian_mode_advanced() {
        let mode = GuardianMode::Advanced {
            model_name: "claude-3".to_string(),
        };
        match mode {
            GuardianMode::Advanced { model_name } => {
                assert_eq!(model_name, "claude-3");
            }
            _ => panic!("Expected Advanced mode"),
        }
    }

    #[test]
    fn test_guardian_config_debug() {
        let config = GuardianConfig::default();
        let debug_str = format!("{config:?}");
        assert!(debug_str.contains("GuardianConfig"));
        assert!(debug_str.contains("enabled"));
        assert!(debug_str.contains("risk_threshold"));
    }

    #[test]
    fn test_risk_level_serde() {
        // Test serialization
        let level = GuardianRiskLevel::High;
        let json = serde_json::to_string(&level).unwrap();
        assert_eq!(json, "\"high\"");

        // Test deserialization
        let parsed: GuardianRiskLevel = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, GuardianRiskLevel::High);
    }

    #[test]
    fn test_guardian_config_serde() {
        let config = GuardianConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let parsed: GuardianConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.enabled, config.enabled);
        assert_eq!(parsed.risk_threshold, config.risk_threshold);
        assert_eq!(parsed.max_retries, config.max_retries);
    }

    #[test]
    fn test_guardian_mode_serde() {
        let mode = GuardianMode::Advanced {
            model_name: "gpt-4o".to_string(),
        };
        let json = serde_json::to_string(&mode).unwrap();
        let parsed: GuardianMode = serde_json::from_str(&json).unwrap();
        match parsed {
            GuardianMode::Advanced { model_name } => {
                assert_eq!(model_name, "gpt-4o");
            }
            _ => panic!("Expected Advanced mode after deserialization"),
        }
    }

    #[test]
    fn test_enabled_method() {
        let disabled = GuardianConfig::default().enabled(false);
        assert!(!disabled.enabled);

        let enabled = GuardianConfig::default().enabled(true);
        assert!(enabled.enabled);
    }

    #[test]
    fn test_risk_threshold_bounds() {
        // Test boundary values
        let low = GuardianConfig::default().with_risk_threshold(0);
        assert_eq!(low.risk_threshold, 0);

        let high = GuardianConfig::default().with_risk_threshold(100);
        assert_eq!(high.risk_threshold, 100);
    }
}
