//! `GuardianSubagentReviewer` — runs the Guardian review inside an
//! isolated subagent session.
//!
//! This is the subagent-backed review path (design D1): instead of
//! calling the LLM inline (like [`GuardianReviewer`]), the reviewer
//! spawns a child session via a [`GuardianSubagentFactory`], sends the
//! review prompt, and parses the child's output into a
//! [`GuardianDecision`].
//!
//! # Why a local factory trait (not `synthia_agent::SubagentSessionFactory`)
//!
//! `synthia-agent` already depends on `synthia-guardian` (for
//! `LoopDetectorSet` etc.). Importing `synthia_agent::SubagentSessionFactory`
//! here would create a circular dependency. Instead,
//! [`GuardianSubagentFactory`] mirrors the `run_child` subset of that
//! trait. The concrete implementation — provided by `synthia-agent` /
//! `synthia-server` in a later task — wraps the real
//! `SubagentSessionFactory` and converts `AgentResult` into
//! [`GuardianSubagentOutput`].
//!
//! # Factory responsibilities (design D2 + D8)
//!
//! The factory implementation MUST apply the following when spawning a
//! Guardian subagent. These cannot be enforced through the
//! `run_child` interface (which is opaque by design — D1 reuses the
//! existing subagent framework), so they are contractual obligations
//! documented here and verified by integration tests in a later task:
//!
//! 1. **Three-layer lockdown** (D2, P6 不信任 LLM):
//!    - *Runtime layer*: `guardian_enabled: false` + `max_iterations: 1`
//!    - *Registry layer*: empty tool registry (no tools the subagent
//!      could call)
//!    - *Permission layer*: Deny-only inheritance via
//!      `derive_subagent_permission`
//!
//!    Three independent layers ensure that if one fails the others still
//!    prevent recursion (a Guardian spawning a Guardian).
//!
//! 2. **Prompt-cache key** `guardian:{parent_session_id}` (D8): the
//!    factory injects this cache key so that repeated Guardian reviews
//!    within the same parent session share a KV-cache prefix (P1 前缀
//!    一致性). The `parent_session_id` (which already carries the
//!    `user_id` namespace) is passed to `run_child` for this purpose.
//!
//! 3. **System prompt** = [`GUARDIAN_POLICY_PROMPT`] (D5): the factory
//!    sets the guardian policy as the subagent's system prompt. The
//!    `prompt` passed to [`GuardianSubagentFactory::run_child`] is the
//!    user message (the review prompt). To be robust against factories
//!    that do not set a separate system prompt, the reviewer also
//!    prepends the policy to the user message — redundancy is harmless.

use std::sync::Arc;

use async_trait::async_trait;
use synthia_provider::Message;
use thiserror::Error;
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};

use crate::{
    ApprovalRequest,
    GUARDIAN_POLICY_PROMPT,
    GuardianConfig,
    GuardianDecision,
    build_review_prompt,
    collect_transcript_entries,
    parse_assessment_response,
    review::reviewer::GuardianReviewer,
};

/// Output returned by a Guardian subagent run.
///
/// Mirrors the subset of `synthia_agent::AgentResult` that the Guardian
/// reviewer consumes: the final output text (the `Finish` event payload)
/// and whether the run completed successfully.
#[derive(Debug, Clone)]
pub struct GuardianSubagentOutput {
    /// The subagent's final output text (expected to be a JSON
    /// assessment matching the schema in [`GUARDIAN_POLICY_PROMPT`]).
    pub output: String,
    /// `true` if the subagent completed normally (status == Completed).
    /// `false` if it errored or was cancelled.
    pub success: bool,
}

/// Errors that can occur when spawning / running a Guardian subagent
/// via [`GuardianSubagentFactory::run_child`].
#[derive(Debug, Error)]
pub enum GuardianSubagentSpawnError {
    /// The factory failed to create or run the child session.
    #[error("guardian subagent spawn failed: {0}")]
    SpawnFailed(String),
    /// The parent session was not found.
    #[error("parent session not found: {0}")]
    ParentNotFound(String),
}

/// Factory for spawning Guardian subagents.
///
/// This is a guardian-local abstraction over
/// `synthia_agent::SubagentSessionFactory::run_child`. See the
/// [module docs](self) for why a local trait is needed (circular
/// dependency avoidance) and for the factory's contractual
/// responsibilities (three-layer lockdown, cache key, system prompt).
#[async_trait]
pub trait GuardianSubagentFactory: Send + Sync {
    /// Spawn a Guardian subagent, send `prompt` as the user message,
    /// and await the final result.
    ///
    /// `parent_session_id` is used by the factory to derive the
    /// prompt-cache key `guardian:{parent_session_id}` (D8) and to
    /// establish the parent→child fork relationship.
    async fn run_child(
        &self,
        user_id: String,
        parent_session_id: String,
        prompt: String,
    ) -> Result<GuardianSubagentOutput, GuardianSubagentSpawnError>;
}

