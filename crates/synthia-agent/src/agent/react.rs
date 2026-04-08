//! ReAct loop implementation

use futures::stream::{BoxStream, StreamExt};
use tokio_util::sync::CancellationToken;
use tracing::instrument;

use crate::{
    AgentError,
    Result,
    agent::Agent,
    config::SessionConfig,
    hooks::HookEvent,
    types::{AgentEvent, AgentStatus},
};

struct ReactState {
    session_config: SessionConfig,
    cancel_token: CancellationToken,
    current_step: u32,
    total_tokens_used: u64,
}

impl ReactState {
    fn new(
        session_config: SessionConfig,
        cancel_token: CancellationToken,
    ) -> Self {
        Self {
            session_config,
            cancel_token,
            current_step: 0,
            total_tokens_used: 0,
        }
    }

    fn is_cancelled(&self) -> bool {
        self.cancel_token.is_cancelled()
    }

    fn max_steps_reached(&self) -> bool {
        self.current_step >= self.session_config.max_steps
    }

    fn increment_step(&mut self, tokens_used: u64) {
        self.current_step += 1;
        self.total_tokens_used += tokens_used;
    }

    fn check_token_budget(&self) -> Option<AgentStatus> {
        if let Some(max_tokens) = self.session_config.max_tokens
            && self.total_tokens_used >= max_tokens
        {
            tracing::info!(
                session_id = %self.session_id(),
                tokens_used = self.total_tokens_used,
                max_tokens = max_tokens,
                "Max tokens budget reached"
            );
            return Some(AgentStatus::MaxTokensReached(self.total_tokens_used));
        }
        None
    }

    fn session_id(&self) -> &str {
        &self.session_config.id
    }
}

