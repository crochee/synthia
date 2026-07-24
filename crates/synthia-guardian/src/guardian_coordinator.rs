//! Guardian Coordinator - Hybrid layer combining SimpleGuardian + CircuitBreaker
//!
//! This coordinator implements the hybrid Guardian layer with:
//! - Fast-path: SimpleGuardian rule-based check first
//! - Circuit breaker: Track denials per session
//! - Escalation path: For medium-risk, escalate to
//!   [`GuardianSubagentReviewer`] when a subagent factory is available
//!   (P4 渐进降级 — falls back to `NeedUserConfirm` on subagent failure).

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use synthia_provider::Message;
use tokio_util::sync::CancellationToken;
use tracing::warn;

use crate::{
    ApprovalRequest,
    Guardian,
    GuardianCircuitBreaker,
    GuardianConfig,
    GuardianDecision,
    GuardianSubagentError,
    GuardianSubagentFactory,
    GuardianSubagentReviewer,
    ReviewDecision,
    SimpleGuardian,
};

/// Outcome of a [`GuardianCoordinator::check`] call.
///
/// Carries the [`GuardianDecision`] plus escalation metadata so the
/// agent loop (Task 5) can emit the appropriate `AgentEvent` types
/// (`GuardianConfirmationRequest` / `GuardianWarning`) without
/// `synthia-guardian` needing to import `synthia-agent` (circular
/// dependency avoidance).
#[derive(Debug)]
pub struct GuardianCheckOutcome {
    /// The final Guardian decision.
    pub decision: GuardianDecision,
    /// `true` if the request was escalated to a Guardian subagent
    /// (regardless of whether the subagent succeeded or fell back).
    pub escalated: bool,
    /// `Some` if the subagent review failed and the coordinator
    /// applied the fallback (`NeedUserConfirm`). `None` if no
    /// subagent was invoked or the subagent succeeded.
    pub subagent_error: Option<GuardianSubagentError>,
}

/// Guardian Coordinator - orchestrates the hybrid Guardian layer
///
/// Combines:
/// - [`SimpleGuardian`] for rule-based fast-path
/// - [`GuardianCircuitBreaker`] for denial tracking (interior
///   mutability via `Mutex` so [`GuardianCoordinator::check`] can
///   take `&self`)
/// - Optional [`GuardianSubagentReviewer`] for LLM-based escalation
///   on medium-risk requests
pub struct GuardianCoordinator {
    simple_guardian: SimpleGuardian,
    circuit_breaker: Mutex<GuardianCircuitBreaker>,
    /// `Some` when `subagent_enabled: true` and a factory was
    /// provided at construction. `None` otherwise (legacy path).
    reviewer: Option<GuardianSubagentReviewer>,
    user_id: String,
    parent_session_id: String,
}

impl std::fmt::Debug for GuardianCoordinator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GuardianCoordinator")
            .field("simple_guardian", &self.simple_guardian)
            .field("circuit_breaker", &self.circuit_breaker)
            .field("has_reviewer", &self.reviewer.is_some())
            .field("user_id", &self.user_id)
            .field("parent_session_id", &self.parent_session_id)
            .finish()
    }
}

impl GuardianCoordinator {
    /// Create a new `GuardianCoordinator` without subagent escalation
    /// (legacy path: medium-risk → `NeedUserConfirm`).
    ///
    /// `user_id` and `parent_session_id` are stored for potential
    /// subagent use but no reviewer is created.
    pub fn new(
        config: GuardianConfig,
        user_id: String,
        parent_session_id: String,
    ) -> Self {
        Self {
            simple_guardian: SimpleGuardian::new(config),
            circuit_breaker: Mutex::new(GuardianCircuitBreaker::new()),
            reviewer: None,
            user_id,
            parent_session_id,
        }
    }