/// Errors returned by [`GuardianSubagentReviewer::review`].
///
/// On any error, the caller ([`crate::GuardianCoordinator`]) falls back
/// to `SimpleGuardian::NeedUserConfirm` (P4 渐进降级, design D3).
#[derive(Debug, Error)]
pub enum GuardianSubagentError {
    /// The factory failed to spawn or run the subagent.
    #[error("guardian subagent spawn failed: {0}")]
    SpawnFailed(String),
    /// The subagent did not complete within `GuardianConfig::timeout`.
    #[error("guardian subagent timed out")]
    Timeout,
    /// The operation was cancelled via the `CancellationToken`.
    #[error("guardian subagent cancelled")]
    Cancelled,
    /// The subagent's output could not be parsed as a JSON assessment.
    #[error("guardian subagent parse failed: {0}")]
    ParseFailed(String),
    /// The subagent session ended abnormally (errored / cancelled).
    #[error("guardian subagent session ended abnormally: {0}")]
    SessionEnded(String),
}

/// Guardian reviewer that runs the LLM assessment inside an isolated
/// subagent session.
///
/// Holds a [`GuardianReviewer`] (reused for
/// [`make_guardian_decision`](GuardianReviewer::make_guardian_decision)
/// — the decision-mapping logic that turns an `Assessment` into a
/// `GuardianDecision`), the [`GuardianConfig`] (for `timeout`), and a
/// [`GuardianSubagentFactory`] (for spawning the subagent).
pub struct GuardianSubagentReviewer {
    factory: Arc<dyn GuardianSubagentFactory>,
    config: GuardianConfig,
    reviewer: GuardianReviewer,
}

impl std::fmt::Debug for GuardianSubagentReviewer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GuardianSubagentReviewer")
            .field("timeout", &self.config.timeout)
            .field("subagent_enabled", &self.config.subagent_enabled)
            .finish()
    }
}

impl GuardianSubagentReviewer {
    /// Create a new `GuardianSubagentReviewer`.
    ///
    /// The `config` is cloned into the internal [`GuardianReviewer`] for
    /// decision mapping; the original is retained for `timeout`.
    pub fn new(
        config: GuardianConfig,
        factory: Arc<dyn GuardianSubagentFactory>,
    ) -> Self {
        let reviewer = GuardianReviewer::new(config.clone());
        Self {
            factory,
            config,
            reviewer,
        }
    }

    /// Run a Guardian review inside a subagent.
    ///
    /// Builds the review prompt from `conversation` + `request`, spawns
    /// a Guardian subagent via the factory, awaits the result with a
    /// timeout, and maps the parsed assessment to a
    /// [`GuardianDecision`].
    ///
    /// # Error paths (P4 渐进降级)
    ///
    /// All error paths return `Err(GuardianSubagentError)` so the caller
    /// can fall back to `SimpleGuardian::NeedUserConfirm`. The errors
    /// are, in order of the recovery cascade:
    ///
    /// - [`GuardianSubagentError::Cancelled`] — `cancel_token` fired
    /// - [`GuardianSubagentError::Timeout`] — exceeded `config.timeout`
    /// - [`GuardianSubagentError::SpawnFailed`] — factory error
    /// - [`GuardianSubagentError::SessionEnded`] — subagent errored
    /// - [`GuardianSubagentError::ParseFailed`] — non-JSON output
    pub async fn review(
        &self,
        request: &ApprovalRequest,
        conversation: &[Message],
        user_id: &str,
        parent_session_id: &str,
        cancel_token: &CancellationToken,
    ) -> Result<GuardianDecision, GuardianSubagentError> {
        // Fast pre-check: if already cancelled, don't spawn.
        if cancel_token.is_cancelled() {
            return Err(GuardianSubagentError::Cancelled);
        }

        // Build the action JSON (same shape as GuardianReviewer::check).
        let action_json = match request.to_json() {
            Ok(json) => serde_json::to_string_pretty(&json).unwrap_or_default(),
            Err(e) => {
                return Err(GuardianSubagentError::ParseFailed(format!(
                    "failed to serialize approval request: {e}"
                )));
            }
        };

        // Build the review prompt (reuses the existing transcript
        // pipeline — D5).
        let review_prompt = build_review_prompt(
            &collect_transcript_entries(conversation),
            &action_json,
            None,
        );

        // Prepend the guardian policy so the subagent sees the risk
        // criteria even if the factory does not set a separate system
        // prompt (defensive — see module docs).
        let full_prompt =
            format!("{GUARDIAN_POLICY_PROMPT}\n\n{review_prompt}");

        debug!(
            "guardian subagent review spawned for parent session {}",
            parent_session_id
        );

        // Race the subagent run (wrapped in a timeout) against
        // cancellation. `biased` ensures cancellation is checked first.
        let run_fut = self.factory.run_child(
            user_id.to_string(),
            parent_session_id.to_string(),
            full_prompt,
        );
        let timeout_fut = tokio::time::timeout(self.config.timeout, run_fut);

        let result = tokio::select! {
            biased;
            _ = cancel_token.cancelled() => {
                return Err(GuardianSubagentError::Cancelled);
            }
            outcome = timeout_fut => outcome,
        };

        // Distinguish timeout (outer Err) from factory error (inner Err).
        let agent_output = match result {
            Ok(Ok(output)) => output,
            Ok(Err(e)) => {
                warn!("guardian subagent spawn failed: {e}");
                return Err(GuardianSubagentError::SpawnFailed(e.to_string()));
            }
            Err(_) => {
                warn!(
                    "guardian subagent timed out after {:?}",
                    self.config.timeout
                );
                return Err(GuardianSubagentError::Timeout);
            }
        };

        // Check the subagent's completion status.
        if !agent_output.success {
            warn!(
                "guardian subagent session ended abnormally: {}",
                agent_output.output
            );
            return Err(GuardianSubagentError::SessionEnded(
                agent_output.output,
            ));
        }

        // Parse the assessment JSON from the subagent's output.
        let assessment = match parse_assessment_response(&agent_output.output) {
            Ok(a) => a,
            Err(e) => {
                warn!(
                    "guardian subagent parse failed: {} (output: {:?})",
                    e, agent_output.output
                );
                return Err(GuardianSubagentError::ParseFailed(e.to_string()));
            }
        };

        // Map to GuardianDecision (reuses GuardianReviewer's logic).
        let decision =
            self.reviewer.make_guardian_decision(assessment, request);
        debug!(
            "guardian subagent review decision for parent session {}: {:?}",
            parent_session_id, decision
        );
        Ok(decision)
    }

