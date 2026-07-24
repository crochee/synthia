use std::time::{Duration, Instant};

use synthia_context::traits::estimate_message_tokens;
use synthia_provider::types::Message;
use synthia_telemetry::span_context::SpanContext;

use crate::{events::SessionEndReason, turn::TurnId};

const MAX_RECENT_TOOL_RESULTS: usize = 100;

pub struct LoopContext {
    pub session_id: String,
    pub iteration: usize,
    pub messages: Vec<Message>,
    pub end_reason: Option<SessionEndReason>,
    pub cumulative_tokens: usize,
    pub recent_tool_results: Vec<(String, String, bool)>,
    pub needs_compact: bool,
    pub span_ctx: SpanContext,
    /// Optional hard token limit used by `token_ratio()`. When set,
    /// `token_ratio()` returns `current_message_tokens / context_token_limit`.
    pub context_token_limit: Option<usize>,
    /// Stable per-turn identifier (`TurnId(Uuid)`) used for cross-event
    /// turn correlation in observability. Generated once at session start
    /// and re-assigned via `set_turn_id` when a new turn begins.
    pub current_turn_id: Option<TurnId>,
    /// 会话开始的墙上时钟时间戳，用于 `should_stop_with_timeout`
    /// 判断会话是否超过最大运行时长。`None` 表示不启用超时检查。
    pub session_start: Option<Instant>,
    /// Next iteration at which the `self_reflect` auto-trigger fallback
    /// should fire if the LLM has not already invoked the tool. Reset to
    /// `iteration + 5` whenever a `self_reflect` call (LLM or auto-
    /// triggered) is processed.
    pub next_self_reflect_iteration: usize,
}

impl LoopContext {
    pub fn new(session_id: String, span_ctx: SpanContext) -> Self {
        Self {
            session_id,
            iteration: 0,
            messages: Vec::new(),
            end_reason: None,
            cumulative_tokens: 0,
            recent_tool_results: Vec::new(),
            needs_compact: false,
            span_ctx,
            context_token_limit: None,
            current_turn_id: None,
            session_start: Some(Instant::now()),
            next_self_reflect_iteration: 5,
        }
    }

    pub fn with_messages(mut self, messages: Vec<Message>) -> Self {
        self.messages = messages;
        self
    }

    pub fn with_iteration(mut self, iteration: usize) -> Self {
        self.iteration = iteration;
        self
    }

    /// Sets the hard token limit used by `token_ratio()`.
    pub fn with_token_limit(mut self, limit: usize) -> Self {
        self.context_token_limit = Some(limit);
        self
    }

    /// Restore loop state from persisted [`synthia_session::SessionMetadata`].
    /// Used during checkpoint-resume to pick up `iteration`,
    /// `end_reason`, `cumulative_tokens`, and `context_token_limit`.
    pub fn from_metadata(
        session_id: String,
        span_ctx: SpanContext,
        metadata: &synthia_session::SessionMetadata,
    ) -> Self {
        let end_reason = metadata
            .end_reason
            .as_ref()
            .and_then(|s| serde_json::from_str::<SessionEndReason>(s).ok());
        Self {
            session_id,
            iteration: metadata.iteration,
            messages: Vec::new(),
            end_reason,
            cumulative_tokens: metadata.cumulative_tokens,
            recent_tool_results: Vec::new(),
            needs_compact: false,
            span_ctx,
            context_token_limit: metadata.context_token_limit,
            current_turn_id: None,
            // 恢复时重新计时：会话墙上时钟从恢复点开始。
            session_start: Some(Instant::now()),
            // Schedule the next auto-trigger a full period after the
            // restored iteration so a resumed session does not instantly
            // re-reflect.
            next_self_reflect_iteration: metadata.iteration.saturating_add(5),
        }
    }

    pub fn increment_iteration(&mut self) {
        self.iteration += 1;
    }

    /// Assign a fresh `TurnId` and store it as `current_turn_id`. Returns
    /// the new id for the caller's convenience.
    pub fn assign_new_turn_id(&mut self) -> TurnId {
        let id = TurnId::new();
        self.current_turn_id = Some(id);
        id
    }

