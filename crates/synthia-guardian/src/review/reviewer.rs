//! `GuardianReviewer` — the LLM-backed Guardian implementation.
//!
//! [`GuardianReviewer`] is the production review path: it builds
//! the review prompt, calls the LLM via the [`ModelRouter`],
//! parses the assessment, and maps it onto a
//! [`GuardianDecision`] / [`ReviewDecision`].
//!
//! Two public entry points:
//!
//! - [`GuardianReviewer::check`] — fast path for the hybrid
//!   layer. Returns a [`GuardianDecision`] with a timeout. On
//!   timeout it fails closed (returns
//!   [`GuardianDecision::Deny`]).
//! - [`GuardianReviewer::review`] — full review path. Returns a
//!   [`ReviewDecision`] (which may include
//!   [`ReviewDecision::NeedsUserInput`] for medium risk). No
//!   timeout — the caller's recovery layer decides how long to
//!   wait.
//!
//! Kept separate from [`super`] (the trait + simple heuristic
//! path) so the LLM-calling path's prompt construction / router
//! plumbing / `CompletionRequest` shaping lives in one focused
//! file.

use std::{sync::Arc, time::Duration};

use synthia_model_router::ModelRouter;
use synthia_provider::{
    CachePolicy,
    CompletionRequest,
    ContentPart,
    Message,
    TextContent,
    ToolChoice,
};
use tokio::time::timeout;
use tracing::{error, info, warn};

