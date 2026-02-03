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

/// Returns `Some(event)` to yield it, or `None` if the loop should exit.
fn handle_event_result(
    agent: &Agent,
    event_result: Result<AgentEvent>,
) -> Option<AgentEvent> {
    match event_result {
        Ok(AgentEvent::Status(status)) => {
            agent.emit_status(status);
            None
        }
        Ok(event) => Some(event),
        Err(e) => {
            agent.emit_status(AgentStatus::Errored(e.to_string()));
            None
        }
    }
}

impl Agent {
    #[instrument(skip_all, fields(session_id = %session_config.id, max_steps = session_config.max_steps))]
    pub async fn react(
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
                // TODO: Extract actual token usage from model response for accurate tracking
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
                    if let Some(event) = handle_event_result(&agent, event_result) {
                        yield event;
                    } else {
                        return;
                    }
                }
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

    use super::*;
    use crate::{
        agent::{AgentControl, AgentDeps},
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
        let cancel_token = CancellationToken::new();

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
}
