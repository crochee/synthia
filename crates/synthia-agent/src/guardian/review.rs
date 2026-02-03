//! Guardian 审查逻辑
//!
//! 此模块实现 Guardian 系统的核心审查逻辑，包括审查循环和决策制定。

use std::sync::Arc;

use async_trait::async_trait;
use rmcp::model::{
    CreateMessageRequestParams,
    ModelHint,
    ModelPreferences,
    SamplingMessage,
};
use synthia_provider::collect_stream;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

use super::{
    ApprovalRequest,
    Assessment,
    GuardianConfig,
    GuardianOption,
    ReviewDecision,
    build_review_prompt,
    collect_transcript_entries,
    parse_assessment_response,
};
use crate::{
    Result,
    context::ContextManager,
    model_router::ModelRouter,
    utils::extract_text_content,
};

/// Risk score threshold for automatic approval
pub const GUARDIAN_APPROVAL_RISK_THRESHOLD: u8 = 80;

/// Risk score result
#[derive(Debug, Clone)]
pub struct RiskScore {
    pub score: u8,
    pub factors: Vec<String>,
}

impl RiskScore {
    pub fn new(score: u8, factors: Vec<String>) -> Self {
        Self {
            score: score.min(100),
            factors,
        }
    }
}

/// Guardian 安全审查接口
///
/// 此 trait 定义安全审查系统的核心功能。
#[async_trait]
pub trait Guardian: Send + Sync {
    /// 审查操作并返回决策
    ///
    /// 返回:
    /// - Ok(Some(ReviewDecision::Approved)): 审查通过
    /// - Ok(Some(ReviewDecision::Denied { .. })): 审查拒绝
    /// - Ok(Some(ReviewDecision::NeedsUserInput { .. })): 需要用户交互才能决定
    /// - Ok(None): 审查被跳过（例如：已禁用）
    /// - Err(error): 审查失败
    async fn review(
        &self,
        cancel_token: &CancellationToken,
        request: ApprovalRequest,
    ) -> Result<Option<ReviewDecision>>;

    /// 检查工具是否需要 Guardian 审查（危险工具）
    fn is_dangerous_tool(&self, tool_name: &str) -> bool;
}

/// 简化的 Guardian 实现
///
/// 此实现提供基础安全审查，无需复杂的模型交互。
#[derive(Debug)]
pub struct SimpleGuardian {
    config: GuardianConfig,
}

impl SimpleGuardian {
    /// 创建新的 SimpleGuardian 实例
    pub fn new(config: GuardianConfig) -> Self {
        Self { config }
    }

    /// 评估请求的风险分数
    fn assess_risk(&self, request: &ApprovalRequest) -> u8 {
        match request {
            ApprovalRequest::Shell { command, .. } => {
                if command
                    .iter()
                    .any(|cmd| cmd.contains("rm") || cmd.contains("sudo"))
                {
                    85
                } else {
                    30
                }
            }
            ApprovalRequest::ApplyPatch { patch, .. } => {
                if patch.contains("rm -rf") || patch.contains("sudo") {
                    90
                } else {
                    40
                }
            }
            _ => 20,
        }
    }
}

#[async_trait]
impl Guardian for SimpleGuardian {
    async fn review(
        &self,
        cancel_token: &CancellationToken,
        request: ApprovalRequest,
    ) -> Result<Option<ReviewDecision>> {
        if !self.config.enabled {
            return Ok(None);
        }
        if cancel_token.is_cancelled() {
            return Ok(None);
        }

        let risk_score = self.assess_risk(&request);

        if risk_score < self.config.risk_threshold {
            Ok(Some(ReviewDecision::Approved))
        } else {
            Ok(Some(ReviewDecision::Denied {
                reason: format!(
                    "Risk score {} exceeds threshold {}",
                    risk_score, self.config.risk_threshold
                ),
            }))
        }
    }

    fn is_dangerous_tool(&self, tool_name: &str) -> bool {
        self.config.dangerous_tools.contains(&tool_name.to_string())
    }
}

/// 使用模型评估的高级 Guardian 实现
pub struct AdvancedGuardian {
    config: GuardianConfig,
    reviewer: GuardianReviewer,
    router: Arc<dyn ModelRouter>,
    context_manager: Arc<dyn ContextManager>,
}

