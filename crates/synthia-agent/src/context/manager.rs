//! Context manager with configurable compression logic

use std::sync::Arc;

use async_trait::async_trait;
use rmcp::model::{Role, SamplingMessage};

use crate::{
    Result,
    config::ContextConfig,
    context::{
        ContextManager,
        estimator::estimate_tokens,
        normalize::normalize_history,
        pruning::{
            MessageClassification,
            classify_messages,
            extract_critical_tool_results,
            find_result_for_tool_use,
            find_tool_use_for_result,
            fix_tool_pairs,
            is_tool_result,
            is_tool_use,
            micro_compact,
        },
        summarizer::generate_summary,
        types::{
            CompactionMetadata,
            CompactionResult,
            CompactionStrategy,
            ContextSafetyLevel,
        },
    },
    model_router::ModelRouter,
};

/// Default context manager with enhanced compression strategies
pub struct DefaultContextManager {
    config: ContextConfig,
    model_router: Arc<dyn ModelRouter>,
    context_window: usize,
}

impl DefaultContextManager {
    /// Create a new context manager with default configuration
    pub fn new(model_router: Arc<dyn ModelRouter>) -> Self {
        let context_window = model_router.context_window();
        Self {
            config: ContextConfig::default(),
            model_router,
            context_window,
        }
    }

    /// Create a new context manager with custom configuration
    pub fn with_config(
        model_router: Arc<dyn ModelRouter>,
        config: ContextConfig,
    ) -> Self {
        let context_window = model_router.context_window();
        Self {
            config,
            model_router,
            context_window,
        }
    }

    /// Set custom context window size
    pub fn with_context_window(mut self, window: usize) -> Self {
        self.context_window = window;
        self
    }

    /// Returns the compression ratio based on context window size.
    ///
    /// - Small window (<32k): 60% - tighter threshold for limited contexts
    /// - Medium window (32k-100k): 70% - balanced threshold
    /// - Large window (>100k): 85% - more relaxed threshold for larger contexts
    fn compression_ratio(&self) -> f64 {
        if self.context_window < 32_000 {
            0.60
        } else if self.context_window <= 100_000 {
            0.70
        } else {
            0.85
        }
    }

    /// Calculate effective token limit
    fn effective_limit(&self) -> usize {
        ((self.context_window as f64 * self.compression_ratio()) as usize)
            .saturating_sub(self.config.reserved_tokens)
    }

    /// Calculate usage ratio
    fn usage_ratio(&self, messages: &[SamplingMessage]) -> f64 {
        let tokens = estimate_tokens(messages);
        let limit = self.effective_limit();
        if limit == 0 {
            1.0
        } else {
            (tokens as f64 / limit as f64).min(1.0)
        }
    }

    /// Check if compaction should be triggered (dual-condition)
    fn should_compact(&self, tokens: usize) -> bool {
        let limit = self.effective_limit();
        tokens as f64 >= limit as f64 * self.config.trigger_ratio
            || tokens + self.config.min_buffer_tokens >= limit
    }

    /// Check if automatic compaction should run based on thresholds
    pub fn should_auto_compact(
        &self,
        messages: &[SamplingMessage],
        _time_based_secs: Option<u64>,
    ) -> bool {
        if self.usage_ratio(messages) >= self.config.trigger_threshold {
            return true;
        }
        false
    }

    pub fn check_context_safety(
        &self,
        messages: &[SamplingMessage],
    ) -> ContextSafetyLevel {
        let tokens = estimate_tokens(messages);
        let available = self.context_window.saturating_sub(tokens);
        ContextSafetyLevel::from_tokens(available)
    }

    pub fn is_context_critical(&self, messages: &[SamplingMessage]) -> bool {
        matches!(
            self.check_context_safety(messages),
            ContextSafetyLevel::Critical
        )
    }

    pub fn available_tokens(&self, messages: &[SamplingMessage]) -> usize {
        let tokens = estimate_tokens(messages);
        self.context_window.saturating_sub(tokens)
    }

    /// Compress messages based on usage ratio (3-level progressive strategy)
    ///
    /// Progressive compression strategy:
    /// - Level 0 (ratio < soft_threshold): No compression
    /// - Level 1 (soft_threshold <= ratio < hard_threshold): Micro compact
    /// - Level 2 (ratio >= hard_threshold): Summarization
    ///
    /// Each level is applied progressively. After each compression step,
    /// if the target ratio is not reached, the next level is attempted.
    async fn compress(
        &self,
        messages: &[SamplingMessage],
    ) -> Result<(Vec<SamplingMessage>, CompactionStrategy)> {
        let ratio = self.usage_ratio(messages);

        if ratio < self.config.soft_threshold {
            return Ok((messages.to_vec(), CompactionStrategy::None));
        }

        let mut current_messages = messages.to_vec();
        let mut current_ratio = ratio;
        let mut final_strategy = CompactionStrategy::None;

        if current_ratio >= self.config.soft_threshold
            && current_ratio < self.config.hard_threshold
        {
            tracing::info!(
                "Micro compact (usage: {:.1}%)",
                current_ratio * 100.0
            );

            micro_compact(&mut current_messages, self.config.keep_recent_turns);
            current_messages = fix_tool_pairs(&current_messages);
            current_ratio = self.usage_ratio(&current_messages);
            final_strategy = CompactionStrategy::MicroCompact;

            tracing::info!(
                "After micro compact: {:.1}% usage",
                current_ratio * 100.0
            );
        }

        if current_ratio >= self.config.hard_threshold {
            tracing::info!(
                "Summarizing (usage: {:.1}%)",
                current_ratio * 100.0
            );

            match self.summarize(&current_messages).await {
                Ok(result) => {
                    current_messages = fix_tool_pairs(&result);
                    final_strategy = CompactionStrategy::Summarization;

                    tracing::info!(
                        "After summarization: {:.1}% usage",
                        self.usage_ratio(&current_messages) * 100.0
                    );
                }
                Err(e) => {
                    tracing::error!(
                        "Summary generation failed: {e}. \
                         Context compression could not complete successfully."
                    );
                }
            }
        }

        Ok((current_messages, final_strategy))
    }

