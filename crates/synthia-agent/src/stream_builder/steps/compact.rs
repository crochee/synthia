use synthia_provider::estimate_messages_token_count;
use synthia_session::types::TokenBudgetStatus;

use crate::{compaction, config::AgentConfig, loop_context::LoopContext};

pub struct StepCompact;

impl StepCompact {
    pub fn check(
        &self,
        ctx: &LoopContext,
        config: &AgentConfig,
    ) -> CompactAction {
        let Some(budget) = &config.context_token_budget else {
            return CompactAction::None;
        };

        let token_count = estimate_messages_token_count(&ctx.messages);
        let status = budget.check(token_count);

        match status {
            TokenBudgetStatus::MustCompact => CompactAction::MustCompact,
            TokenBudgetStatus::Warning => CompactAction::Warning,
            _ => CompactAction::None,
        }
    }

    pub fn execute(
        &self,
        ctx: &mut LoopContext,
        config: &AgentConfig,
    ) -> Option<CompactionResult> {
        let Some(budget) = &config.context_token_budget else {
            return None;
        };

        let _old_tokens = estimate_messages_token_count(&ctx.messages);
        let result = compaction::try_compact(&mut ctx.messages, budget);
        if let Some(ref _r) = result {
            ctx.needs_compact = false;
        }
        result.map(|r| CompactionResult {
            old_tokens: r.old_tokens,
            new_tokens: r.new_tokens,
            implementation: r.implementation,
            phase: r.phase,
            messages_compacted: r.messages_compacted,
        })
    }

    /// Execute compaction against explicit thresholds.
    ///
    /// Used by the 80% auto-trigger fallback in the main loop, which
    /// bypasses the configured `context_token_budget` thresholds.
    pub fn execute_with_threshold(
        &self,
        ctx: &mut LoopContext,
        hard_limit: usize,
        soft_limit: usize,
    ) -> Option<CompactionResult> {
        let result = compaction::try_compact_with_threshold(
            &mut ctx.messages,
            hard_limit,
            soft_limit,
        );
        if result.is_some() {
            ctx.needs_compact = false;
        }
        result.map(|r| CompactionResult {
            old_tokens: r.old_tokens,
            new_tokens: r.new_tokens,
            implementation: r.implementation,
            phase: r.phase,
            messages_compacted: r.messages_compacted,
        })
    }
}

pub enum CompactAction {
    None,
    Warning,
    MustCompact,
}

pub struct CompactionResult {
    pub old_tokens: usize,
    pub new_tokens: usize,
    pub implementation: String,
    pub phase: String,
    pub messages_compacted: usize,
}