    /// Create a new `GuardianCoordinator` with subagent escalation
    /// enabled.
    ///
    /// When `config.subagent_enabled` is `true`, a
    /// [`GuardianSubagentReviewer`] is created from the provided
    /// `factory`. When `false`, the reviewer is `None` (legacy path)
    /// and the factory is unused.
    pub fn with_subagent_factory(
        config: GuardianConfig,
        user_id: String,
        parent_session_id: String,
        factory: Arc<dyn GuardianSubagentFactory>,
    ) -> Self {
        let reviewer = if config.subagent_enabled {
            Some(GuardianSubagentReviewer::new(config.clone(), factory))
        } else {
            None
        };
        Self {
            simple_guardian: SimpleGuardian::new(config),
            circuit_breaker: Mutex::new(GuardianCircuitBreaker::new()),
            reviewer,
            user_id,
            parent_session_id,
        }
    }

    /// Get a snapshot of the circuit breaker for state queries.
    ///
    /// Returns a clone to avoid holding the `Mutex` lock across an
    /// `.await` boundary in callers.
    pub fn circuit_breaker(&self) -> GuardianCircuitBreaker {
        self.circuit_breaker
            .lock()
            .expect("circuit breaker mutex poisoned")
            .clone()
    }

    /// Hybrid Guardian check with risk-tier dispatch.
    ///
    /// # Risk tiers
    /// - risk < 50 → `Allow` (fast-path via [`SimpleGuardian`])
    /// - risk >= 80 → `Deny` (fast-path via [`SimpleGuardian`])
    /// - risk in [50, 80):
    ///   - If `subagent_factory` is `Some` AND the coordinator has a
    ///     [`GuardianSubagentReviewer`], escalate to subagent review.
    ///   - Otherwise, `NeedUserConfirm` (legacy path).
    ///
    /// # Fallback (P4 渐进降级)
    /// On subagent error/timeout/cancellation, falls back to the
    /// `NeedUserConfirm` decision from the fast-path. The error is
    /// captured in [`GuardianCheckOutcome::subagent_error`].
    ///
    /// # Circuit breaker
    /// `Allow` → `record_approval`; `Deny`/`NeedUserConfirm` →
    /// `record_denial`. If the circuit breaker has tripped
    /// (`should_interrupt`), returns `Deny` immediately.
    pub async fn check(
        &self,
        request: &ApprovalRequest,
        conversation: &[Message],
        cancel_token: &CancellationToken,
        subagent_factory: Option<&dyn GuardianSubagentFactory>,
    ) -> GuardianCheckOutcome {
        // Circuit breaker fast-fail: if session is interrupted, deny all.
        if self
            .circuit_breaker
            .lock()
            .expect("circuit breaker mutex poisoned")
            .should_interrupt()
        {
            let decision = GuardianDecision::Deny {
                reason: "Session interrupt - too many denials".to_string(),
            };
            return GuardianCheckOutcome {
                decision,
                escalated: false,
                subagent_error: None,
            };
        }

        // Fast-path: SimpleGuardian handles disabled / low / high /
        // medium-legacy. For medium-risk it returns NeedUserConfirm.
        let fast_decision = self.simple_guardian.check(request).await;

        // Intercept medium-risk NeedUserConfirm for subagent escalation.
        // The `subagent_factory` parameter gates escalation (per spec:
        // None → legacy path). The coordinator's internal reviewer
        // provides the actual factory + timeout + parsing logic.
        let (decision, escalated, subagent_error) = if matches!(
            fast_decision,
            GuardianDecision::NeedUserConfirm { .. }
        ) && subagent_factory
            .is_some()
        {
            if let Some(reviewer) = self.reviewer.as_ref() {
                match reviewer
                    .review(
                        request,
                        conversation,
                        &self.user_id,
                        &self.parent_session_id,
                        cancel_token,
                    )
                    .await
                {
                    Ok(subagent_decision) => (subagent_decision, true, None),
                    Err(e) => {
                        // P4 渐进降级: subagent failed → fall back to
                        // the fast-path NeedUserConfirm decision.
                        warn!(
                            error = %e,
                            "guardian subagent review failed, falling back to NeedUserConfirm"
                        );
                        (fast_decision.clone(), true, Some(e))
                    }
                }
            } else {
                (fast_decision, false, None)
            }
        } else {
            (fast_decision, false, None)
        };

        // Wire circuit breaker based on the final decision.
        {
            let mut cb = self
                .circuit_breaker
                .lock()
                .expect("circuit breaker mutex poisoned");
            match &decision {
                GuardianDecision::Allow => cb.record_approval(),
                GuardianDecision::Deny { .. } => cb.record_denial(),
                GuardianDecision::NeedUserConfirm { .. } => {
                    // Medium risk - record as denial for circuit breaker
                    // purposes but still need user confirmation.
                    cb.record_denial();
                }
            }
        }

        GuardianCheckOutcome {
            decision,
            escalated,
            subagent_error,
        }
    }
}