    pub fn set_end_reason(&mut self, reason: SessionEndReason) {
        self.end_reason = Some(reason);
    }

    pub fn add_tool_result(
        &mut self,
        name: String,
        tool_call_id: String,
        summary: String,
        success: bool,
    ) {
        if self.recent_tool_results.len() >= MAX_RECENT_TOOL_RESULTS {
            self.recent_tool_results.remove(0);
        }
        self.recent_tool_results
            .push((name.clone(), summary.clone(), success));
        // Inject the tool result into the LLM-facing message log so the
        // next iteration's `CompletionRequest` carries it. Without this,
        // the LLM never sees what the tool returned and cannot ground
        // its next response on the tool's output. The `tool_call_id`
        // matches the `ToolUse.id` from the assistant message that
        // requested the call; the provider contract (Anthropic /
        // OpenAI) requires this id be present on `Role::Tool` messages.
        // Errors are NOT injected as `Role::Tool` messages: the
        // recovery cascade in `StreamBuilder` handles the error path
        // separately by re-prompting the LLM with a guidance message,
        // so we silently skip the message push for failures to avoid
        // surfacing internal error text as a user-visible tool result.
        if success {
            self.messages.push(Message::tool(
                synthia_provider::Content::text(summary),
                tool_call_id,
            ));
        }
    }

    /// 检查会话是否应停止：迭代上限 + 可选的墙上时钟超时。
    ///
    /// 当 `wall_clock_timeout` 为 `None` 或 `Some(Duration::ZERO)` 时
    /// 不检查时间，仅检查 `end_reason` 和迭代数。否则会比较
    /// `session_start` 距今的已过时间是否超过超时阈值。
    pub fn should_stop_with_timeout(
        &self,
        max_iterations: usize,
        wall_clock_timeout: Option<Duration>,
    ) -> bool {
        if self.end_reason.is_some() || self.iteration >= max_iterations {
            return true;
        }
        if let (Some(start), Some(timeout)) =
            (self.session_start, wall_clock_timeout)
            && timeout > Duration::ZERO
            && start.elapsed() >= timeout
        {
            return true;
        }
        false
    }

    pub fn should_stop(&self, max_iterations: usize) -> bool {
        self.should_stop_with_timeout(max_iterations, None)
    }

    /// Record that a `self_reflect` call (LLM-driven or auto-triggered)
    /// was processed this iteration. Reschedules the auto-trigger fallback
    /// to `iteration + 5`.
    pub fn record_self_reflect_call(&mut self) {
        self.next_self_reflect_iteration = self.iteration + 5;
    }