impl std::fmt::Debug for AdvancedGuardian {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AdvancedGuardian")
            .field("config", &self.config)
            .field("reviewer", &self.reviewer)
            .field("router", &"<ModelRouter>")
            .finish()
    }
}

impl AdvancedGuardian {
    /// 创建新的 AdvancedGuardian 实例
    pub fn new(
        config: GuardianConfig,
        router: Arc<dyn ModelRouter>,
        context_manager: Arc<dyn ContextManager>,
    ) -> Self {
        let reviewer = GuardianReviewer::new(config.clone());
        Self {
            config,
            reviewer,
            router,
            context_manager,
        }
    }
}

#[async_trait]
impl Guardian for AdvancedGuardian {
    async fn review(
        &self,
        cancel_token: &CancellationToken,
        request: ApprovalRequest,
    ) -> Result<Option<ReviewDecision>> {
        if !self.config.enabled {
            return Ok(None);
        }
        if cancel_token.is_cancelled() {
            return Ok(None);
        }

        let conversation = self.context_manager.get_recent_messages(50).await?;
        let decision = self
            .reviewer
            .review(cancel_token.clone(), request, &conversation, &self.router)
            .await?;

        Ok(Some(decision))
    }

    fn is_dangerous_tool(&self, tool_name: &str) -> bool {
        self.config.dangerous_tools.contains(&tool_name.to_string())
    }
}

/// Guardian 审查器
///
/// 执行实际的审查逻辑，包括构建提示词、调用模型和解析响应。
#[derive(Clone)]
pub struct GuardianReviewer {
    config: GuardianConfig,
}

impl std::fmt::Debug for GuardianReviewer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GuardianReviewer")
            .field("enabled", &self.config.enabled)
            .field("risk_threshold", &self.config.risk_threshold)
            .finish()
    }
}

impl GuardianReviewer {
    /// 创建新的审查器实例
    pub fn new(config: GuardianConfig) -> Self {
        Self { config }
    }

    /// 执行审查
    pub async fn review(
        &self,
        cancel_token: CancellationToken,
        request: ApprovalRequest,
        conversation: &[SamplingMessage],
        router: &Arc<dyn ModelRouter>,
    ) -> Result<ReviewDecision> {
        if !self.config.enabled {
            info!("Guardian is disabled, auto-approving action");
            return Ok(ReviewDecision::Approved);
        }

        let action_json = match request.to_json() {
            Ok(json) => serde_json::to_string_pretty(&json).unwrap_or_default(),
            Err(e) => {
                error!("Failed to serialize approval request: {}", e);
                return Ok(ReviewDecision::Denied {
                    reason: "Failed to serialize approval request".to_string(),
                });
            }
        };

        let action_summary = request.action_summary();
        info!("Reviewing action: {}", action_summary);

        let review_prompt = build_review_prompt(
            &collect_transcript_entries(conversation),
            &action_json,
            None,
        );

        // 使用 router 获取 provider 和模型配置
        let routing_result = router.route(conversation).await?;
        let provider = &routing_result.provider;

        info!(
            "Guardian using model: {} (provider: {})",
            routing_result.decision.selected_model,
            routing_result.decision.provider_type
        );

        let params = CreateMessageRequestParams {
            meta: None,
            task: None,
            messages: vec![SamplingMessage::user_text(review_prompt)],
            model_preferences: Some(ModelPreferences {
                hints: Some(vec![ModelHint {
                    name: Some(routing_result.decision.selected_model.clone()),
                }]),
                cost_priority: None,
                speed_priority: None,
                intelligence_priority: None,
            }),
            system_prompt: Some(
                "You are a security reviewer. Respond only with valid JSON."
                    .to_string(),
            ),
            include_context: None,
            temperature: Some(0.0),
            max_tokens: 1024,
            stop_sequences: None,
            metadata: None,
            tools: None,
            tool_choice: None,
        };
        let stream = provider.stream(params, cancel_token).await?;
        let assessment = collect_stream(stream).await?;

        let text_content = extract_text_content(&assessment.message);
        let decision = parse_assessment_response(&text_content)?;
        Ok(self.make_decision(decision))
    }