#[async_trait]
impl Guardian for GuardianCoordinator {
    async fn review(
        &self,
        cancel_token: &CancellationToken,
        request: ApprovalRequest,
    ) -> anyhow::Result<Option<ReviewDecision>> {
        self.simple_guardian.review(cancel_token, request).await
    }

    fn is_dangerous_tool(&self, tool_name: &str) -> bool {
        self.simple_guardian.is_dangerous_tool(tool_name)
    }

    /// Hybrid dispatch: delegates to the inherent
    /// [`GuardianCoordinator::check`] and discards the escalation
    /// metadata, returning just the [`GuardianDecision`].
    async fn check(
        &self,
        request: &ApprovalRequest,
        conversation: &[Message],
        cancel_token: CancellationToken,
        subagent_factory: Option<&dyn GuardianSubagentFactory>,
    ) -> GuardianDecision {
        self.check(request, conversation, &cancel_token, subagent_factory)
            .await
            .decision
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::subagent_reviewer::GuardianSubagentOutput;

    // ---- Mock factory for subagent escalation tests ----

    #[derive(Clone)]
    enum MockBehavior {
        Ok(GuardianSubagentOutput),
        Delay(Duration, GuardianSubagentOutput),
        Fail(String),
    }

    struct MockFactory {
        behavior: Mutex<MockBehavior>,
    }

    impl MockFactory {
        fn ok(output: GuardianSubagentOutput) -> Self {
            Self {
                behavior: Mutex::new(MockBehavior::Ok(output)),
            }
        }

        fn delay(delay: Duration, output: GuardianSubagentOutput) -> Self {
            Self {
                behavior: Mutex::new(MockBehavior::Delay(delay, output)),
            }
        }

        fn fail(msg: impl Into<String>) -> Self {
            Self {
                behavior: Mutex::new(MockBehavior::Fail(msg.into())),
            }
        }
    }

    #[async_trait]
    impl GuardianSubagentFactory for MockFactory {
        async fn run_child(
            &self,
            _user_id: String,
            _parent_session_id: String,
            _prompt: String,
        ) -> Result<GuardianSubagentOutput, crate::GuardianSubagentSpawnError>
        {
            let behavior = self.behavior.lock().unwrap().clone();
            match behavior {
                MockBehavior::Ok(output) => Ok(output),
                MockBehavior::Delay(d, output) => {
                    tokio::time::sleep(d).await;
                    Ok(output)
                }
                MockBehavior::Fail(msg) => {
                    Err(crate::GuardianSubagentSpawnError::SpawnFailed(msg))
                }
            }
        }
    }

    fn allow_output() -> GuardianSubagentOutput {
        GuardianSubagentOutput {
            output: r#"{"risk_level":"low","risk_score":30,"rationale":"safe action","evidence":[]}"#
                .to_string(),
            success: true,
        }
    }

    fn deny_output() -> GuardianSubagentOutput {
        GuardianSubagentOutput {
            output: r#"{"risk_level":"high","risk_score":85,"rationale":"destructive","evidence":[]}"#
                .to_string(),
            success: true,
        }
    }

    // ---- Helpers ----

    fn make_legacy_coordinator() -> GuardianCoordinator {
        let config = GuardianConfig::default().enabled(true);
        GuardianCoordinator::new(
            config,
            "test-user".to_string(),
            "test-session".to_string(),
        )
    }

    fn make_subagent_coordinator(
        factory: Arc<dyn GuardianSubagentFactory>,
        timeout: Duration,
    ) -> GuardianCoordinator {
        let config = GuardianConfig::default()
            .enabled(true)
            .with_subagent_enabled(true)
            .with_timeout(timeout);
        GuardianCoordinator::with_subagent_factory(
            config,
            "test-user".to_string(),
            "test-session".to_string(),
            factory,
        )
    }

    // ---- Existing tests (updated to new signature) ----

    #[tokio::test]
    async fn test_coordinator_allow_low_risk() {
        let coordinator = make_legacy_coordinator();

        let request =
            ApprovalRequest::shell("id", vec!["ls".to_string()], "/", None);
        let outcome = coordinator
            .check(&request, &[], &CancellationToken::new(), None)
            .await;

        assert!(outcome.decision.is_allowed());
        assert!(!outcome.escalated);
    }

    #[tokio::test]
    async fn test_coordinator_deny_high_risk() {
        let coordinator = make_legacy_coordinator();

        let request = ApprovalRequest::shell(
            "id",
            vec!["rm -rf /".to_string()],
            "/",
            None,
        );
        let outcome = coordinator
            .check(&request, &[], &CancellationToken::new(), None)
            .await;

        assert!(!outcome.decision.is_allowed());
        assert!(matches!(outcome.decision, GuardianDecision::Deny { .. }));
        assert!(!outcome.escalated);
    }

    #[tokio::test]
    async fn test_coordinator_need_user_confirm_medium_risk() {
        let coordinator = make_legacy_coordinator();

        let request = ApprovalRequest::network_access(
            "id", "target", "host", "https", 443,
        );
        let outcome = coordinator
            .check(&request, &[], &CancellationToken::new(), None)
            .await;

        assert!(!outcome.decision.is_allowed());
        assert!(matches!(
            outcome.decision,
            GuardianDecision::NeedUserConfirm { .. }
        ));
        assert!(!outcome.escalated);
    }

    #[tokio::test]
    async fn test_coordinator_circuit_breaker_tracks_denials() {
        let coordinator = make_legacy_coordinator();

        // First denial
        let request1 = ApprovalRequest::shell(
            "id",
            vec!["rm -rf /".to_string()],
            "/",
            None,
        );
        coordinator
            .check(&request1, &[], &CancellationToken::new(), None)
            .await;
        assert!(!coordinator.circuit_breaker().should_interrupt());

        // Second denial
        let request2 = ApprovalRequest::shell(
            "id",
            vec!["sudo rm -rf /".to_string()],
            "/",
            None,
        );
        coordinator
            .check(&request2, &[], &CancellationToken::new(), None)
            .await;
        assert!(!coordinator.circuit_breaker().should_interrupt());

        // Third denial - should trigger interrupt
        let request3 = ApprovalRequest::shell(
            "id",
            vec!["chmod 777 /".to_string()],
            "/",
            None,
        );
        coordinator
            .check(&request3, &[], &CancellationToken::new(), None)
            .await;
        assert!(coordinator.circuit_breaker().should_interrupt());
    }

    #[tokio::test]
    async fn test_coordinator_approval_resets_consecutive() {
        let coordinator = make_legacy_coordinator();

        // Two denials
        let request1 = ApprovalRequest::shell(
            "id",
            vec!["rm -rf /".to_string()],
            "/",
            None,
        );
        coordinator
            .check(&request1, &[], &CancellationToken::new(), None)
            .await;
        let request2 = ApprovalRequest::shell(
            "id",
            vec!["sudo rm -rf /".to_string()],
            "/",
            None,
        );
        coordinator
            .check(&request2, &[], &CancellationToken::new(), None)
            .await;

        // One approval
        let request3 =
            ApprovalRequest::shell("id", vec!["ls".to_string()], "/", None);
        coordinator
            .check(&request3, &[], &CancellationToken::new(), None)
            .await;

        // Consecutive should be reset
        assert_eq!(coordinator.circuit_breaker().consecutive_denials(), 0);
    }

    // ---- New tests: subagent escalation paths ----

    #[tokio::test]
    async fn test_medium_risk_escalation_to_subagent_returns_allow() {
        // Mock factory returns a low-risk assessment → Allow.
        let factory = Arc::new(MockFactory::ok(allow_output()));
        let coordinator =
            make_subagent_coordinator(factory, Duration::from_secs(5));

        // Network access → medium risk (65) → escalate to subagent.
        let request = ApprovalRequest::network_access(
            "id", "target", "host", "https", 443,
        );
        let outcome = coordinator
            .check(
                &request,
                &[],
                &CancellationToken::new(),
                Some(&MockFactory::ok(allow_output())),
            )
            .await;

        assert!(outcome.escalated, "should have escalated to subagent");
        assert!(
            outcome.decision.is_allowed(),
            "subagent returned low-risk → Allow"
        );
        assert!(outcome.subagent_error.is_none(), "no error expected");
    }

    #[tokio::test]
    async fn test_subagent_failure_fallback_to_need_user_confirm() {
        // Mock factory fails → fallback to NeedUserConfirm.
        let factory = Arc::new(MockFactory::fail("connection refused"));
        let coordinator =
            make_subagent_coordinator(factory, Duration::from_secs(5));

        let request = ApprovalRequest::network_access(
            "id", "target", "host", "https", 443,
        );
        let outcome = coordinator
            .check(
                &request,
                &[],
                &CancellationToken::new(),
                Some(&MockFactory::fail("dummy")),
            )
            .await;

        assert!(outcome.escalated, "should have attempted escalation");
        assert!(matches!(
            outcome.decision,
            GuardianDecision::NeedUserConfirm { .. }
        ));
        assert!(
            outcome.subagent_error.is_some(),
            "subagent error should be captured"
        );
        assert!(
            matches!(
                outcome.subagent_error,
                Some(GuardianSubagentError::SpawnFailed(_))
            ),
            "expected SpawnFailed error"
        );
    }

    #[tokio::test]
    async fn test_subagent_timeout_fallback_to_need_user_confirm() {
        // Mock factory delays 2s; coordinator timeout is 100ms.
        let factory = Arc::new(MockFactory::delay(
            Duration::from_secs(2),
            allow_output(),
        ));
        let coordinator =
            make_subagent_coordinator(factory, Duration::from_millis(100));

        let request = ApprovalRequest::network_access(
            "id", "target", "host", "https", 443,
        );
        let outcome = coordinator
            .check(
                &request,
                &[],
                &CancellationToken::new(),
                Some(&MockFactory::delay(
                    Duration::from_secs(2),
                    allow_output(),
                )),
            )
            .await;

        assert!(outcome.escalated, "should have attempted escalation");
        assert!(matches!(
            outcome.decision,
            GuardianDecision::NeedUserConfirm { .. }
        ));
        assert!(
            matches!(
                outcome.subagent_error,
                Some(GuardianSubagentError::Timeout)
            ),
            "expected Timeout error"
        );
    }

    #[tokio::test]
    async fn test_subagent_cancellation_fallback_to_need_user_confirm() {
        // Mock factory delays 10s; we cancel after 50ms.
        let factory = Arc::new(MockFactory::delay(
            Duration::from_secs(10),
            allow_output(),
        ));
        let coordinator =
            make_subagent_coordinator(factory, Duration::from_secs(5));

        let request = ApprovalRequest::network_access(
            "id", "target", "host", "https", 443,
        );
        let cancel = CancellationToken::new();
        let cancel_clone = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            cancel_clone.cancel();
        });