    /// Returns the configured timeout.
    pub fn timeout(&self) -> std::time::Duration {
        self.config.timeout
    }
}

#[cfg(test)]
mod tests {
    use std::{sync::Mutex, time::Duration};

    use async_trait::async_trait;
    use tokio_util::sync::CancellationToken;

    use super::*;
    use crate::{ApprovalRequest, GuardianConfig, GuardianDecision};

    /// What the mock factory should do when `run_child` is called.
    #[derive(Clone)]
    enum MockBehavior {
        /// Return a successful output immediately.
        Ok(GuardianSubagentOutput),
        /// Sleep for `delay` then return the output.
        Delay(Duration, GuardianSubagentOutput),
        /// Return a spawn error.
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
        ) -> Result<GuardianSubagentOutput, GuardianSubagentSpawnError>
        {
            let behavior = self.behavior.lock().unwrap().clone();
            match behavior {
                MockBehavior::Ok(output) => Ok(output),
                MockBehavior::Delay(d, output) => {
                    tokio::time::sleep(d).await;
                    Ok(output)
                }
                MockBehavior::Fail(msg) => {
                    Err(GuardianSubagentSpawnError::SpawnFailed(msg))
                }
            }
        }
    }

    fn low_risk_output() -> GuardianSubagentOutput {
        GuardianSubagentOutput {
            output: r#"{"risk_level":"low","risk_score":30,"rationale":"safe action","evidence":[]}"#.to_string(),
            success: true,
        }
    }

    fn high_risk_output() -> GuardianSubagentOutput {
        GuardianSubagentOutput {
            output: r#"{"risk_level":"high","risk_score":85,"rationale":"destructive","evidence":[]}"#.to_string(),
            success: true,
        }
    }

    fn make_request() -> ApprovalRequest {
        ApprovalRequest::shell("test-id", vec!["ls".to_string()], "/tmp", None)
    }

    fn make_reviewer(
        timeout: Duration,
        factory: Arc<dyn GuardianSubagentFactory>,
    ) -> GuardianSubagentReviewer {
        let config = GuardianConfig::default()
            .with_timeout(timeout)
            .with_subagent_enabled(true);
        GuardianSubagentReviewer::new(config, factory)
    }

    #[tokio::test]
    async fn review_returns_allow_for_low_risk_assessment() {
        let factory = Arc::new(MockFactory::ok(low_risk_output()));
        let reviewer = make_reviewer(Duration::from_secs(5), factory);
        let request = make_request();
        let cancel = CancellationToken::new();

        let decision = reviewer
            .review(&request, &[], "user-1", "session-1", &cancel)
            .await
            .expect("low-risk review should succeed");

        assert!(
            matches!(decision, GuardianDecision::Allow),
            "expected Allow, got {decision:?}"
        );
    }

    #[tokio::test]
    async fn review_returns_deny_for_high_risk_assessment() {
        let factory = Arc::new(MockFactory::ok(high_risk_output()));
        let reviewer = make_reviewer(Duration::from_secs(5), factory);
        let request = make_request();
        let cancel = CancellationToken::new();

        let decision = reviewer
            .review(&request, &[], "user-1", "session-1", &cancel)
            .await
            .expect("high-risk review should succeed");

        assert!(
            matches!(decision, GuardianDecision::Deny { .. }),
            "expected Deny, got {decision:?}"
        );
        if let GuardianDecision::Deny { reason } = decision {
            assert_eq!(reason, "destructive");
        }
    }

    #[tokio::test]
    async fn review_returns_err_on_timeout() {
        // Mock blocks for 2s; reviewer timeout is 100ms.
        let factory = Arc::new(MockFactory::delay(
            Duration::from_secs(2),
            low_risk_output(),
        ));
        let reviewer = make_reviewer(Duration::from_millis(100), factory);
        let request = make_request();
        let cancel = CancellationToken::new();

        let result = reviewer
            .review(&request, &[], "user-1", "session-1", &cancel)
            .await;

        assert!(
            matches!(result, Err(GuardianSubagentError::Timeout)),
            "expected Timeout, got {result:?}"
        );
    }

    #[tokio::test]
    async fn review_returns_err_on_parse_failure() {
        let factory = Arc::new(MockFactory::ok(GuardianSubagentOutput {
            output: "this is not valid JSON".to_string(),
            success: true,
        }));
        let reviewer = make_reviewer(Duration::from_secs(5), factory);
        let request = make_request();
        let cancel = CancellationToken::new();

        let result = reviewer
            .review(&request, &[], "user-1", "session-1", &cancel)
            .await;

        assert!(
            matches!(result, Err(GuardianSubagentError::ParseFailed(_))),
            "expected ParseFailed, got {result:?}"
        );
    }

    #[tokio::test]
    async fn review_returns_err_on_cancel() {
        let factory = Arc::new(MockFactory::delay(
            Duration::from_secs(10),
            low_risk_output(),
        ));
        let reviewer = make_reviewer(Duration::from_secs(5), factory);
        let request = make_request();
        let cancel = CancellationToken::new();

        // Cancel after a short delay so the review has started.
        let cancel_clone = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            cancel_clone.cancel();
        });

        let result = reviewer
            .review(&request, &[], "user-1", "session-1", &cancel)
            .await;

        assert!(
            matches!(result, Err(GuardianSubagentError::Cancelled)),
            "expected Cancelled, got {result:?}"
        );
    }

    #[tokio::test]
    async fn review_returns_err_on_spawn_failed() {
        let factory = Arc::new(MockFactory::fail("connection refused"));
        let reviewer = make_reviewer(Duration::from_secs(5), factory);
        let request = make_request();
        let cancel = CancellationToken::new();

        let result = reviewer
            .review(&request, &[], "user-1", "session-1", &cancel)
            .await;

        assert!(
            matches!(result, Err(GuardianSubagentError::SpawnFailed(_))),
            "expected SpawnFailed, got {result:?}"
        );
    }

    #[tokio::test]
    async fn review_returns_err_on_session_ended() {
        let factory = Arc::new(MockFactory::ok(GuardianSubagentOutput {
            output: "agent panicked".to_string(),
            success: false,
        }));
        let reviewer = make_reviewer(Duration::from_secs(5), factory);
        let request = make_request();
        let cancel = CancellationToken::new();

        let result = reviewer
            .review(&request, &[], "user-1", "session-1", &cancel)
            .await;

        assert!(
            matches!(result, Err(GuardianSubagentError::SessionEnded(_))),
            "expected SessionEnded, got {result:?}"
        );
    }

    #[tokio::test]
    async fn review_returns_err_when_already_cancelled() {
        let factory = Arc::new(MockFactory::ok(low_risk_output()));
        let reviewer = make_reviewer(Duration::from_secs(5), factory);
        let request = make_request();
        let cancel = CancellationToken::new();
        cancel.cancel();

        let result = reviewer
            .review(&request, &[], "user-1", "session-1", &cancel)
            .await;

        assert!(
            matches!(result, Err(GuardianSubagentError::Cancelled)),
            "expected Cancelled, got {result:?}"
        );
    }

    #[test]
    fn timeout_returns_configured_duration() {
        let factory = Arc::new(MockFactory::ok(low_risk_output()));
        let reviewer = make_reviewer(Duration::from_secs(42), factory);
        assert_eq!(reviewer.timeout(), Duration::from_secs(42));
    }
}