    /// Returns the current context utilization ratio (0.0-1.0+) of the
    /// configured `context_token_limit`. Returns 0.0 when no limit is set
    /// or the limit is zero.
    pub fn token_ratio(&self) -> f64 {
        let Some(limit) = self.context_token_limit else {
            return 0.0;
        };
        if limit == 0 {
            return 0.0;
        }
        let tokens: usize =
            self.messages.iter().map(estimate_message_tokens).sum();
        tokens as f64 / limit as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_loop_context_new() {
        let span_ctx = SpanContext::new("test-session");
        let ctx = LoopContext::new("session-123".to_string(), span_ctx);
        assert_eq!(ctx.session_id, "session-123");
        assert_eq!(ctx.iteration, 0);
        assert!(ctx.messages.is_empty());
        assert!(ctx.end_reason.is_none());
        assert_eq!(ctx.cumulative_tokens, 0);
        assert!(ctx.recent_tool_results.is_empty());
        assert!(!ctx.needs_compact);
    }

    #[test]
    fn test_loop_context_with_messages() {
        let span_ctx = SpanContext::new("test-session");
        let messages = vec![Message::user("hello")];
        let ctx = LoopContext::new("session-123".to_string(), span_ctx)
            .with_messages(messages);
        assert_eq!(ctx.messages.len(), 1);
    }

    #[test]
    fn test_loop_context_with_iteration() {
        let span_ctx = SpanContext::new("test-session");
        let ctx = LoopContext::new("session-123".to_string(), span_ctx)
            .with_iteration(5);
        assert_eq!(ctx.iteration, 5);
    }

    #[test]
    fn test_increment_iteration() {
        let span_ctx = SpanContext::new("test-session");
        let mut ctx = LoopContext::new("session-123".to_string(), span_ctx);
        assert_eq!(ctx.iteration, 0);
        ctx.increment_iteration();
        assert_eq!(ctx.iteration, 1);
        ctx.increment_iteration();
        assert_eq!(ctx.iteration, 2);
    }

    #[test]
    fn test_set_end_reason() {
        let span_ctx = SpanContext::new("test-session");
        let mut ctx = LoopContext::new("session-123".to_string(), span_ctx);
        assert!(ctx.end_reason.is_none());
        ctx.set_end_reason(SessionEndReason::Completed);
        assert_eq!(ctx.end_reason, Some(SessionEndReason::Completed));
        ctx.set_end_reason(SessionEndReason::MaxIterationsReached);
        assert_eq!(
            ctx.end_reason,
            Some(SessionEndReason::MaxIterationsReached)
        );
    }

    #[test]
    fn test_add_tool_result() {
        let span_ctx = SpanContext::new("test-session");
        let mut ctx = LoopContext::new("session-123".to_string(), span_ctx);
        assert!(ctx.recent_tool_results.is_empty());
        ctx.add_tool_result(
            "read_file".to_string(),
            "call-1".to_string(),
            "ok".to_string(),
            true,
        );
        assert_eq!(ctx.recent_tool_results.len(), 1);
        assert_eq!(ctx.recent_tool_results[0].0, "read_file");
        assert_eq!(ctx.recent_tool_results[0].1, "ok");
        assert!(ctx.recent_tool_results[0].2);
        // The successful result must be appended to ctx.messages as a
        // Role::Tool message carrying the matching tool_call_id so the
        // next LLM call sees it. The error case below must NOT add a
        // Role::Tool message (errors are handled by the recovery cascade
        // via re-prompting, not by surfacing internal error text to the
        // LLM as a tool result).
        assert_eq!(ctx.messages.len(), 1);
        assert_eq!(ctx.messages[0].role, synthia_provider::Role::Tool);
        assert_eq!(ctx.messages[0].tool_call_id.as_deref(), Some("call-1"));
        ctx.add_tool_result(
            "bash".to_string(),
            "call-2".to_string(),
            "error".to_string(),
            false,
        );
        assert_eq!(ctx.recent_tool_results.len(), 2);
        assert_eq!(ctx.recent_tool_results[1].0, "bash");
        assert!(!ctx.recent_tool_results[1].2);
        // The error case still has 1 message (no new Role::Tool pushed).
        assert_eq!(ctx.messages.len(), 1);
    }

    #[test]
    fn test_token_ratio_without_limit_is_zero() {
        let span_ctx = SpanContext::new("test-session");
        let mut ctx = LoopContext::new("session".to_string(), span_ctx);
        for _ in 0..5 {
            ctx.messages.push(Message::user("hello world"));
        }
        assert_eq!(ctx.token_ratio(), 0.0);
    }

    #[test]
    fn test_token_ratio_with_limit() {
        let span_ctx = SpanContext::new("test-session");
        let mut ctx = LoopContext::new("session".to_string(), span_ctx);
        for _ in 0..100 {
            ctx.messages.push(Message::user(
                "the quick brown fox jumps over the lazy dog",
            ));
        }
        // Hard limit chosen so that the resulting ratio lands in (0, 1).
        let hard_limit = ctx
            .messages
            .iter()
            .map(estimate_message_tokens)
            .sum::<usize>()
            * 2;
        let ctx = ctx.with_token_limit(hard_limit);
        let ratio = ctx.token_ratio();
        assert!(ratio > 0.0 && ratio < 1.0, "ratio was {ratio}");
        assert!((ratio - 0.5).abs() < 0.05);
    }

    #[test]
    fn test_token_ratio_with_zero_limit_is_zero() {
        let span_ctx = SpanContext::new("test-session");
        let mut ctx = LoopContext::new("session".to_string(), span_ctx);
        ctx.messages.push(Message::user("hello"));
        let ctx = ctx.with_token_limit(0);
        assert_eq!(ctx.token_ratio(), 0.0);
    }

    #[test]
    fn test_loop_context_default_current_turn_id_is_none() {
        let span_ctx = SpanContext::new("test-session");
        let ctx = LoopContext::new("session".to_string(), span_ctx);
        assert!(ctx.current_turn_id.is_none());
    }

    #[test]
    fn test_loop_context_assign_new_turn_id() {
        let span_ctx = SpanContext::new("test-session");
        let mut ctx = LoopContext::new("session".to_string(), span_ctx);
        let id = ctx.assign_new_turn_id();
        assert_eq!(ctx.current_turn_id, Some(id));
    }

    #[test]
    fn test_loop_context_with_messages_preserves_turn_id() {
        let span_ctx = SpanContext::new("test-session");
        let mut ctx = LoopContext::new("session".to_string(), span_ctx);
        let id = ctx.assign_new_turn_id();
        let ctx = ctx.with_messages(vec![Message::user("hello")]);
        assert_eq!(ctx.current_turn_id, Some(id));
        assert_eq!(ctx.messages.len(), 1);
    }

    #[test]
    fn test_should_stop_with_timeout_expired() {
        let span_ctx = SpanContext::new("test-session");
        let mut ctx = LoopContext::new("session".to_string(), span_ctx);
        ctx.iteration = 1;
        // session_start 设为 100s 前，超时阈值 50s → 已超时。
        ctx.session_start =
            Instant::now().checked_sub(Duration::from_secs(100));
        assert!(
            ctx.should_stop_with_timeout(100, Some(Duration::from_secs(50)))
        );
    }

    #[test]
    fn test_should_stop_with_timeout_none_only_checks_iteration() {
        let span_ctx = SpanContext::new("test-session");
        let mut ctx = LoopContext::new("session".to_string(), span_ctx);
        ctx.session_start =
            Instant::now().checked_sub(Duration::from_secs(100));
        // iteration 未达上限，timeout=None → 不停止。
        assert!(!ctx.should_stop_with_timeout(100, None));
        // iteration 达上限 → 停止。
        ctx.iteration = 100;
        assert!(ctx.should_stop_with_timeout(100, None));
    }

    #[test]
    fn test_should_stop_with_timeout_zero_is_disabled() {
        let span_ctx = SpanContext::new("test-session");
        let mut ctx = LoopContext::new("session".to_string(), span_ctx);
        ctx.session_start =
            Instant::now().checked_sub(Duration::from_secs(100));
        // timeout=ZERO → 视为禁用，iteration 未达上限 → 不停止。
        assert!(!ctx.should_stop_with_timeout(100, Some(Duration::ZERO)));
    }

    #[test]
    fn test_should_stop_delegates_without_timeout() {
        let span_ctx = SpanContext::new("test-session");
        let mut ctx = LoopContext::new("session".to_string(), span_ctx);
        ctx.session_start =
            Instant::now().checked_sub(Duration::from_secs(100));
        // should_stop 不检查时间，只检查 iteration 和 end_reason。
        ctx.iteration = 1;
        assert!(!ctx.should_stop(100));
        ctx.iteration = 100;
        assert!(ctx.should_stop(100));
    }

    #[test]
    fn test_session_start_set_on_new() {
        let span_ctx = SpanContext::new("test-session");
        let ctx = LoopContext::new("session".to_string(), span_ctx);
        assert!(ctx.session_start.is_some());
        // 刚创建的 session_start 应该很近（不超过 5s）。
        let elapsed = ctx.session_start.unwrap().elapsed();
        assert!(elapsed < Duration::from_secs(5));
    }

    // ---- H4 regression tests ----
    //
    // Verifies that `LoopContext::from_metadata` restores ALL four
    // resumable fields (iteration / end_reason / cumulative_tokens /
    // context_token_limit). Before the H4 fix, main_loop.rs only
    // restored the last two inline, leaving iteration=0 and
    // end_reason=None — meaning a resumed session that had hit
    // `max_iterations` would silently restart iteration counting
    // and run forever.

    fn make_metadata_with_iteration(
        iteration: usize,
    ) -> synthia_session::SessionMetadata {
        synthia_session::SessionMetadata {
            version: 1,
            id: "test-session".to_string(),
            owner_user_id: "test-user".to_string(),
            state: synthia_session::types::SessionState::Initializing,
            token_usage: Default::default(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
            config: Default::default(),
            message_count: 0,
            end_reason: None,
            iteration,
            cumulative_tokens: 0,
            context_token_limit: None,
            title: None,
            controller_version: 1,
            parent_id: None,
        }
    }

    #[test]
    fn test_from_metadata_restores_iteration() {
        let metadata = make_metadata_with_iteration(50);
        let span_ctx = SpanContext::new("s1");
        let ctx =
            LoopContext::from_metadata("s1".to_string(), span_ctx, &metadata);
        assert_eq!(ctx.iteration, 50);
    }

    #[test]
    fn test_from_metadata_restores_cumulative_tokens() {
        let mut metadata = make_metadata_with_iteration(0);
        metadata.cumulative_tokens = 12_345;
        let span_ctx = SpanContext::new("s1");
        let ctx =
            LoopContext::from_metadata("s1".to_string(), span_ctx, &metadata);
        assert_eq!(ctx.cumulative_tokens, 12_345);
    }

    #[test]
    fn test_from_metadata_restores_context_token_limit() {
        let mut metadata = make_metadata_with_iteration(0);
        metadata.context_token_limit = Some(200_000);
        let span_ctx = SpanContext::new("s1");
        let ctx =
            LoopContext::from_metadata("s1".to_string(), span_ctx, &metadata);
        assert_eq!(ctx.context_token_limit, Some(200_000));
    }

    #[test]
    fn test_from_metadata_restores_end_reason() {
        let mut metadata = make_metadata_with_iteration(0);
        metadata.end_reason = Some(
            serde_json::to_string(&SessionEndReason::MaxIterationsReached)
                .unwrap(),
        );
        let span_ctx = SpanContext::new("s1");
        let ctx =
            LoopContext::from_metadata("s1".to_string(), span_ctx, &metadata);
        assert!(matches!(
            ctx.end_reason,
            Some(SessionEndReason::MaxIterationsReached)
        ));
    }

    #[test]
    fn test_from_metadata_end_reason_none_when_field_absent() {
        let metadata = make_metadata_with_iteration(0);
        let span_ctx = SpanContext::new("s1");
        let ctx =
            LoopContext::from_metadata("s1".to_string(), span_ctx, &metadata);
        assert!(ctx.end_reason.is_none());
    }

    #[test]
    fn test_from_metadata_starts_with_empty_messages() {
        // `from_metadata` initializes messages to an empty Vec so
        // `seed_initial_messages` can populate them afterward
        // without being clobbered.
        let metadata = make_metadata_with_iteration(50);
        let span_ctx = SpanContext::new("s1");
        let ctx =
            LoopContext::from_metadata("s1".to_string(), span_ctx, &metadata);
        assert!(ctx.messages.is_empty());
    }

    #[test]
    fn test_from_metadata_max_iterations_resume_stops_immediately() {
        // H4 regression: metadata.iteration=50 + max_iterations=50
        // → should_stop_with_timeout returns true immediately,
        // preventing infinite loop on resume.
        let metadata = make_metadata_with_iteration(50);
        let span_ctx = SpanContext::new("s1");
        let ctx =
            LoopContext::from_metadata("s1".to_string(), span_ctx, &metadata);
        assert!(ctx.should_stop_with_timeout(50, None));
    }

    #[test]
    fn test_from_metadata_below_max_iterations_resumes() {
        // Sanity: iteration=49 + max_iterations=50 should NOT stop
        // (one more iteration allowed).
        let metadata = make_metadata_with_iteration(49);
        let span_ctx = SpanContext::new("s1");
        let ctx =
            LoopContext::from_metadata("s1".to_string(), span_ctx, &metadata);
        assert!(!ctx.should_stop_with_timeout(50, None));
    }
}