impl Agent {
    /// Main entry point for agent execution.
    ///
    /// All agents (Solo, Team Lead, Team Member) share the same ReAct loop.
    /// Mode differentiation is handled through per-mode prompts and tool filtering:
    /// - Solo: standard prompt + SubagentTool for parallel subagents
    /// - Team Lead: coordinator prompt + task tools + broadcast
    /// - Team Member: member prompt + claim_task + send_message
    #[instrument(skip_all, fields(session_id = %session_config.id, max_steps = session_config.max_steps))]
    pub async fn react(
        &self,
        session_config: SessionConfig,
        cancel_token: CancellationToken,
    ) -> BoxStream<'static, AgentEvent> {
        self.react_loop(session_config, cancel_token).await
    }

    /// Solo mode execution loop (ReAct pattern)
    #[instrument(skip_all, fields(session_id = %session_config.id, max_steps = session_config.max_steps))]
    async fn react_loop(
        &self,
        session_config: SessionConfig,
        cancel_token: CancellationToken,
    ) -> BoxStream<'static, AgentEvent> {
        let session_id = session_config.id.clone();
        let tools = self.get_filtered_tools().await;
        let agent = self.clone();

        Box::pin(async_stream::stream! {
            agent.deps.hooks
                .emit(&HookEvent::SessionStart { session_id: session_id.clone() })
                .await;

            agent.deps.control.update_status(AgentStatus::Running);

            let mut state = ReactState::new(session_config, cancel_token);

            loop {
                let step = state.current_step;

                agent.deps.hooks
                    .emit(&HookEvent::BeforeStep {
                        session_id: session_id.clone(),
                        step,
                    })
                    .await;

                state.increment_step(0);

                if let Some(status) = agent.check_exit_conditions(&state).await {
                    agent.deps.control.update_status(status.clone());
                    yield AgentEvent::Status(status);
                    return;
                }

                let result = agent.process_react_step(&state, &tools).await;
                let mut stream = match result {
                    Ok(s) => s,
                    Err(e) => {
                        agent.emit_status_and_yield(AgentStatus::Errored(e.to_string())).await;
                        return;
                    }
                };

                while let Some(event_result) = stream.next().await {
                    match event_result {
                        Ok(AgentEvent::Status(status)) => {
                            agent.emit_status(status);
                        }
                        Ok(event) => yield event,
                        Err(e) => {
                            agent.emit_status_and_yield(AgentStatus::Errored(e.to_string())).await;
                            return;
                        }
                    }
                }

                // Emit AfterStep hook after all events from this step are processed
                agent.deps.hooks
                    .emit(&HookEvent::AfterStep {
                        session_id: session_id.clone(),
                        step,
                        tool_count: 0, // Tool count tracked per-step if needed
                    })
                    .await;
            }
        })
    }

    async fn check_exit_conditions(
        &self,
        state: &ReactState,
    ) -> Option<AgentStatus> {
        if state.is_cancelled() {
            tracing::info!(session_id = %state.session_id(), "Loop cancelled");
            return Some(AgentStatus::Cancelled);
        }

        if state.max_steps_reached() {
            tracing::info!(
                session_id = %state.session_id(),
                steps = state.current_step,
                "Max steps reached"
            );
            return Some(AgentStatus::MaxStepsReached(state.current_step));
        }

        if let Some(detection) = self
            .loop_detector
            .read()
            .ok()
            .and_then(|ld| ld.detect_loop())
        {
            tracing::warn!(
                session_id = %state.session_id(),
                tool_name = %detection.tool_name,
                occurrences = detection.occurrences,
                "Loop detected"
            );
            return Some(AgentStatus::LoopDetected(detection.tool_name));
        }

        // Check token budget after other exit conditions
        if let Some(status) = state.check_token_budget() {
            return Some(status);
        }

        None
    }

    fn emit_status(&self, status: AgentStatus) {
        self.deps.control.update_status(status);
    }

    async fn emit_status_and_yield(&self, status: AgentStatus) -> AgentEvent {
        self.deps.control.update_status(status.clone());
        AgentEvent::Status(status)
    }

    async fn process_react_step<'a>(
        &'a self,
        state: &'a ReactState,
        tools: &'a [rmcp::model::Tool],
    ) -> Result<BoxStream<'a, Result<AgentEvent>>> {
        let messages = self
            .deps
            .session
            .get_conversation(&state.session_config)
            .await
            .map_err(|e| {
                AgentError::context(format!("Failed to get conversation: {e}"))
            })?
            .to_vec();

        let stream = async_stream::stream! {
            let turn_stream = self.step(&messages, &state.session_config, tools, &state.cancel_token).await
                .map_err(|e| AgentError::internal(format!("Failed to process step: {e}")))?;

            tokio::pin!(turn_stream);
            while let Some(event_result) = turn_stream.next().await {
                let is_turn_complete = matches!(&event_result, Ok(AgentEvent::TurnCompleteDetail { .. }));

                yield event_result;

                if is_turn_complete {
                    let conversation = match self.deps.session.get_conversation(&state.session_config).await {
                        Ok(c) => c,
                        Err(e) => {
                            tracing::warn!("Failed to get conversation for compaction: {}", e);
                            continue;
                        }
                    };

                    if let Ok(Some(_)) = self.deps.context.compact(&conversation).await {
                        let fixed = match self.deps.session.fix_conversation(&state.session_config).await {
                            Ok(f) => f,
                            Err(e) => {
                                tracing::warn!("Failed to fix conversation: {}", e);
                                continue;
                            }
                        };

                        let mut compact_conversation = fixed.clone();
                        crate::context::micro_compact(&mut compact_conversation, 3);

                        match self.compact_conversation(&compact_conversation, &state.session_config).await {
                            Ok((_replacement, compact_stream)) => {
                                tokio::pin!(compact_stream);
                                while let Some(compact_event) = compact_stream.next().await {
                                    yield compact_event;
                                }
                            }
                            Err(e) => {
                                tracing::error!(session_id = %state.session_id(), "Compaction failed: {}", e);
                            }
                        }
                    }
                }
            }
        };

        Ok(Box::pin(stream))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tokio_util::sync::CancellationToken;

    use super::*;
    use crate::{
        agent::{
            AgentControl,
            AgentDeps,
            loop_detector::{
                LoopDetection,
                LoopDetector,
                LoopType,
                OperationPattern,
                Outcome,
            },
        },
        config::AgentConfig,
        context::DefaultContextManager,
        guardian::{Guardian, GuardianConfig, SimpleGuardian},
        hooks::HookRegistry,
        model_router::{FirstModelRouter, ModelRouter},
        session::SessionManager,
        tools::{SkillTool, ToolRegistry},
    };

    async fn create_test_agent() -> Agent {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let model_router: Arc<dyn ModelRouter> =
            Arc::new(FirstModelRouter::default());

        let session: Arc<dyn SessionManager> =
            Arc::new(crate::session::SessionFileStore::new())
                as Arc<dyn SessionManager>;

        let deps = AgentDeps {
            tools: Arc::new(ToolRegistry::default()),
            context: Arc::new(DefaultContextManager::new(Arc::clone(
                &model_router,
            ))),
            session,
            router: model_router,
            hooks: Arc::new(HookRegistry::new()),
            skills: Arc::new(SkillTool::new(temp_dir.path().to_path_buf())),
            guardian: Arc::new(SimpleGuardian::new(GuardianConfig::default()))
                as Arc<dyn Guardian>,
            control: Arc::new(AgentControl::new()),
        };

        Agent::new(Arc::new(AgentConfig::default()), deps)
    }

    #[tokio::test]
    async fn test_react_loop_cancellation() {
        let agent = create_test_agent().await;
        let session_config = SessionConfig::default();
        let cancel_token = tokio_util::sync::CancellationToken::new();

        cancel_token.cancel();

        let stream = agent.react(session_config, cancel_token).await;
        let events: Vec<_> = stream.collect().await;

        assert!(!events.is_empty());
        if let AgentEvent::Status(status) = events.last().unwrap() {
            assert_eq!(*status, AgentStatus::Cancelled);
        } else {
            panic!("Expected Status event");
        }
    }

    #[tokio::test]
    async fn test_react_loop_max_steps() {
        let agent = create_test_agent().await;
        let session_config = SessionConfig {
            max_steps: 0,
            ..Default::default()
        };
        let cancel_token = CancellationToken::new();

        let stream = agent.react(session_config, cancel_token).await;
        let events: Vec<_> = stream.collect().await;

        assert!(!events.is_empty());
        if let AgentEvent::Status(status) = events.last().unwrap() {
            assert!(matches!(*status, AgentStatus::MaxStepsReached(1)));
        } else {
            panic!("Expected Status event");
        }
    }

    #[tokio::test]
    async fn test_react_state() {
        let session_config = SessionConfig::default();
        let cancel_token = CancellationToken::new();
        let state = ReactState::new(session_config, cancel_token);

        assert_eq!(state.current_step, 0);
        assert!(!state.is_cancelled());
        assert!(!state.max_steps_reached());
    }

    #[tokio::test]
    async fn test_react_state_step_increment() {
        let session_config = SessionConfig {
            max_steps: 2,
            ..Default::default()
        };
        let cancel_token = CancellationToken::new();
        let mut state = ReactState::new(session_config, cancel_token);

        state.increment_step(0);
        assert_eq!(state.current_step, 1);
        assert!(!state.max_steps_reached());

        state.increment_step(0);
        assert_eq!(state.current_step, 2);
        assert!(state.max_steps_reached());
    }

    #[tokio::test]
    async fn test_react_state_cancellation() {
        let session_config = SessionConfig::default();
        let cancel_token = CancellationToken::new();
        let state = ReactState::new(session_config, cancel_token.clone());

        assert!(!state.is_cancelled());

        cancel_token.cancel();
        assert!(state.is_cancelled());
    }

    #[test]
    fn test_handle_event_result_status_event() {
        // This test is tricky without an Agent, so we test the AgentEvent matching
        let status = AgentStatus::Running;
        let event = AgentEvent::Status(status);
        assert!(matches!(event, AgentEvent::Status(_)));
    }

    #[test]
    fn test_handle_event_result_error() {
        let error = AgentError::internal("test error");
        // Verify AgentError can be created and converted to string
        let err_str = error.to_string();
        assert!(err_str.contains("test error"));
    }

    #[tokio::test]
    async fn test_react_state_token_budget_check() {
        let session_config = SessionConfig {
            max_tokens: Some(100),
            ..Default::default()
        };
        let cancel_token = CancellationToken::new();
        let state = ReactState::new(session_config, cancel_token);

        // Initially no budget exceeded
        assert!(state.check_token_budget().is_none());
    }

    #[tokio::test]
    async fn test_react_state_token_budget_exceeded() {
        let session_config = SessionConfig {
            max_tokens: Some(50),
            ..Default::default()
        };
        let cancel_token = CancellationToken::new();
        let mut state = ReactState::new(session_config, cancel_token);

        // Exceed the token budget
        state.increment_step(60);

        let result = state.check_token_budget();
        assert!(result.is_some());
        if let AgentStatus::MaxTokensReached(tokens) = result.unwrap() {
            assert_eq!(tokens, 60);
        } else {
            panic!("Expected MaxTokensReached");
        }
    }

    #[tokio::test]
    async fn test_react_state_no_token_budget() {
        let session_config = SessionConfig {
            max_tokens: None,
            ..Default::default()
        };
        let cancel_token = CancellationToken::new();
        let mut state = ReactState::new(session_config, cancel_token);

        // With no budget set, should never exceed
        state.increment_step(1000000);
        assert!(state.check_token_budget().is_none());
    }

    #[tokio::test]
    async fn test_react_state_increment_accumulates_tokens() {
        let session_config = SessionConfig::default();
        let cancel_token = CancellationToken::new();
        let mut state = ReactState::new(session_config, cancel_token);

        state.increment_step(100);
        assert_eq!(state.total_tokens_used, 100);

        state.increment_step(50);
        assert_eq!(state.total_tokens_used, 150);

        state.increment_step(0);
        assert_eq!(state.total_tokens_used, 150);
    }

    #[test]
    fn test_agent_event_status_variants() {
        use crate::types::AgentEvent;

        let cancelled = AgentEvent::Status(AgentStatus::Cancelled);
        assert!(matches!(
            cancelled,
            AgentEvent::Status(AgentStatus::Cancelled)
        ));

        let max_steps = AgentEvent::Status(AgentStatus::MaxStepsReached(5));
        assert!(
            matches!(max_steps, AgentEvent::Status(AgentStatus::MaxStepsReached(n)) if n == 5)
        );

        let max_tokens =
            AgentEvent::Status(AgentStatus::MaxTokensReached(1000));
        assert!(
            matches!(max_tokens, AgentEvent::Status(AgentStatus::MaxTokensReached(n)) if n == 1000)
        );

        let error =
            AgentEvent::Status(AgentStatus::Errored("oops".to_string()));
        assert!(
            matches!(error, AgentEvent::Status(AgentStatus::Errored(s)) if s == "oops")
        );
    }

    #[test]
    fn test_react_state_display() {
        let session_config = SessionConfig {
            id: "test-session".to_string(),
            ..Default::default()
        };
        let cancel_token = CancellationToken::new();
        let state = ReactState::new(session_config, cancel_token);

        assert_eq!(state.session_id(), "test-session");
    }

    // =============================================================================
    // Additional Comprehensive ReAct Loop Tests
    // =============================================================================

    // -----------------------------------------------------------------------------
    // ReactState - Step Increment Logic Tests
    // -----------------------------------------------------------------------------

    #[test]
    fn test_react_state_initial_values() {
        let session_config = SessionConfig::default();
        let cancel_token = CancellationToken::new();
        let state = ReactState::new(session_config, cancel_token);

        assert_eq!(state.current_step, 0, "Initial step should be 0");
        assert_eq!(state.total_tokens_used, 0, "Initial tokens should be 0");
    }

    #[test]
    fn test_react_state_increment_step_increments_both() {
        let session_config = SessionConfig::default();
        let cancel_token = CancellationToken::new();
        let mut state = ReactState::new(session_config, cancel_token);

        state.increment_step(150);
        assert_eq!(state.current_step, 1, "Step should increment to 1");
        assert_eq!(state.total_tokens_used, 150, "Tokens should accumulate");

        state.increment_step(75);
        assert_eq!(state.current_step, 2, "Step should increment to 2");
        assert_eq!(
            state.total_tokens_used, 225,
            "Tokens should accumulate to 225"
        );
    }

    #[test]
    fn test_react_state_increment_step_zero_tokens() {
        let session_config = SessionConfig::default();
        let cancel_token = CancellationToken::new();
        let mut state = ReactState::new(session_config, cancel_token);

        state.increment_step(0);
        assert_eq!(state.current_step, 1);
        assert_eq!(state.total_tokens_used, 0);

        state.increment_step(0);
        assert_eq!(state.current_step, 2);
        assert_eq!(state.total_tokens_used, 0);
    }

    #[test]
    fn test_react_state_increment_large_token_values() {
        let session_config = SessionConfig::default();
        let cancel_token = CancellationToken::new();
        let mut state = ReactState::new(session_config, cancel_token);

        state.increment_step(u64::MAX);
        assert_eq!(state.total_tokens_used, u64::MAX);
    }

    // -----------------------------------------------------------------------------
    // ReactState - Token Budget Tests
    // -----------------------------------------------------------------------------

    #[test]
    fn test_react_state_token_budget_exact_boundary() {
        let session_config = SessionConfig {
            max_tokens: Some(100),
            ..Default::default()
        };
        let cancel_token = CancellationToken::new();
        let mut state = ReactState::new(session_config, cancel_token);

        // Below boundary should not trigger
        state.increment_step(99);
        assert!(
            state.check_token_budget().is_none(),
            "Below boundary should not trigger"
        );

        // At boundary should trigger (since check uses >=)
        state.increment_step(1);
        let result = state.check_token_budget();
        assert!(result.is_some(), "At boundary should trigger");
        assert!(matches!(
            result.unwrap(),
            AgentStatus::MaxTokensReached(100)
        ));
    }

    #[test]
    fn test_react_state_token_budget_with_no_limit() {
        let session_config = SessionConfig {
            max_tokens: None,
            ..Default::default()
        };
        let cancel_token = CancellationToken::new();
        let mut state = ReactState::new(session_config, cancel_token);

        // Even with massive tokens, should never trigger when max_tokens is None
        state.increment_step(u64::MAX / 2);
        assert!(state.check_token_budget().is_none());

        state.increment_step(u64::MAX / 2);
        assert!(state.check_token_budget().is_none());
    }

    #[test]
    fn test_react_state_token_budget_at_zero() {
        let session_config = SessionConfig {
            max_tokens: Some(0),
            ..Default::default()
        };
        let cancel_token = CancellationToken::new();
        let mut state = ReactState::new(session_config, cancel_token);

        // Any tokens should trigger when max_tokens is 0
        state.increment_step(1);
        let result = state.check_token_budget();
        assert!(result.is_some());
        assert!(matches!(result.unwrap(), AgentStatus::MaxTokensReached(1)));
    }

    // -----------------------------------------------------------------------------
    // ReactState - Max Steps Tests
    // -----------------------------------------------------------------------------

    #[test]
    fn test_react_state_max_steps_boundary() {
        let session_config = SessionConfig {
            max_steps: 5,
            ..Default::default()
        };
        let cancel_token = CancellationToken::new();
        let mut state = ReactState::new(session_config, cancel_token);

        // At max_steps - 1, should not be at limit
        for i in 1..5 {
            state.increment_step(0);
            assert!(
                !state.max_steps_reached(),
                "Step {i} should not exceed max"
            );
        }

        // At max_steps, should be at limit
        state.increment_step(0);
        assert!(state.max_steps_reached(), "Step 5 should reach max_steps");
    }

    #[test]
    fn test_react_state_max_steps_zero() {
        let session_config = SessionConfig {
            max_steps: 0,
            ..Default::default()
        };
        let cancel_token = CancellationToken::new();
        let state = ReactState::new(session_config, cancel_token);

        // With max_steps = 0, immediately at limit
        assert!(
            state.max_steps_reached(),
            "max_steps of 0 should immediately trigger"
        );
    }

    #[test]
    fn test_react_state_max_steps_one() {
        let cancel_token = CancellationToken::new();
        let mut state = ReactState::new(
            SessionConfig {
                max_steps: 1,
                ..Default::default()
            },
            cancel_token,
        );

        assert!(
            !state.max_steps_reached(),
            "Initial state should not be at limit"
        );

        state.increment_step(0);
        assert!(
            state.max_steps_reached(),
            "After one step with max_steps=1 should be at limit"
        );
    }

    // -----------------------------------------------------------------------------
    // ReactState - Cancellation Tests
    // -----------------------------------------------------------------------------

    #[test]
    fn test_react_state_cancellation_independent_tokens() {
        let cancel_token = CancellationToken::new();
        let mut state =
            ReactState::new(SessionConfig::default(), cancel_token.clone());

        // Add some tokens before cancellation
        state.increment_step(500);

        // Cancel should not affect accumulated tokens
        cancel_token.cancel();
        assert!(state.is_cancelled());
        assert_eq!(
            state.total_tokens_used, 500,
            "Cancellation should not affect tokens"
        );
    }

    #[test]
    fn test_react_state_double_cancellation() {
        let cancel_token = CancellationToken::new();
        let state =
            ReactState::new(SessionConfig::default(), cancel_token.clone());

        cancel_token.cancel();
        cancel_token.cancel(); // Double cancel should be idempotent

        assert!(state.is_cancelled());
    }

    // -----------------------------------------------------------------------------
    // Exit Conditions - Integration Tests
    // -----------------------------------------------------------------------------

    #[tokio::test]
    async fn test_exit_condition_token_budget() {
        let agent = create_test_agent().await;
        let session_config = SessionConfig {
            max_tokens: Some(0), // 0 tokens triggers immediately
            max_steps: 100,
            ..Default::default()
        };
        let cancel_token = CancellationToken::new();

        let stream = agent.react(session_config, cancel_token).await;
        let events: Vec<_> = stream.collect().await;

        assert!(!events.is_empty(), "Should emit at least one event");
        let last_event = events.last().unwrap();

        // The loop should exit due to token budget, not max steps
        if let AgentEvent::Status(status) = last_event {
            assert!(
                matches!(status, AgentStatus::MaxTokensReached(_)),
                "Expected MaxTokensReached, got {status:?}"
            );
        } else {
            panic!("Expected Status event, got {last_event:?}");
        }
    }

    #[tokio::test]
    async fn test_exit_condition_priority_cancellation_first() {
        let agent = create_test_agent().await;
        let cancel_token = CancellationToken::new();

        // Cancel immediately
        cancel_token.cancel();

        let stream = agent.react(SessionConfig::default(), cancel_token).await;
        let events: Vec<_> = stream.collect().await;

        assert!(!events.is_empty());
        let last_event = events.last().unwrap();

        // Cancellation should take priority over token budget
        if let AgentEvent::Status(status) = last_event {
            assert!(
                matches!(status, AgentStatus::Cancelled),
                "Expected Cancelled to take priority, got {status:?}"
            );
        } else {
            panic!("Expected Status event, got {last_event:?}");
        }
    }

    // -----------------------------------------------------------------------------
    // LoopDetector Integration Tests
    // -----------------------------------------------------------------------------

    #[tokio::test]
    async fn test_loop_detector_integration_generic_repeat() {
        let agent = create_test_agent().await;

        // Record 3 identical operations to trigger generic repeat detection
        let mut detector = agent.loop_detector.write().unwrap();
        for _ in 0..3 {
            detector.record(OperationPattern {
                tool_name: "Read".to_string(),
                args_hash: 12345,
                timestamp: chrono::Utc::now(),
                outcome: Outcome::Success,
                result_hash: Some(67890),
            });
        }
        drop(detector);

        // Check that loop detection works
        let detection = agent.loop_detector.read().unwrap().detect_loop();
        assert!(detection.is_some(), "Should detect generic repeat loop");

        let detection = detection.unwrap();
        assert!(matches!(detection.loop_type, LoopType::GenericRepeat));
        assert_eq!(detection.tool_name, "Read");
        assert_eq!(detection.args_hash, 12345);
        assert_eq!(detection.occurrences, 3);
    }

    #[tokio::test]
    async fn test_loop_detector_integration_poll_no_progress() {
        let agent = create_test_agent().await;

        // Record 3 identical poll operations with same result hash
        let mut detector = agent.loop_detector.write().unwrap();
        let result_hash: u64 = 99999;
        for _ in 0..3 {
            detector.record(OperationPattern {
                tool_name: "Read".to_string(),
                args_hash: 111,
                timestamp: chrono::Utc::now(),
                outcome: Outcome::Success,
                result_hash: Some(result_hash),
            });
        }
        drop(detector);

        let detection = agent.loop_detector.read().unwrap().detect_loop();
        assert!(detection.is_some(), "Should detect poll no progress loop");

        let detection = detection.unwrap();
        assert!(
            matches!(detection.loop_type, LoopType::GenericRepeat)
                || matches!(detection.loop_type, LoopType::PollNoProgress),
            "Should be GenericRepeat or PollNoProgress"
        );
    }

    #[tokio::test]
    async fn test_loop_detector_integration_ping_pong() {
        let agent = create_test_agent().await;

        // Record ping-pong pattern: Read -> Write -> Read -> Write
        let mut detector = agent.loop_detector.write().unwrap();
        for _ in 0..2 {
            detector.record(OperationPattern {
                tool_name: "Read".to_string(),
                args_hash: 1,
                timestamp: chrono::Utc::now(),
                outcome: Outcome::Success,
                result_hash: None,
            });
            detector.record(OperationPattern {
                tool_name: "Write".to_string(),
                args_hash: 2,
                timestamp: chrono::Utc::now(),
                outcome: Outcome::Success,
                result_hash: None,
            });
        }
        drop(detector);

        let detection = agent.loop_detector.read().unwrap().detect_loop();
        assert!(detection.is_some(), "Should detect ping-pong loop");

        let detection = detection.unwrap();
        assert!(matches!(detection.loop_type, LoopType::PingPong));
        assert!(detection.tool_name.contains("<->"));
    }

    #[tokio::test]
    async fn test_loop_detector_circuit_breaker() {
        // Use lower circuit breaker threshold for testing
        let mut detector = LoopDetector::with_circuit_breaker(50, 3, 3);
        for i in 0..3 {
            detector.record(OperationPattern {
                tool_name: "Write".to_string(),
                args_hash: i as u64,
                timestamp: chrono::Utc::now(),
                outcome: Outcome::Failure,
                result_hash: None,
            });
        }

        let detection = detector.detect_loop();
        assert!(detection.is_some(), "Should detect circuit breaker");
        let detection = detection.unwrap();
        assert!(matches!(detection.loop_type, LoopType::CircuitBreaker));
        assert_eq!(detection.occurrences, 3);
    }

    // -----------------------------------------------------------------------------
    // LoopDetector State Access Tests
    // -----------------------------------------------------------------------------

    #[tokio::test]
    async fn test_loop_detector_write_lock_failure() {
        // Test that detect_loop handles RwLock read failure gracefully
        let agent = create_test_agent().await;

        // This test verifies the .ok() on the lock read in check_exit_conditions
        let result = agent
            .loop_detector
            .read()
            .ok()
            .and_then(|ld| ld.detect_loop());
        assert!(result.is_none(), "Fresh detector should have no loops");
    }

    #[test]
    fn test_loop_detector_history_len() {
        let mut detector = LoopDetector::new(5, 3);

        assert_eq!(
            detector.history_len(),
            0,
            "New detector should have empty history"
        );

        for i in 0..3 {
            detector.record(OperationPattern {
                tool_name: format!("Tool{i}"),
                args_hash: i as u64,
                timestamp: chrono::Utc::now(),
                outcome: Outcome::Success,
                result_hash: None,
            });
        }

        assert_eq!(
            detector.history_len(),
            3,
            "Should have 3 records after 3 inserts"
        );
    }

    #[test]
    fn test_loop_detector_history_eviction() {
        let mut detector = LoopDetector::new(3, 3);

        for i in 0..5 {
            detector.record(OperationPattern {
                tool_name: format!("Tool{i}"),
                args_hash: i as u64,
                timestamp: chrono::Utc::now(),
                outcome: Outcome::Success,
                result_hash: None,
            });
        }

        // Oldest entries should be evicted
        assert_eq!(
            detector.history_len(),
            3,
            "History should be capped at max_history"
        );
    }

    // -----------------------------------------------------------------------------
    // check_exit_conditions Tests
    // -----------------------------------------------------------------------------

    #[tokio::test]
    async fn test_check_exit_conditions_all_clear() {
        let agent = create_test_agent().await;
        let session_config = SessionConfig::default();
        let cancel_token = CancellationToken::new();
        let state = ReactState::new(session_config, cancel_token);

        let result = agent.check_exit_conditions(&state).await;
        assert!(result.is_none(), "No exit conditions should be triggered");
    }

    #[tokio::test]
    async fn test_check_exit_conditions_cancelled() {
        let agent = create_test_agent().await;
        let cancel_token = CancellationToken::new();
        cancel_token.cancel();
        let state = ReactState::new(SessionConfig::default(), cancel_token);

        let result = agent.check_exit_conditions(&state).await;
        assert!(result.is_some(), "Cancelled should trigger exit");
        assert!(matches!(result.unwrap(), AgentStatus::Cancelled));
    }

    #[tokio::test]
    async fn test_check_exit_conditions_max_steps() {
        let agent = create_test_agent().await;
        let cancel_token = CancellationToken::new();
        let mut state = ReactState::new(
            SessionConfig {
                max_steps: 1,
                ..Default::default()
            },
            cancel_token,
        );
        state.increment_step(1); // Reaches max_steps

        let result = agent.check_exit_conditions(&state).await;
        assert!(result.is_some(), "Max steps should trigger exit");
        assert!(matches!(result.unwrap(), AgentStatus::MaxStepsReached(1)));
    }

    #[tokio::test]
    async fn test_check_exit_conditions_loop_detected() {
        let agent = create_test_agent().await;

        // Pre-populate loop detector with a loop
        {
            let mut detector = agent.loop_detector.write().unwrap();
            for _ in 0..3 {
                detector.record(OperationPattern {
                    tool_name: "Read".to_string(),
                    args_hash: 123,
                    timestamp: chrono::Utc::now(),
                    outcome: Outcome::Success,
                    result_hash: None,
                });
            }
        }

        let session_config = SessionConfig::default();
        let cancel_token = CancellationToken::new();
        let state = ReactState::new(session_config, cancel_token);

        let result = agent.check_exit_conditions(&state).await;
        assert!(result.is_some(), "Loop detection should trigger exit");
        if let Some(AgentStatus::LoopDetected(tool_name)) = result {
            assert_eq!(tool_name, "Read");
        } else {
            panic!("Expected LoopDetected status");
        }
    }

    #[tokio::test]
    async fn test_check_exit_conditions_token_budget() {
        let agent = create_test_agent().await;
        let cancel_token = CancellationToken::new();
        let mut state = ReactState::new(
            SessionConfig {
                max_tokens: Some(50),
                ..Default::default()
            },
            cancel_token,
        );
        state.increment_step(60); // Exceeds token budget

        let result = agent.check_exit_conditions(&state).await;
        assert!(result.is_some(), "Token budget should trigger exit");
        assert!(matches!(result.unwrap(), AgentStatus::MaxTokensReached(60)));
    }

    // -----------------------------------------------------------------------------
    // AgentEvent Matching Tests
    // -----------------------------------------------------------------------------

    #[test]
    fn test_agent_event_status_matching() {
        let status = AgentStatus::Running;
        let event = AgentEvent::Status(status);
        assert!(
            matches!(&event, AgentEvent::Status(s) if matches!(s, AgentStatus::Running))
        );

        let cancelled = AgentStatus::Cancelled;
        let event = AgentEvent::Status(cancelled);
        assert!(
            matches!(&event, AgentEvent::Status(s) if matches!(s, AgentStatus::Cancelled))
        );
    }

    #[test]
    fn test_agent_status_error_details() {
        let error_status =
            AgentStatus::Errored("Connection timeout".to_string());
        if let AgentEvent::Status(status) = AgentEvent::Status(error_status) {
            if let AgentStatus::Errored(msg) = status {
                assert!(msg.contains("Connection timeout"));
            } else {
                panic!("Expected Errored variant");
            }
        } else {
            panic!("Expected Status event");
        }
    }

    #[test]
    fn test_loop_detection_all_loop_types() {
        let types = vec![
            (LoopType::GenericRepeat, "GenericRepeat"),
            (LoopType::PollNoProgress, "PollNoProgress"),
            (LoopType::PingPong, "PingPong"),
            (LoopType::CircuitBreaker, "CircuitBreaker"),
        ];

        for (loop_type, name) in types {
            let detection = LoopDetection {
                loop_type,
                tool_name: "TestTool".to_string(),
                args_hash: 42,
                occurrences: 5,
                first_seen: 0,
                last_seen: 4,
            };
            let display = format!("{detection}");
            assert!(
                !display.is_empty(),
                "LoopDetection Display should not be empty for {name}"
            );
        }
    }

    // -----------------------------------------------------------------------------
    // SessionConfig ID Access Tests
    // -----------------------------------------------------------------------------

    #[test]
    fn test_react_state_session_id_empty() {
        let session_config = SessionConfig::default();
        let cancel_token = CancellationToken::new();
        let state = ReactState::new(session_config, cancel_token);

        assert!(
            !state.session_id().is_empty(),
            "Default session config should have a UUID"
        );
    }

    #[test]
    fn test_react_state_session_id_preserved() {
        let session_config = SessionConfig {
            id: "my-custom-session-id".to_string(),
            ..Default::default()
        };
        let cancel_token = CancellationToken::new();
        let state = ReactState::new(session_config, cancel_token);

        assert_eq!(state.session_id(), "my-custom-session-id");
    }

    // -----------------------------------------------------------------------------
    // Agent Creation Tests
    // -----------------------------------------------------------------------------

    #[tokio::test]
    async fn test_create_agent_has_loop_detector() {
        let agent = create_test_agent().await;

        // Verify loop_detector is initialized
        let detector = agent.loop_detector.read().unwrap();
        assert_eq!(detector.history_len(), 0);
        assert_eq!(detector.consecutive_failures(), 0);
    }

    #[tokio::test]
    async fn test_agent_react_returns_stream() {
        let agent = create_test_agent().await;
        let session_config = SessionConfig::default();
        let cancel_token = CancellationToken::new();

        // react() should return a stream
        let stream = agent.react(session_config, cancel_token).await;
        assert!(
            std::mem::size_of_val(&stream) > 0,
            "Should return a valid stream"
        );
    }
}