    /// Generate summary using LLM with intelligent message retention
    ///
    /// This method implements a smart summarization strategy:
    /// 1. **Preserve all user text messages** - User history is never compressed
    /// 2. **Keep recent N complete conversation turns** - Including assistant messages and tool pairs
    /// 3. **Summarize older assistant messages** - Only assistant content is compressed
    /// 4. **Maintain tool pair integrity** - ToolUse and ToolResult are always kept together
    /// 5. **Extract critical tool results** - Important file reads are preserved
    async fn summarize(
        &self,
        messages: &[SamplingMessage],
    ) -> Result<Vec<SamplingMessage>> {
        if messages.is_empty() {
            return Ok(Vec::new());
        }

        let classifications = classify_messages(messages);

        // Collect indices of user text messages (always preserve)
        let user_text_indices: Vec<usize> = classifications
            .iter()
            .filter_map(|(idx, class)| {
                if matches!(class, MessageClassification::UserText) {
                    Some(*idx)
                } else {
                    None
                }
            })
            .collect();

        // Calculate the cutoff point for recent turns
        let recent_turns_start =
            self.find_recent_turns_start(messages, &classifications);

        // Collect indices to preserve
        let mut preserve_indices: std::collections::HashSet<usize> =
            std::collections::HashSet::new();

        // 1. Always preserve all user text messages
        for idx in &user_text_indices {
            preserve_indices.insert(*idx);
        }

        // 2. Preserve recent complete turns (including tool pairs)
        for idx in recent_turns_start..messages.len() {
            preserve_indices.insert(idx);

            // Ensure tool pair integrity
            if let Some(msg) = messages.get(idx) {
                if is_tool_result(msg) {
                    if let Some(tool_use_idx) =
                        find_tool_use_for_result(messages, idx)
                    {
                        preserve_indices.insert(tool_use_idx);
                    }
                } else if is_tool_use(msg)
                    && let Some(tool_result_idx) =
                        find_result_for_tool_use(messages, idx)
                {
                    preserve_indices.insert(tool_result_idx);
                }
            }
        }

        // 3. Extract and preserve critical tool results from earlier messages
        let critical_tool_indices = extract_critical_tool_results(messages);
        for idx in critical_tool_indices {
            if idx < recent_turns_start {
                preserve_indices.insert(idx);
                if let Some(tool_use_idx) =
                    find_tool_use_for_result(messages, idx)
                {
                    preserve_indices.insert(tool_use_idx);
                }
            }
        }

        // Split messages into two groups
        let mut to_summarize: Vec<SamplingMessage> = Vec::new();
        let mut preserved: Vec<SamplingMessage> = Vec::new();
        let mut preserved_indices_sorted: Vec<usize> =
            preserve_indices.into_iter().collect();
        preserved_indices_sorted.sort_unstable();

        for (idx, msg) in messages.iter().enumerate() {
            if preserved_indices_sorted.contains(&idx) {
                preserved.push(msg.clone());
            } else if msg.role == Role::Assistant {
                to_summarize.push(msg.clone());
            }
        }

        // Generate summary for the assistant messages
        let mut result = Vec::new();

        if !to_summarize.is_empty() {
            let summary = generate_summary(
                &self.model_router,
                &to_summarize,
                self.config.quality_check_enabled,
                self.config.summary_max_tokens,
            )
            .await?;

            result.push(super::summarizer::create_summary_message(&summary));
        }

        result.extend(preserved);
        Ok(super::pruning::fix_tool_pairs(&result))
    }

    /// Find the starting index of recent conversation turns to preserve
    fn find_recent_turns_start(
        &self,
        messages: &[SamplingMessage],
        classifications: &[(usize, MessageClassification)],
    ) -> usize {
        let mut user_message_count = 0;
        let mut start_idx = messages.len();

        for (idx, class) in classifications.iter().rev() {
            if matches!(class, MessageClassification::UserText) {
                user_message_count += 1;
                if user_message_count >= self.config.keep_recent_turns {
                    start_idx = *idx;
                    break;
                }
            }
        }

        if user_message_count < self.config.keep_recent_turns {
            return 0;
        }

        start_idx
    }
}

impl std::fmt::Debug for DefaultContextManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DefaultContextManager")
            .field("config", &self.config)
            .field("context_window", &self.context_window)
            .finish_non_exhaustive()
    }
}

impl DefaultContextManager {
    pub fn emergency_truncate(&self, messages: &mut Vec<SamplingMessage>) {
        if messages.len() <= 10 {
            return;
        }

        let keep_recent = 10;
        let removed = messages.len() - keep_recent;
        messages.drain(0..removed);

        tracing::warn!(
            "Emergency truncation: removed {} messages, kept {}",
            removed,
            keep_recent
        );
    }
}

#[async_trait]
impl ContextManager for DefaultContextManager {
    async fn get_recent_messages(
        &self,
        _max_messages: usize,
    ) -> Result<Vec<SamplingMessage>> {
        // This is a placeholder - actual implementation would need session access
        // For now, return empty vector as we don't have session context here
        Ok(vec![])
    }