    /// 根据评估结果作出决策
    fn make_decision(&self, assessment: Assessment) -> ReviewDecision {
        let threshold = self.config.risk_threshold;
        let risk_score = assessment.risk_score;

        // Medium risk: 50-80 requires user interaction
        if risk_score >= 50 && risk_score < threshold {
            info!(
                "Action requires user input (risk: {}/{})",
                risk_score, threshold
            );
            return ReviewDecision::NeedsUserInput {
                question: format!(
                    "This action has medium risk (score: {risk_score}). Do you want to proceed?"
                ),
                options: vec![
                    GuardianOption {
                        id: "yes".to_string(),
                        label: "Yes".to_string(),
                        description: "Proceed with this action".to_string(),
                    },
                    GuardianOption {
                        id: "no".to_string(),
                        label: "No".to_string(),
                        description: "Cancel this action".to_string(),
                    },
                    GuardianOption {
                        id: "cancel".to_string(),
                        label: "Cancel".to_string(),
                        description: "Cancel and stop further similar requests"
                            .to_string(),
                    },
                ],
            };
        }

        if risk_score < threshold {
            info!("Action approved (risk: {}/{})", risk_score, threshold);
            ReviewDecision::Approved
        } else {
            warn!("Action denied (risk: {}/{})", risk_score, threshold);
            ReviewDecision::Denied {
                reason: assessment.rationale,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::guardian::{Assessment, Evidence, GuardianRiskLevel};

    #[test]
    fn test_risk_threshold_approval() {
        let config = GuardianConfig::default()
            .enabled(true)
            .with_risk_threshold(80);
        let reviewer = GuardianReviewer::new(config);

        let assessment = Assessment {
            risk_level: GuardianRiskLevel::Low,
            risk_score: 30,
            rationale: "Safe".to_string(),
            evidence: vec![],
        };

        assert!(assessment.risk_score < reviewer.config.risk_threshold);
    }

    #[test]
    fn test_simple_guardian_risk_assessment() {
        let config = GuardianConfig::default()
            .enabled(true)
            .with_risk_threshold(80);
        let guardian = SimpleGuardian::new(config);

        // 测试高风险命令
        let risky_request = ApprovalRequest::shell(
            "id",
            vec!["sudo".to_string(), "rm".to_string()],
            "/",
            None,
        );
        assert_eq!(guardian.assess_risk(&risky_request), 85);

        // 测试低风险命令
        let safe_request =
            ApprovalRequest::shell("id", vec!["ls".to_string()], "/", None);
        assert_eq!(guardian.assess_risk(&safe_request), 30);
    }

    #[test]
    fn test_guardian_reviewer_make_decision() {
        let config = GuardianConfig::default()
            .enabled(true)
            .with_risk_threshold(80);
        let reviewer = GuardianReviewer::new(config);

        // 测试批准决策
        let approved_assessment = Assessment {
            risk_level: GuardianRiskLevel::Low,
            risk_score: 30,
            rationale: "Safe operation".to_string(),
            evidence: vec![],
        };

        match reviewer.make_decision(approved_assessment) {
            ReviewDecision::Approved => {}
            _ => panic!("Expected Approved decision"),
        }

        // 测试拒绝决策
        let denied_assessment = Assessment {
            risk_level: GuardianRiskLevel::High,
            risk_score: 90,
            rationale: "Too risky".to_string(),
            evidence: vec![Evidence {
                message: "Found dangerous command".to_string(),
                why: "rm -rf detected".to_string(),
            }],
        };

        match reviewer.make_decision(denied_assessment) {
            ReviewDecision::Denied { reason } => {
                assert_eq!(reason, "Too risky")
            }
            _ => panic!("Expected Denied decision"),
        }

        // 测试 NeedsUserInput 决策 (中等风险 50-79)
        let medium_assessment = Assessment {
            risk_level: GuardianRiskLevel::Medium,
            risk_score: 65,
            rationale: "Moderate risk operation".to_string(),
            evidence: vec![],
        };

        match reviewer.make_decision(medium_assessment) {
            ReviewDecision::NeedsUserInput { question, options } => {
                assert!(question.contains("65"));
                assert_eq!(options.len(), 3);
            }
            _ => panic!("Expected NeedsUserInput decision for medium risk"),
        }
    }
}
