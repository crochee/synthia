//! Guardian 审查逻辑
//!
//! 此模块实现 Guardian 系统的核心审查逻辑，包括 trait 定义、
//! 简单启发式实现 [`SimpleGuardian`]、以及 LLM 驱动的
//! [`reviewer::GuardianReviewer`]。
//!
//! # 模块结构
//!
//! - [`super`]: 公共 trait [`Guardian`] + 风险评分类型
//!   [`RiskScore`] + 简单启发式实现 [`SimpleGuardian`] +
//!   自动审批风险阈值常量 [`GUARDIAN_APPROVAL_RISK_THRESHOLD`]。
//! - [`reviewer`][]: LLM 驱动的 [`reviewer::GuardianReviewer`]
//!   —— 生产环境的实际审查路径（含 prompt 构造、router
//!   调用、assessment 解析、决策映射）。
//! - [`tests`][]: 所有审查相关单元测试。

use async_trait::async_trait;
use synthia_provider::Message;
use tokio_util::sync::CancellationToken;

use crate::{
    ApprovalRequest,
    GuardianConfig,
    GuardianSubagentFactory,
    InjectionScanner,
    ReviewDecision,
    guardian_decision::*,
};

pub mod reviewer;

#[allow(clippy::module_inception)]
#[cfg(test)]
mod tests;

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
    ) -> anyhow::Result<Option<ReviewDecision>>;

    /// 检查工具是否需要 Guardian 审查（危险工具）
    fn is_dangerous_tool(&self, tool_name: &str) -> bool;

    /// Hybrid Guardian check with risk-tier dispatch.
    ///
    /// Returns a [`GuardianDecision`] based on risk-tier:
    /// - risk < 50 → [`GuardianDecision::Allow`] (fast-path)
    /// - risk >= 80 → [`GuardianDecision::Deny`] (fast-path)
    /// - risk in [50, 80) → escalate to subagent review
    ///   (if `subagent_factory` is `Some` and subagent is enabled),
    ///   otherwise [`GuardianDecision::NeedUserConfirm`] (legacy path)
    ///
    /// `conversation` provides session context for the subagent review.
    /// `cancel_token` propagates parent cancellation into the subagent.
    /// `subagent_factory` gates the escalation path: `None` forces legacy.
    async fn check(
        &self,
        request: &ApprovalRequest,
        conversation: &[Message],
        cancel_token: CancellationToken,
        subagent_factory: Option<&dyn GuardianSubagentFactory>,
    ) -> GuardianDecision;
}

/// 简化的 Guardian 实现
///
/// 此实现提供基础安全审查，无需复杂的模型交互。
#[derive(Debug)]
pub struct SimpleGuardian {
    config: GuardianConfig,
    injection_scanner: InjectionScanner,
}

impl SimpleGuardian {
    pub fn new(config: GuardianConfig) -> Self {
        Self {
            config,
            injection_scanner: InjectionScanner::new(),
        }
    }

    /// 评估请求的风险分数 (0-100)
    pub fn assess_risk(&self, request: &ApprovalRequest) -> u8 {
        match request {
            ApprovalRequest::Shell { command, .. }
            | ApprovalRequest::ExecCommand { command, .. } => {
                let cmd_str = command.join(" ");
                if cmd_str.contains("rm -rf")
                    || cmd_str.contains("sudo")
                    || cmd_str.contains("chmod 777")
                {
                    90
                } else if cmd_str.contains("curl")
                    && (cmd_str.contains("-H") || cmd_str.contains("--header"))
                {
                    75
                } else if cmd_str.contains("export")
                    && (cmd_str.contains("SECRET")
                        || cmd_str.contains("KEY")
                        || cmd_str.contains("TOKEN"))
                {
                    85
                } else {
                    30
                }
            }
            ApprovalRequest::ApplyPatch { patch, .. } => {
                if patch.contains("rm -rf")
                    || patch.contains("sudo")
                    || patch.contains("chmod 777")
                {
                    90
                } else {
                    40
                }
            }
            ApprovalRequest::NetworkAccess { .. } => {
                // Network access is medium-high risk
                65
            }
            ApprovalRequest::McpToolCall {
                tool_name: _,
                annotations,
                arguments,
                ..
            } => {
                // Check MCP annotations for risk hints
                if let Some(ann) = annotations {
                    if ann.destructive_hint == Some(true) {
                        return 85;
                    }
                    if ann.open_world_hint == Some(true) {
                        return 70;
                    }
                }
                // V1 security fix: scan args_json for path traversal
                if let Some(args) = arguments {
                    let injection_matches =
                        self.injection_scanner.scan_args_json(args);
                    if !injection_matches.is_empty() {
                        return 90; // Critical: injection detected in args
                    }
                }
                // Default to medium risk for MCP tools
                50
            }
        }
    }

    /// Quick check - returns GuardianDecision for the hybrid layer
    pub async fn check(&self, request: &ApprovalRequest) -> GuardianDecision {
        if !self.config.enabled {
            return GuardianDecision::Allow;
        }

        let risk_score = self.assess_risk(request);

        // Low risk (< 50): allow
        if risk_score < 50 {
            return GuardianDecision::Allow;
        }

        // High risk (>= 80): deny with reason
        if risk_score >= 80 {
            return GuardianDecision::Deny {
                reason: format!("Risk score {} exceeds threshold", risk_score),
            };
        }

        // Medium risk (50-79): need user confirm
        let action_type = ActionType::from_approval_request(request);
        GuardianDecision::NeedUserConfirm {
            request: Box::new(request.clone()),
            timeout: action_type.default_timeout(),
            blocking: action_type.is_blocking(),
            action_type,
        }
    }
}

#[async_trait]
impl Guardian for SimpleGuardian {
    async fn review(
        &self,
        cancel_token: &CancellationToken,
        request: ApprovalRequest,
    ) -> anyhow::Result<Option<ReviewDecision>> {
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

    /// Fast-path only check: ignores `conversation`, `cancel_token`,
    /// and `subagent_factory`. Delegates to the inherent
    /// [`SimpleGuardian::check`] which handles the risk-tier dispatch
    /// (low → Allow, high → Deny, medium → NeedUserConfirm).
    async fn check(
        &self,
        request: &ApprovalRequest,
        _conversation: &[Message],
        _cancel_token: CancellationToken,
        _subagent_factory: Option<&dyn GuardianSubagentFactory>,
    ) -> GuardianDecision {
        self.check(request).await
    }
}