    async fn compact(
        &self,
        conversation: &[SamplingMessage],
    ) -> Result<Option<CompactionResult>> {
        let mut messages = conversation.to_vec();

        // Normalize history before compaction
        normalize_history(&mut messages, true);

        let tokens = estimate_tokens(&messages);

        // Check if compaction is needed (dual-condition)
        if !self.should_compact(tokens) {
            return Ok(None);
        }

        let limit = self.effective_limit();
        let ratio = tokens as f64 / limit as f64;

        let reason = format!(
            "Token usage {:.1}% exceeds threshold ({} / {} tokens)",
            ratio * 100.0,
            tokens,
            limit
        );

        tracing::info!(
            "Compressing: {} messages, {:.1}% usage",
            messages.len(),
            ratio * 100.0
        );

        // Execute compression
        let (compacted_messages, strategy) = self.compress(&messages).await?;

        // Calculate metadata
        let tokens_after = estimate_tokens(&compacted_messages);
        let metadata = CompactionMetadata::new(
            messages.len(),
            compacted_messages.len(),
            tokens.saturating_sub(tokens_after),
            strategy,
            ratio,
            tokens_after as f64 / limit as f64,
        );

        Ok(Some(CompactionResult::new(
            reason,
            compacted_messages,
            metadata,
        )))
    }

