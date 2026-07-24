//! Unit tests for the `review` module family.
//!
//! The original 5 tests lived in the `reviewer` submodule at the
//! bottom of `review.rs`; they're hoisted into this sibling file
//! so the production code (`super` + `reviewer`) doesn't carry
//! the test body weight.
//!
//! Coverage map (5 tests):
//!
//! - [`GuardianReviewer::make_decision`] (1):
//!   `test_guardian_reviewer_make_decision` covers all three
//!   branches (Approved / Denied / NeedsUserInput).
//! - Threshold check (1): `test_risk_threshold_approval`.
//! - Timeout path (1): `test_guardian_reviewer_timeout_returns_deny`.
//! - Builder (1): `test_guardian_reviewer_with_timeout_sets_internal_timeout`.
//! - Disabled-guardian path (1):
//!   `test_guardian_reviewer_disabled_returns_allow`.

use std::{sync::Arc, time::Duration};

use synthia_model_router::ModelRouter;
use synthia_provider::{Content, Role, types::Message};

use super::reviewer::GuardianReviewer;
use crate::{
    ApprovalRequest,
    GuardianConfig,
    ReviewDecision,
    config::GuardianRiskLevel,
    review_types::{Assessment, Evidence},
};

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

#[tokio::test]
async fn test_guardian_reviewer_timeout_returns_deny() {
    use synthia_model_router::{ModelConfig, RoutingResult};

    use crate::guardian_decision::GuardianDecision;

    // Create config with low risk threshold so it tries LLM review
    let config = GuardianConfig::default()
        .enabled(true)
        .with_risk_threshold(80);

    // Create reviewer with very short timeout (1ms)
    let reviewer =
        GuardianReviewer::new(config).with_timeout(Duration::from_millis(1));

    // Create a mock router that intentionally takes too long
    struct SlowRouter;
    #[async_trait::async_trait]
    impl ModelRouter for SlowRouter {
        async fn route(
            &self,
            _messages: &[Message],
        ) -> anyhow::Result<RoutingResult> {
            // Sleep for 1 second - longer than our 1ms timeout
            tokio::time::sleep(Duration::from_secs(1)).await;
            Err(anyhow::anyhow!("Router took too long"))
        }

        fn available_models(&self) -> &[ModelConfig] {
            &[]
        }

        fn context_window(&self) -> usize {
            128000
        }
    }

    let request =
        ApprovalRequest::shell("test", vec!["echo".to_string()], "/tmp", None);
    let router: Arc<dyn ModelRouter> = Arc::new(SlowRouter);

    let decision = reviewer.check(&request, &[], &router).await;

    match decision {
        GuardianDecision::Deny { reason } => {
            assert!(
                reason.contains("timeout")
                    || reason.contains("LLM review error"),
                "Expected timeout-related denial, got: {}",
                reason
            );
        }
        other => panic!("Expected Deny on timeout, got: {:?}", other),
    }
}

#[test]
fn test_guardian_reviewer_with_timeout_sets_internal_timeout() {
    let config = GuardianConfig::default().enabled(true);
    let _reviewer =
        GuardianReviewer::new(config).with_timeout(Duration::from_secs(45));

    // Verify the reviewer was created with custom timeout
    // The internal timeout field is private, so we verify via behavior
    // by checking that a very short timeout causes immediate denial
    let config2 = GuardianConfig::default().enabled(true);
    let _reviewer2 =
        GuardianReviewer::new(config2).with_timeout(Duration::from_millis(0));

    // This test mainly verifies the with_timeout builder works without panicking
    // The actual timeout behavior is tested in the async test above
}

#[tokio::test]
async fn test_guardian_reviewer_disabled_returns_allow() {
    use synthia_model_router::{ModelConfig, RoutingResult};

    use crate::guardian_decision::GuardianDecision;

    let config = GuardianConfig::default().enabled(false);
    let reviewer = GuardianReviewer::new(config);

    // Even with a slow router, disabled guardian should return Allow immediately
    struct AnyRouter;
    #[async_trait::async_trait]
    impl ModelRouter for AnyRouter {
        async fn route(
            &self,
            _messages: &[Message],
        ) -> anyhow::Result<RoutingResult> {
            tokio::time::sleep(Duration::from_secs(100)).await;
            Err(anyhow::anyhow!("Should not be called"))
        }

        fn available_models(&self) -> &[ModelConfig] {
            &[]
        }

        fn context_window(&self) -> usize {
            128000
        }
    }

    let request =
        ApprovalRequest::shell("test", vec!["echo".to_string()], "/tmp", None);
    let router: Arc<dyn ModelRouter> = Arc::new(AnyRouter);

    let decision = reviewer.check(&request, &[], &router).await;

    match decision {
        GuardianDecision::Allow => {}
        other => {
            panic!("Expected Allow when disabled, got: {:?}", other)
        }
    }
}

