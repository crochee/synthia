//! Guardian 核心类型定义
//!
//! 此模块集中定义 Guardian 安全审查系统的所有核心类型，
//! 包括风险评估结果、审查决策和相关数据结构。

use serde::{Deserialize, Serialize};

use super::config::GuardianRiskLevel;

/// 风险评估结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Assessment {
    pub risk_level: GuardianRiskLevel,
    pub risk_score: u8,
    pub rationale: String,
    pub evidence: Vec<Evidence>,
}

/// 风险证据项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Evidence {
    pub message: String,
    pub why: String,
}

/// Guardian 选项，用于需要用户交互的场景
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardianOption {
    pub id: String,
    pub label: String,
    pub description: String,
}

/// 审查决策结果
#[derive(Debug, Clone)]
pub enum ReviewDecision {
    Approved,
    Denied {
        reason: String,
    },
    /// 需要用户输入以继续审查
    NeedsUserInput {
        question: String,
        options: Vec<GuardianOption>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_review_decision_variants() {
        let approved = ReviewDecision::Approved;
        let denied = ReviewDecision::Denied {
            reason: "Too risky".to_string(),
        };
        let needs_input = ReviewDecision::NeedsUserInput {
            question: "Are you sure?".to_string(),
            options: vec![GuardianOption {
                id: "yes".to_string(),
                label: "Yes".to_string(),
                description: "Proceed".to_string(),
            }],
        };

        match approved {
            ReviewDecision::Approved => {}
            _ => panic!("Expected Approved variant"),
        }

        match denied {
            ReviewDecision::Denied { reason } => {
                assert_eq!(reason, "Too risky")
            }
            _ => panic!("Expected Denied variant"),
        }

        match needs_input {
            ReviewDecision::NeedsUserInput { question, options } => {
                assert_eq!(question, "Are you sure?");
                assert_eq!(options.len(), 1);
            }
            _ => panic!("Expected NeedsUserInput variant"),
        }
    }
}