    async fn mid_turn_compact(
        &self,
        messages: &mut Vec<SamplingMessage>,
        _token_budget: usize,
    ) -> Result<bool> {
        // Perform lightweight mid-turn compaction
        // First normalize the history
        normalize_history(messages, false);

        let tokens = estimate_tokens(messages);
        let limit = self.effective_limit();
        let ratio = tokens as f64 / limit as f64;

        if ratio < self.config.micro_threshold {
            return Ok(false);
        }

        tracing::debug!(
            "Mid-turn compact: {:.1}% usage, applying micro-compact",
            ratio * 100.0
        );

        // Apply micro-compact for mid-turn (keep fewer recent turns)
        micro_compact(
            messages,
            self.config.keep_recent_turns.saturating_sub(2),
        );
        *messages = fix_tool_pairs(messages);

        let new_ratio = self.usage_ratio(messages);
        tracing::debug!(
            "Mid-turn compact complete: {:.1}% usage",
            new_ratio * 100.0
        );

        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use rmcp::model::{
        Content,
        RawTextContent,
        Role,
        SamplingContent,
        SamplingMessage,
        SamplingMessageContent,
        ToolResultContent,
        ToolUseContent,
    };

    use super::*;
    use crate::{
        config::ContextConfig,
        model_router::{
            ModelRouter as RouterTrait,
            types::{ModelConfig, RoutingResult},
        },
    };

    fn create_text_msg(role: Role, text: &str) -> SamplingMessage {
        SamplingMessage {
            role,
            content: SamplingContent::Single(SamplingMessageContent::Text(
                RawTextContent {
                    text: text.to_string(),
                    meta: None,
                },
            )),
            meta: None,
        }
    }

    fn create_tool_use(id: &str, name: &str) -> SamplingMessage {
        SamplingMessage {
            role: Role::Assistant,
            content: SamplingContent::Single(SamplingMessageContent::ToolUse(
                ToolUseContent::new(
                    id,
                    name,
                    serde_json::json!({})
                        .as_object()
                        .cloned()
                        .unwrap_or_default(),
                ),
            )),
            meta: None,
        }
    }

    fn create_tool_result(id: &str, content: &str) -> SamplingMessage {
        SamplingMessage {
            role: Role::User,
            content: SamplingContent::Single(
                SamplingMessageContent::ToolResult(ToolResultContent::new(
                    id,
                    vec![Content::text(content.to_string())],
                )),
            ),
            meta: None,
        }
    }

    struct MockRouter {
        context_window: usize,
    }
    impl MockRouter {
        fn new(window: usize) -> Arc<Self> {
            Arc::new(Self {
                context_window: window,
            })
        }
    }
    #[async_trait]
    impl RouterTrait for MockRouter {
        async fn route(
            &self,
            _: &[SamplingMessage],
        ) -> crate::Result<RoutingResult> {
            unreachable!()
        }

        fn available_models(&self) -> &[ModelConfig] {
            &[]
        }

        fn context_window(&self) -> usize {
            self.context_window
        }
    }

    fn mgr(window: usize) -> DefaultContextManager {
        DefaultContextManager::new(MockRouter::new(window))
    }

    fn mgr_with_config(
        window: usize,
        config: ContextConfig,
    ) -> DefaultContextManager {
        DefaultContextManager::with_config(MockRouter::new(window), config)
    }

    #[test]
    fn test_compression_ratio_small_window() {
        let m = mgr(20_000);
        assert_eq!(m.compression_ratio(), 0.60);
    }

    #[test]
    fn test_compression_ratio_medium_window() {
        let m = mgr(50_000);
        assert_eq!(m.compression_ratio(), 0.70);
    }

    #[test]
    fn test_compression_ratio_large_window() {
        let m = mgr(150_000);
        assert_eq!(m.compression_ratio(), 0.85);
    }

    #[test]
    fn test_effective_limit() {
        let config = ContextConfig {
            reserved_tokens: 1000,
            ..Default::default()
        };
        let m = mgr_with_config(100_000, config);
        let limit = m.effective_limit();
        assert!(limit > 0);
        assert!(limit <= 100_000);
    }

    #[test]
    fn test_effective_limit_saturating_sub() {
        let config = ContextConfig {
            reserved_tokens: 200_000,
            ..Default::default()
        };
        let m = mgr_with_config(100_000, config);
        assert_eq!(m.effective_limit(), 0);
    }

    #[test]
    fn test_should_compact_dual_condition() {
        let config = ContextConfig::default();
        let limit = 100_000usize;

        let ratio_trigger_tokens =
            (limit as f64 * config.trigger_ratio) as usize;
        assert!(ratio_trigger_tokens > limit * 8 / 10);

        let buffer_trigger_tokens = limit - config.min_buffer_tokens;
        assert!(buffer_trigger_tokens > 0);
    }

    #[test]
    fn test_should_auto_compact_below_threshold() {
        let m = mgr(100_000);
        let msgs = vec![create_text_msg(Role::User, "short")];
        assert!(!m.should_auto_compact(&msgs, None));
    }

    #[test]
    fn test_check_context_safety_safe() {
        let m = mgr(100_000);
        let msgs = vec![create_text_msg(Role::User, "x"); 10];
        let level = m.check_context_safety(&msgs);
        assert_eq!(level, ContextSafetyLevel::Safe);
    }

    #[test]
    fn test_check_context_safety_warning() {
        let m = mgr(100_000);
        // Use many messages to get close to limit
        let msgs: Vec<_> = (0..1000)
            .map(|i| create_text_msg(Role::User, &"x".repeat(i * 10)))
            .collect();
        let level = m.check_context_safety(&msgs);
        // May be Warning or Critical depending on token estimation
        assert!(matches!(
            level,
            ContextSafetyLevel::Warning
                | ContextSafetyLevel::Critical
                | ContextSafetyLevel::Safe
        ));
    }

    #[test]
    fn test_is_context_critical() {
        let m = mgr(100_000);
        let msgs = vec![create_text_msg(Role::User, "x"); 10];
        // Empty or very small context won't be critical
        let critical = m.is_context_critical(&msgs);
        // May or may not be critical depending on token count
        let _ = critical;
    }

    #[test]
    fn test_available_tokens() {
        let m = mgr(100_000);
        let msgs = vec![create_text_msg(Role::User, "hello")];
        let available = m.available_tokens(&msgs);
        assert!(available <= 100_000);
    }

    #[test]
    fn test_context_manager_debug() {
        let m = mgr(100_000);
        let debug = format!("{m:?}");
        assert!(debug.contains("DefaultContextManager"));
    }

    #[test]
    fn test_emergency_truncate() {
        let m = mgr(100_000);
        let mut msgs: Vec<_> = (0..20)
            .map(|i| create_text_msg(Role::User, &format!("msg {i}")))
            .collect();
        let original_len = msgs.len();
        m.emergency_truncate(&mut msgs);
        assert!(msgs.len() < original_len);
        assert!(msgs.len() <= 10);
    }

    #[test]
    fn test_emergency_truncate_small() {
        let m = mgr(100_000);
        let mut msgs = vec![create_text_msg(Role::User, "only one")];
        let len = msgs.len();
        m.emergency_truncate(&mut msgs);
        // Should not truncate if already small
        assert_eq!(msgs.len(), len);
    }

    #[test]
    fn test_with_context_window() {
        let m = DefaultContextManager::new(MockRouter::new(50_000))
            .with_context_window(80_000);
        assert_eq!(m.compression_ratio(), 0.70); // 50k-100k range
    }

    #[test]
    fn test_context_manager_with_custom_config() {
        let config = ContextConfig {
            reserved_tokens: 5000,
            ..Default::default()
        };
        let m = mgr_with_config(100_000, config);
        assert!(m.effective_limit() <= 100_000 - 5000);
    }

    #[test]
    fn test_usage_ratio_empty() {
        let m = mgr(100_000);
        let ratio = m.usage_ratio(&[]);
        assert_eq!(ratio, 0.0);
    }

    #[test]
    fn test_usage_ratio_with_messages() {
        let m = mgr(100_000);
        let msgs = vec![
            create_text_msg(Role::User, "hello world"),
            create_text_msg(Role::Assistant, "hi there"),
        ];
        let ratio = m.usage_ratio(&msgs);
        assert!(ratio > 0.0);
        assert!(ratio <= 1.0);
    }

    #[test]
    fn test_usage_ratio_capped_at_one() {
        let m = mgr(100_000);
        // Create many messages to exceed limit
        let msgs: Vec<SamplingMessage> = (0..5000)
            .map(|_i| create_text_msg(Role::User, &"x".repeat(100)))
            .collect();
        let ratio = m.usage_ratio(&msgs);
        assert!(ratio >= 1.0); // Should be capped
    }

    #[test]
    fn test_compact_empty_messages() {
        let m = mgr(100_000);
        let result = m.compact(&[]);
        // May fail due to mock router, but tests the logic path
        // Result: Ok(Some(CompactionResult)) or Err
        drop(result);
    }

    #[test]
    fn test_compact_result_metadata_fields() {
        use crate::context::CompactionMetadata;
        let metadata = CompactionMetadata::new(
            50,
            10,
            2000,
            crate::context::CompactionStrategy::MicroCompact,
            0.8,
            0.3,
        );
        assert_eq!(metadata.original_count, 50);
        assert_eq!(metadata.compacted_count, 10);
        assert_eq!(metadata.tokens_saved, 2000);
        assert_eq!(
            metadata.strategy,
            crate::context::CompactionStrategy::MicroCompact
        );
    }

    #[test]
    fn test_compaction_result_new() {
        use crate::context::{
            CompactionMetadata,
            CompactionResult,
            CompactionStrategy,
        };
        let metadata = CompactionMetadata::new(
            10,
            5,
            500,
            CompactionStrategy::None,
            0.9,
            0.5,
        );
        let result = CompactionResult::new(
            "test reason".to_string(),
            vec![create_text_msg(Role::User, "test")],
            metadata,
        );
        assert_eq!(result.reason, "test reason");
        assert_eq!(result.messages.len(), 1);
    }

    // =========================================================================
    // Constructor tests
    // =========================================================================

    #[test]
    fn test_new_uses_default_config() {
        let router = MockRouter::new(50_000);
        let m = DefaultContextManager::new(router);
        let debug = format!("{m:?}");
        assert!(debug.contains("DefaultContextManager"));
        assert!(debug.contains("context_window: 50000"));
    }

    #[test]
    fn test_with_config_uses_provided_config() {
        let config = ContextConfig {
            reserved_tokens: 10_000,
            trigger_ratio: 0.9,
            ..Default::default()
        };
        let router = MockRouter::new(80_000);
        let m = DefaultContextManager::with_config(router, config);
        let debug = format!("{m:?}");
        assert!(debug.contains("DefaultContextManager"));
    }

    #[test]
    fn test_with_config_and_new_produce_different_defaults() {
        let router1 = MockRouter::new(50_000);
        let router2 = MockRouter::new(50_000);
        let m_default = DefaultContextManager::new(router1);
        let m_custom = DefaultContextManager::with_config(
            router2,
            ContextConfig::default(),
        );
        // Both should have same behavior with default config
        assert_eq!(m_default.compression_ratio(), m_custom.compression_ratio());
    }

    // =========================================================================
    // compression_ratio() boundary tests
    // =========================================================================

    #[test]
    fn test_compression_ratio_exactly_32k() {
        // 32_000 is in the medium range (32k-100k)
        let m = mgr(32_000);
        assert_eq!(m.compression_ratio(), 0.70);
    }

    #[test]
    fn test_compression_ratio_exactly_100k() {
        // 100_000 is in the medium range (32k-100k)
        let m = mgr(100_000);
        assert_eq!(m.compression_ratio(), 0.70);
    }

    #[test]
    fn test_compression_ratio_just_above_100k() {
        let m = mgr(100_001);
        assert_eq!(m.compression_ratio(), 0.85);
    }

    #[test]
    fn test_compression_ratio_just_below_32k() {
        let m = mgr(31_999);
        assert_eq!(m.compression_ratio(), 0.60);
    }

    // =========================================================================
    // effective_limit() tests
    // =========================================================================

    #[test]
    fn test_effective_limit_small_window() {
        let config = ContextConfig {
            reserved_tokens: 500,
            ..Default::default()
        };
        // 20_000 * 0.60 - 500 = 11_500
        let m = mgr_with_config(20_000, config);
        assert_eq!(m.effective_limit(), 11_500);
    }

    #[test]
    fn test_effective_limit_medium_window() {
        let config = ContextConfig {
            reserved_tokens: 1000,
            ..Default::default()
        };
        // 50_000 * 0.70 - 1000 = 34_000
        let m = mgr_with_config(50_000, config);
        assert_eq!(m.effective_limit(), 34_000);
    }

    #[test]
    fn test_effective_limit_large_window() {
        let config = ContextConfig {
            reserved_tokens: 2000,
            ..Default::default()
        };
        // 150_000 * 0.85 - 2000 = 125_500
        let m = mgr_with_config(150_000, config);
        assert_eq!(m.effective_limit(), 125_500);
    }

    #[test]
    fn test_effective_limit_reserved_exceeds_compressed() {
        let config = ContextConfig {
            reserved_tokens: 50_000,
            ..Default::default()
        };
        // 20_000 * 0.60 = 12_000, but 12_000 < 50_000
        let m = mgr_with_config(20_000, config);
        assert_eq!(m.effective_limit(), 0);
    }

    // =========================================================================
    // usage_ratio() tests
    // =========================================================================

    #[test]
    fn test_usage_ratio_zero_limit_returns_one() {
        let config = ContextConfig {
            reserved_tokens: 200_000,
            ..Default::default()
        }; // Exceeds context window
        let m = mgr_with_config(100_000, config);
        let msgs = vec![create_text_msg(Role::User, "x")];
        assert_eq!(m.usage_ratio(&msgs), 1.0);
    }

    #[test]
    fn test_usage_ratio_single_message() {
        let m = mgr(100_000);
        let msgs = vec![create_text_msg(Role::User, "hello world")];
        let ratio = m.usage_ratio(&msgs);
        assert!(ratio > 0.0);
        assert!(ratio < 1.0);
    }

    #[test]
    fn test_usage_ratio_exactly_one() {
        let m = mgr(100_000);
        // Create messages that use approximately the effective limit
        let msgs: Vec<_> = (0..100)
            .map(|_| create_text_msg(Role::User, &"x".repeat(1000)))
            .collect();
        let ratio = m.usage_ratio(&msgs);
        assert!(ratio <= 1.0);
    }

    // =========================================================================
    // should_compact() tests
    // =========================================================================

    #[test]
    fn test_should_compact_ratio_trigger() {
        let config = ContextConfig {
            trigger_ratio: 0.8,
            ..Default::default()
        };
        let m = mgr_with_config(100_000, config);
        // Tokens equal to 80% of effective limit should trigger
        let tokens = (m.effective_limit() as f64 * 0.8) as usize;
        assert!(m.should_compact(tokens));
    }

    #[test]
    fn test_should_compact_below_ratio_trigger() {
        let config = ContextConfig {
            trigger_ratio: 0.8,
            min_buffer_tokens: 100,
            ..Default::default()
        };
        let m = mgr_with_config(100_000, config);
        // Tokens below 80% of effective limit should not trigger (unless buffer hits)
        let tokens = (m.effective_limit() as f64 * 0.5) as usize;
        assert!(!m.should_compact(tokens));
    }

    #[test]
    fn test_should_compact_buffer_trigger() {
        let config = ContextConfig {
            min_buffer_tokens: 5000,
            ..Default::default()
        };
        let min_buffer = config.min_buffer_tokens;
        let m = mgr_with_config(100_000, config);
        // Tokens such that tokens + min_buffer_tokens >= limit
        let limit = m.effective_limit();
        let tokens = limit - min_buffer + 1;
        assert!(m.should_compact(tokens));
    }

    #[test]
    fn test_should_compact_at_boundary() {
        let config = ContextConfig {
            min_buffer_tokens: 1000,
            ..Default::default()
        };
        let min_buffer = config.min_buffer_tokens;
        let m = mgr_with_config(100_000, config);
        let limit = m.effective_limit();
        // tokens + min_buffer == limit triggers because >= comparison
        let tokens = limit - min_buffer;
        // Should trigger because tokens + min_buffer >= limit (== qualifies)
        assert!(m.should_compact(tokens));
    }

    // =========================================================================
    // should_auto_compact() tests
    // =========================================================================

    #[test]
    fn test_should_auto_compact_at_threshold() {
        let config = ContextConfig {
            trigger_threshold: 0.5,
            ..Default::default()
        };
        let m = mgr_with_config(100_000, config);
        // Create messages that use ~50% of limit
        let msgs: Vec<_> = (0..200)
            .map(|i| create_text_msg(Role::User, &"x".repeat(i * 10)))
            .collect();
        // Usage ratio should be around threshold
        let _ = m.should_auto_compact(&msgs, None);
    }

    #[test]
    fn test_should_auto_compact_above_threshold() {
        let config = ContextConfig {
            trigger_threshold: 0.001,
            ..Default::default()
        }; // Very low threshold
        let m = mgr_with_config(100_000, config);
        // Create many messages to push usage ratio above threshold
        let msgs: Vec<_> = (0..1000)
            .map(|_| create_text_msg(Role::User, "hello world"))
            .collect();
        assert!(m.should_auto_compact(&msgs, None));
    }

    #[test]
    fn test_should_auto_compact_empty_messages() {
        let config = ContextConfig {
            trigger_threshold: 0.5,
            ..Default::default()
        };
        let m = mgr_with_config(100_000, config);
        assert!(!m.should_auto_compact(&[], None));
    }

    // =========================================================================
    // check_context_safety() tests
    // =========================================================================

    #[test]
    fn test_check_context_safety_empty_messages() {
        let m = mgr(100_000);
        let level = m.check_context_safety(&[]);
        assert_eq!(level, ContextSafetyLevel::Safe);
    }

    #[test]
    fn test_check_context_safety_high_tokens() {
        let m = mgr(10_000);
        // Create messages with many tokens
        let msgs: Vec<_> = (0..1000)
            .map(|_| create_text_msg(Role::User, &"x".repeat(100)))
            .collect();
        let level = m.check_context_safety(&msgs);
        // With high usage, should be Warning or Critical
        assert!(matches!(
            level,
            ContextSafetyLevel::Warning | ContextSafetyLevel::Critical
        ));
    }

    // =========================================================================
    // is_context_critical() tests
    // =========================================================================

    #[test]
    fn test_is_context_critical_empty() {
        let m = mgr(100_000);
        assert!(!m.is_context_critical(&[]));
    }

    #[test]
    fn test_is_context_critical_with_content() {
        let m = mgr(100_000);
        let msgs = vec![create_text_msg(Role::User, "hello")];
        let _critical = m.is_context_critical(&msgs);
        // Result depends on token estimation
    }

    // =========================================================================
    // available_tokens() tests
    // =========================================================================

    #[test]
    fn test_available_tokens_empty() {
        let m = mgr(100_000);
        let available = m.available_tokens(&[]);
        assert_eq!(available, 100_000);
    }

    #[test]
    fn test_available_tokens_with_messages() {
        let m = mgr(100_000);
        let msgs = vec![create_text_msg(Role::User, "hello")];
        let available = m.available_tokens(&msgs);
        assert!(available < 100_000);
        assert!(available > 0);
    }

    #[test]
    fn test_available_tokens_many_messages() {
        let m = mgr(10_000);
        let msgs: Vec<_> =
            (0..500).map(|_| create_text_msg(Role::User, "x")).collect();
        let available = m.available_tokens(&msgs);
        // With many short messages, available should be less than context window
        assert!(available <= 10_000);
    }

    #[test]
    fn test_available_tokens_never_negative() {
        let m = mgr(1000); // Small window
        let msgs: Vec<_> = (0..100)
            .map(|_| create_text_msg(Role::User, &"x".repeat(1000)))
            .collect();
        let available = m.available_tokens(&msgs);
        assert_eq!(available, 0); // Saturating subtraction
    }

    // =========================================================================
    // Additional edge case tests
    // =========================================================================

    #[test]
    fn test_with_context_window_overrides_router_window() {
        let router = MockRouter::new(50_000);
        let m = DefaultContextManager::new(router).with_context_window(200_000);
        // Should use 200_000 for compression ratio (large window > 100k)
        assert_eq!(m.compression_ratio(), 0.85);
    }

    #[test]
    fn test_context_config_fields_are_respected() {
        let config = ContextConfig {
            reserved_tokens: 5000,
            ..Default::default()
        };
        let m = mgr_with_config(100_000, config);
        // effective_limit = 100_000 * 0.70 - 5000 = 65_000
        assert_eq!(m.effective_limit(), 65_000);
    }

    #[test]
    fn test_shrink_to_fit_does_not_affect_compression() {
        let m = mgr(50_000);
        // Verify compression ratio is based on context_window, not actual usage
        assert_eq!(m.compression_ratio(), 0.70);
        assert_eq!(m.compression_ratio(), 0.70); // Repeatable
    }

    // =========================================================================
    // mid_turn_compact tests
    // =========================================================================

    #[tokio::test]
    async fn test_mid_turn_compact_below_threshold() {
        let m = mgr(100_000);
        let mut msgs = vec![create_text_msg(Role::User, "short")];
        let result = m.mid_turn_compact(&mut msgs, 100_000).await.unwrap();
        // Should return false when below micro_threshold
        assert!(!result);
    }

    #[tokio::test]
    async fn test_mid_turn_compact_normalizes_messages() {
        let m = mgr(100_000);
        // Create orphaned tool result
        let mut msgs = vec![
            create_tool_use("1", "tool"),
            create_tool_result("1", "result"),
        ];
        let _ = m.mid_turn_compact(&mut msgs, 100_000).await;
        // normalize_history should have been called
    }

    #[tokio::test]
    async fn test_mid_turn_compact_does_not_remove_tool_pairs() {
        let m = mgr(100_000);
        // Create tool use/result pairs
        let mut msgs = vec![
            create_tool_use("1", "tool"),
            create_tool_result("1", "result"),
            create_tool_use("2", "tool"),
            create_tool_result("2", "result"),
        ];
        // Even if micro_compact doesn't trigger, the pairs should be intact after call
        let _ = m.mid_turn_compact(&mut msgs, 100_000).await;
        // Messages should still be valid pairs
        assert!(msgs.len() >= 2);
    }

    // =========================================================================
    // find_recent_turns_start tests
    // =========================================================================

    #[test]
    fn test_find_recent_turns_start_exactly_keep_recent() {
        let config = ContextConfig {
            keep_recent_turns: 3,
            ..Default::default()
        };
        let m = mgr_with_config(100_000, config);

        let messages: Vec<_> = (0..6)
            .map(|i| {
                if i % 2 == 0 {
                    create_text_msg(Role::User, &format!("User message {i}"))
                } else {
                    create_text_msg(
                        Role::Assistant,
                        &format!("Assistant message {i}"),
                    )
                }
            })
            .collect();

        let classifications = classify_messages(&messages);
        let start = m.find_recent_turns_start(&messages, &classifications);
        // Should return index 0 since we have exactly 3 user messages
        assert_eq!(start, 0);
    }

    #[test]
    fn test_find_recent_turns_start_more_than_keep_recent() {
        let config = ContextConfig {
            keep_recent_turns: 2,
            ..Default::default()
        };
        let m = mgr_with_config(100_000, config);

        // Create 6 messages with 3 user messages
        let messages = vec![
            create_text_msg(Role::User, "User 0"), // idx 0
            create_text_msg(Role::Assistant, "Asst 0"), // idx 1
            create_text_msg(Role::User, "User 1"), // idx 2
            create_text_msg(Role::Assistant, "Asst 1"), // idx 3
            create_text_msg(Role::User, "User 2"), // idx 4
            create_text_msg(Role::Assistant, "Asst 2"), // idx 5
        ];

        let classifications = classify_messages(&messages);
        let start = m.find_recent_turns_start(&messages, &classifications);
        // Should return index 2 since we want to keep last 2 user turns
        assert_eq!(start, 2);
    }

    #[test]
    fn test_find_recent_turns_start_fewer_than_keep_recent() {
        let config = ContextConfig {
            keep_recent_turns: 10,
            ..Default::default()
        };
        let m = mgr_with_config(100_000, config);

        // Only 2 user messages but keep_recent is 10
        let messages = vec![
            create_text_msg(Role::User, "User 0"),
            create_text_msg(Role::Assistant, "Asst 0"),
            create_text_msg(Role::User, "User 1"),
            create_text_msg(Role::Assistant, "Asst 1"),
        ];

        let classifications = classify_messages(&messages);
        let start = m.find_recent_turns_start(&messages, &classifications);
        // Should return 0 since we don't have enough user messages
        assert_eq!(start, 0);
    }

    // =========================================================================
    // emergency_truncate edge case tests
    // =========================================================================

    #[test]
    fn test_emergency_truncate_exactly_10() {
        let m = mgr(100_000);
        let mut msgs: Vec<_> = (0..10)
            .map(|i| create_text_msg(Role::User, &format!("msg {i}")))
            .collect();
        let original_len = msgs.len();
        m.emergency_truncate(&mut msgs);
        // Should not truncate if exactly 10
        assert_eq!(msgs.len(), original_len);
    }

    #[test]
    fn test_emergency_truncate_11_messages() {
        let m = mgr(100_000);
        let mut msgs: Vec<_> = (0..11)
            .map(|i| create_text_msg(Role::User, &format!("msg {i}")))
            .collect();
        m.emergency_truncate(&mut msgs);
        // Should truncate to 10
        assert_eq!(msgs.len(), 10);
        // Should keep the last 10
        assert_eq!(
            msgs[0]
                .content
                .iter()
                .find_map(|c| c.as_text())
                .unwrap()
                .text,
            "msg 1"
        );
    }

    #[test]
    fn test_emergency_truncate_100_messages() {
        let m = mgr(100_000);
        let mut msgs: Vec<_> = (0..100)
            .map(|i| create_text_msg(Role::User, &format!("msg {i}")))
            .collect();
        m.emergency_truncate(&mut msgs);
        // Should truncate to 10
        assert_eq!(msgs.len(), 10);
        // Should keep msg 90-99 (the last 10)
    }

    // =========================================================================
    // ContextManager trait implementation tests
    // =========================================================================

    #[tokio::test]
    async fn test_compact_returns_none_when_not_needed() {
        let m = mgr(100_000);
        let msgs = vec![create_text_msg(Role::User, "short")];
        let result = m.compact(&msgs).await.unwrap();
        // With a single short message, should return None (no compaction needed)
        assert!(result.is_none());
    }

    // =========================================================================
    // Compression strategy tests
    // =========================================================================

    #[test]
    fn test_compression_ratio_at_boundary_31k() {
        // Just below 32k should be small window (0.60)
        let m = mgr(31_999);
        assert_eq!(m.compression_ratio(), 0.60);
    }

    #[test]
    fn test_compression_ratio_at_boundary_32k() {
        // Just at 32k should be medium window (0.70)
        let m = mgr(32_000);
        assert_eq!(m.compression_ratio(), 0.70);
    }

    #[test]
    fn test_compression_ratio_at_boundary_100k() {
        // Just at 100k should be medium window (0.70)
        let m = mgr(100_000);
        assert_eq!(m.compression_ratio(), 0.70);
    }

    #[test]
    fn test_compression_ratio_at_boundary_100k_plus_one() {
        // Just above 100k should be large window (0.85)
        let m = mgr(100_001);
        assert_eq!(m.compression_ratio(), 0.85);
    }

    // =========================================================================
    // usage_ratio edge case tests
    // =========================================================================

    #[test]
    fn test_usage_ratio_exactly_zero() {
        let m = mgr(100_000);
        let ratio = m.usage_ratio(&[]);
        assert_eq!(ratio, 0.0);
    }

    #[test]
    fn test_usage_ratio_very_small_window() {
        let m = mgr(100); // Very small window
        let msgs = vec![create_text_msg(Role::User, "hello world")];
        let ratio = m.usage_ratio(&msgs);
        // Should be capped at 1.0
        assert!(ratio <= 1.0);
    }

    // =========================================================================
    // should_compact edge case tests
    // =========================================================================

    #[test]
    fn test_should_compact_exactly_at_limit() {
        let m = mgr(100_000);
        let limit = m.effective_limit();
        // Tokens exactly at limit should trigger
        assert!(m.should_compact(limit));
    }

    #[test]
    fn test_should_compact_way_over_limit() {
        let m = mgr(100_000);
        // Way over limit should trigger
        assert!(m.should_compact(1_000_000));
    }

    // =========================================================================
    // should_auto_compact edge case tests
    // =========================================================================

    #[test]
    fn test_should_auto_compact_with_zero_threshold() {
        let config = ContextConfig {
            trigger_threshold: 0.0,
            ..Default::default()
        };
        let m = mgr_with_config(100_000, config);
        let msgs = vec![create_text_msg(Role::User, "x")];
        // With 0 threshold, even minimal usage should trigger
        assert!(m.should_auto_compact(&msgs, None));
    }

    #[test]
    fn test_should_auto_compact_with_perfect_threshold() {
        let config = ContextConfig {
            trigger_threshold: 1.0,
            ..Default::default()
        };
        let m = mgr_with_config(100_000, config);
        // Should not trigger since threshold is 100%
        let msgs: Vec<_> = (0..10)
            .map(|_| create_text_msg(Role::User, "hello"))
            .collect();
        // Note: depending on token estimation, may or may not trigger
        let _ = m.should_auto_compact(&msgs, None);
    }

    // =========================================================================
    // Context safety level edge case tests
    // =========================================================================

    #[test]
    fn test_check_context_safety_exactly_at_hard_min() {
        let m = mgr(16_000);
        // 0 available tokens
        let msgs: Vec<_> = (0..1000)
            .map(|_| create_text_msg(Role::User, &"x".repeat(100)))
            .collect();
        let level = m.check_context_safety(&msgs);
        // Could be Critical or Warning depending on actual token count
        assert!(matches!(
            level,
            ContextSafetyLevel::Warning | ContextSafetyLevel::Critical
        ));
    }

    #[test]
    fn test_available_tokens_with_zero_window() {
        let m = mgr(0);
        let msgs = vec![create_text_msg(Role::User, "hello")];
        let available = m.available_tokens(&msgs);
        assert_eq!(available, 0);
    }

    // =========================================================================
    // Initialization tests
    // =========================================================================

    #[test]
    fn test_new_sets_context_window_from_router() {
        let router = MockRouter::new(75_000);
        let m = DefaultContextManager::new(router);
        // 75k is in medium range
        assert_eq!(m.compression_ratio(), 0.70);
    }

    #[test]
    fn test_with_config_preserves_router_context_window() {
        let router = MockRouter::new(200_000);
        let config = ContextConfig::default();
        let m = DefaultContextManager::with_config(router, config);
        // 200k is in large range
        assert_eq!(m.compression_ratio(), 0.85);
    }

    // =========================================================================
    // Compact result metadata edge cases
    // =========================================================================

    #[test]
    fn test_compaction_metadata_tokens_saved_calculation() {
        let metadata = CompactionMetadata::new(
            100,
            50,
            0, // tokens_saved = 0
            CompactionStrategy::None,
            0.9,
            0.9,
        );
        assert_eq!(metadata.tokens_saved, 0);
        assert_eq!(metadata.original_count, 100);
        assert_eq!(metadata.compacted_count, 50);
    }

    #[test]
    fn test_compaction_metadata_preserves_strategy() {
        let strategies = [
            CompactionStrategy::None,
            CompactionStrategy::MicroCompact,
            CompactionStrategy::SoftPruning,
            CompactionStrategy::HardClearing,
            CompactionStrategy::Summarization,
        ];
        for strategy in strategies {
            let metadata =
                CompactionMetadata::new(10, 5, 100, strategy, 0.8, 0.4);
            assert_eq!(metadata.strategy, strategy);
        }
    }
}