use crate::{
    ApprovalRequest,
    GuardianConfig,
    ReviewDecision,
    build_review_prompt,
    collect_transcript_entries,
    guardian_decision::{ActionType, GuardianDecision},
    parse_assessment_response,
    review_types::Assessment,
};

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// Guardian 审查器
pub struct GuardianReviewer {
    pub(super) config: GuardianConfig,
    pub(super) timeout: Duration,
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
        Self {
            config,
            timeout: DEFAULT_TIMEOUT,
        }
    }

    /// 设置超时时间
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// 快速检查 - 返回 GuardianDecision (带超时)
    ///
    /// `conversation` 提供当前会话的上下文消息，用于让
    /// Guardian 在评估动作时能够看到之前的对话内容（例如
    /// 检测隐藏在前序轮次中的 prompt injection）。
    ///
    /// 启用 `otel` feature 时，本方法会在入口创建名为
    /// `guardian.check` 的 span，并在内层检查返回后记录
    /// `guardian.decision` (`allow` / `deny` /
    /// `need_user_confirm`) 与 `guardian.layer` (`reviewer`)
    /// 属性。span 创建为旁路观测：不修改任何 prompt 构造、
    /// 不影响决策路径。
    pub async fn check(
        &self,
        request: &ApprovalRequest,
        conversation: &[Message],
        router: &Arc<dyn ModelRouter>,
    ) -> GuardianDecision {
        // `guardian.check` span (OTel semantic conventions for the
        // Guardian reviewer layer). The `guardian.decision` field
        // is populated AFTER the inner check returns; it MUST be
        // declared as `Empty` at the callsite because
        // `Span::record(field, value)` is a silent no-op if the
        // field was not declared in the `span!` macro (lesson from
        // Task 7). `guardian.layer` is known at compile time
        // (`"reviewer"`) so it is set inline.
        #[cfg(feature = "otel")]
        let guardian_span = tracing::span!(
            target: "synthia.guardian",
            tracing::Level::INFO,
            "guardian.check",
            guardian.decision = tracing::field::Empty,
            guardian.layer = "reviewer",
        );
        #[cfg(feature = "otel")]
        let _guardian_guard = guardian_span.enter();

        let decision = self.check_inner(request, conversation, router).await;

        #[cfg(feature = "otel")]
        {
            let decision_str = match &decision {
                GuardianDecision::Allow => "allow",
                GuardianDecision::Deny { .. } => "deny",
                GuardianDecision::NeedUserConfirm { .. } => "need_user_confirm",
            };
            guardian_span.record("guardian.decision", decision_str);
        }

        decision
    }

    /// Inner check logic — the original `check` body, factored out so
    /// the public [`GuardianReviewer::check`] can wrap it with the
    /// `guardian.check` span (created and recorded once at the
    /// outer boundary, regardless of how many early-return paths
    /// this inner function takes).
    async fn check_inner(
        &self,
        request: &ApprovalRequest,
        conversation: &[Message],
        router: &Arc<dyn ModelRouter>,
    ) -> GuardianDecision {
        if !self.config.enabled {
            return GuardianDecision::Allow;
        }

        let action_json = match request.to_json() {
            Ok(json) => serde_json::to_string_pretty(&json).unwrap_or_default(),
            Err(e) => {
                error!("Failed to serialize approval request: {}", e);
                return GuardianDecision::Deny {
                    reason: "Failed to serialize approval request".to_string(),
                };
            }
        };

        let review_prompt = build_review_prompt(
            &collect_transcript_entries(conversation),
            &action_json,
            None,
        );

        // 使用 timeout 执行 LLM 调用
        let result = timeout(
            self.timeout,
            self.call_llm_internal(&review_prompt, request, router),
        )
        .await;

        match result {
            Ok(Ok(decision)) => decision,
            Ok(Err(e)) => {
                tracing::warn!("LLM review failed: {}", e);
                GuardianDecision::Deny {
                    reason: format!("LLM review error: {}", e),
                }
            }
            Err(_) => {
                tracing::warn!("LLM review timed out after {:?}", self.timeout);
                GuardianDecision::Deny {
                    reason: "LLM review timeout - fail closed".to_string(),
                }
            }
        }
    }

    async fn call_llm_internal(
        &self,
        prompt: &str,
        request: &ApprovalRequest,
        router: &Arc<dyn ModelRouter>,
    ) -> anyhow::Result<GuardianDecision> {
        let routing_result = router.route(&[]).await?;
        let provider = &routing_result.provider;

        let params = CompletionRequest {
            model: routing_result.decision.selected_model.clone(),
            messages: Arc::new(vec![Message {
                role: synthia_provider::Role::User,
                content: synthia_provider::Content::Single(ContentPart::Text(
                    TextContent {
                        text: prompt.to_string(),
                        cache_control: None,
                    },
                )),
                tool_call_id: None,
                name: None,
                ..Default::default()
            }]),
            tools: Arc::new(vec![]),
            tool_choice: ToolChoice::None,
            temperature: Some(0.0),
            max_tokens: Some(1024),
            stop_sequences: vec![],
            extra_body: None,
            cache_policy: Some(CachePolicy::default()),
        };

        let response = provider.complete(params).await?;
        let text_content = response.content.extract_text().unwrap_or_default();
        let assessment = parse_assessment_response(&text_content)?;

        Ok(self.make_guardian_decision(assessment, request))
    }

    pub(crate) fn make_guardian_decision(
        &self,
        assessment: Assessment,
        request: &ApprovalRequest,
    ) -> GuardianDecision {
        let risk_score = assessment.risk_score;

        if risk_score < 50 {
            GuardianDecision::Allow
        } else if risk_score >= 80 {
            GuardianDecision::Deny {
                reason: assessment.rationale,
            }
        } else {
            let action_type = ActionType::Credential; // Default for reviewer
            GuardianDecision::NeedUserConfirm {
                request: Box::new(request.clone()),
                timeout: action_type.default_timeout(),
                blocking: action_type.is_blocking(),
                action_type,
            }
        }
    }

    /// 执行审查
    pub async fn review(
        &self,
        _cancel_token: tokio_util::sync::CancellationToken,
        request: ApprovalRequest,
        conversation: &[Message],
        router: &Arc<dyn ModelRouter>,
    ) -> anyhow::Result<ReviewDecision> {
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

        let params = CompletionRequest {
            model: routing_result.decision.selected_model.clone(),
            messages: Arc::new(vec![Message {
                role: synthia_provider::Role::User,
                content: synthia_provider::Content::Single(ContentPart::Text(
                    TextContent {
                        text: review_prompt,
                        cache_control: None,
                    },
                )),
                tool_call_id: None,
                name: None,
                ..Default::default()
            }]),
            tools: Arc::new(vec![]),
            tool_choice: ToolChoice::None,
            temperature: Some(0.0),
            max_tokens: Some(1024),
            stop_sequences: vec![],
            extra_body: None,
            cache_policy: Some(CachePolicy::default()),
        };
        let response = provider.complete(params).await?;
        let text_content = response.content.extract_text().unwrap_or_default();

        let decision = parse_assessment_response(&text_content)?;
        Ok(self.make_decision(decision))
    }

    /// 根据评估结果作出决策
    pub(super) fn make_decision(
        &self,
        assessment: Assessment,
    ) -> ReviewDecision {
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
                    crate::GuardianOption {
                        id: "yes".to_string(),
                        label: "Yes".to_string(),
                        description: "Proceed with this action".to_string(),
                    },
                    crate::GuardianOption {
                        id: "no".to_string(),
                        label: "No".to_string(),
                        description: "Cancel this action".to_string(),
                    },
                    crate::GuardianOption {
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