        let outcome = coordinator
            .check(
                &request,
                &[],
                &cancel,
                Some(&MockFactory::delay(
                    Duration::from_secs(10),
                    allow_output(),
                )),
            )
            .await;

        assert!(outcome.escalated, "should have attempted escalation");
        assert!(matches!(
            outcome.decision,
            GuardianDecision::NeedUserConfirm { .. }
        ));
        assert!(
            matches!(
                outcome.subagent_error,
                Some(GuardianSubagentError::Cancelled)
            ),
            "expected Cancelled error"
        );
    }

    #[tokio::test]
    async fn test_subagent_disabled_legacy_path_returns_need_user_confirm() {
        // Coordinator without subagent_enabled → reviewer is None.
        // Even if subagent_factory is Some, medium-risk → NeedUserConfirm.
        let coordinator = make_legacy_coordinator();

        let request = ApprovalRequest::network_access(
            "id", "target", "host", "https", 443,
        );
        let dummy_factory = MockFactory::ok(allow_output());
        let outcome = coordinator
            .check(
                &request,
                &[],
                &CancellationToken::new(),
                Some(&dummy_factory),
            )
            .await;

        assert!(!outcome.escalated, "subagent disabled → no escalation");
        assert!(matches!(
            outcome.decision,
            GuardianDecision::NeedUserConfirm { .. }
        ));
        assert!(outcome.subagent_error.is_none());
    }

    #[tokio::test]
    async fn test_subagent_factory_none_forces_legacy_path() {
        // Coordinator WITH subagent enabled, but subagent_factory
        // parameter is None → legacy path (per spec).
        let factory = Arc::new(MockFactory::ok(allow_output()));
        let coordinator =
            make_subagent_coordinator(factory, Duration::from_secs(5));

        let request = ApprovalRequest::network_access(
            "id", "target", "host", "https", 443,
        );
        let outcome = coordinator
            .check(&request, &[], &CancellationToken::new(), None)
            .await;

        assert!(!outcome.escalated, "subagent_factory None → no escalation");
        assert!(matches!(
            outcome.decision,
            GuardianDecision::NeedUserConfirm { .. }
        ));
        assert!(outcome.subagent_error.is_none());
    }

    #[tokio::test]
    async fn test_subagent_returns_deny_for_high_risk_assessment() {
        // Subagent returns a high-risk assessment → Deny.
        let factory = Arc::new(MockFactory::ok(deny_output()));
        let coordinator =
            make_subagent_coordinator(factory, Duration::from_secs(5));

        let request = ApprovalRequest::network_access(
            "id", "target", "host", "https", 443,
        );
        let outcome = coordinator
            .check(
                &request,
                &[],
                &CancellationToken::new(),
                Some(&MockFactory::ok(deny_output())),
            )
            .await;

        assert!(outcome.escalated, "should have escalated to subagent");
        assert!(matches!(outcome.decision, GuardianDecision::Deny { .. }));
        assert!(outcome.subagent_error.is_none());
    }

    #[tokio::test]
    async fn test_trait_check_returns_decision_only() {
        // Verify the Guardian trait impl returns just the decision.
        let coordinator = make_legacy_coordinator();

        let request =
            ApprovalRequest::shell("id", vec!["ls".to_string()], "/", None);
        let decision = <GuardianCoordinator as Guardian>::check(
            &coordinator,
            &request,
            &[],
            CancellationToken::new(),
            None,
        )
        .await;

        assert!(decision.is_allowed());
    }
}