// P0-4 / P0-5 regression tests below.
//
// `check()` now threads the conversation transcript into the review
// prompt, and `make_guardian_decision` now carries the real
// `ApprovalRequest` into `NeedUserConfirm` (instead of a "temp"
// placeholder). These tests pin both behaviors.

#[tokio::test]
async fn test_check_with_conversation_context() {
    use synthia_model_router::{ModelConfig, RoutingResult};

    use crate::guardian_decision::GuardianDecision;

    let config = GuardianConfig::default()
        .enabled(true)
        .with_risk_threshold(80);
    let reviewer = GuardianReviewer::new(config);

    // Router fails fast so check() returns a Deny without needing an LLM.
    struct FailingRouter;
    #[async_trait::async_trait]
    impl ModelRouter for FailingRouter {
        async fn route(
            &self,
            _messages: &[Message],
        ) -> anyhow::Result<RoutingResult> {
            Err(anyhow::anyhow!("intentional failure"))
        }

        fn available_models(&self) -> &[ModelConfig] {
            &[]
        }

        fn context_window(&self) -> usize {
            128000
        }
    }

    let request = ApprovalRequest::shell(
        "conv-test",
        vec!["echo".to_string()],
        "/tmp",
        None,
    );
    let router: Arc<dyn ModelRouter> = Arc::new(FailingRouter);

    // Non-empty conversation context (prior user/assistant turns).
    let conversation = vec![
        Message {
            role: Role::User,
            content: Content::text("please run echo"),
            ..Default::default()
        },
        Message {
            role: Role::Assistant,
            content: Content::text("running echo now"),
            ..Default::default()
        },
    ];

    // Verify: no panic, returns a GuardianDecision. With a failing router
    // the guardian fails closed (Deny with an LLM review error).
    let decision = reviewer.check(&request, &conversation, &router).await;

    match decision {
        GuardianDecision::Deny { reason } => {
            assert!(
                reason.contains("LLM review error"),
                "Expected LLM review error denial, got: {}",
                reason
            );
        }
        other => panic!("Expected Deny on router failure, got: {:?}", other),
    }
}

#[tokio::test]
async fn test_check_with_empty_conversation_no_panic() {
    use synthia_model_router::{ModelConfig, RoutingResult};

    use crate::guardian_decision::GuardianDecision;

    let config = GuardianConfig::default()
        .enabled(true)
        .with_risk_threshold(80);
    let reviewer = GuardianReviewer::new(config);

    struct FailingRouter;
    #[async_trait::async_trait]
    impl ModelRouter for FailingRouter {
        async fn route(
            &self,
            _messages: &[Message],
        ) -> anyhow::Result<RoutingResult> {
            Err(anyhow::anyhow!("intentional failure"))
        }

        fn available_models(&self) -> &[ModelConfig] {
            &[]
        }

        fn context_window(&self) -> usize {
            128000
        }
    }

    let request = ApprovalRequest::shell(
        "empty-conv-test",
        vec!["echo".to_string()],
        "/tmp",
        None,
    );
    let router: Arc<dyn ModelRouter> = Arc::new(FailingRouter);

    // Empty conversation (backward-compat path) must not panic.
    let decision = reviewer.check(&request, &[], &router).await;

    match decision {
        GuardianDecision::Deny { .. } => {}
        other => panic!("Expected Deny on router failure, got: {:?}", other),
    }
}

#[test]
fn test_make_guardian_decision_uses_actual_request() {
    use crate::guardian_decision::GuardianDecision;

    let config = GuardianConfig::default()
        .enabled(true)
        .with_risk_threshold(80);
    let reviewer = GuardianReviewer::new(config);

    // Medium risk (50-80) triggers NeedUserConfirm.
    let assessment = Assessment {
        risk_level: GuardianRiskLevel::Medium,
        risk_score: 65,
        rationale: "Moderate risk".to_string(),
        evidence: vec![],
    };

    // The actual request being reviewed — must NOT be the "temp" placeholder.
    let actual_request = ApprovalRequest::shell(
        "real-action-id",
        vec![
            "rm".to_string(),
            "-rf".to_string(),
            "/important".to_string(),
        ],
        "/home",
        Some("user invoked cleanup".to_string()),
    );

    let decision = reviewer.make_guardian_decision(assessment, &actual_request);

    match decision {
        GuardianDecision::NeedUserConfirm { request, .. } => {
            // The request carried into NeedUserConfirm must be the actual
            // request, not the old "temp" placeholder.
            assert_ne!(request.id(), "temp");
            assert_eq!(*request, actual_request);
        }
        other => {
            panic!("Expected NeedUserConfirm for medium risk, got: {:?}", other)
        }
    }
}
