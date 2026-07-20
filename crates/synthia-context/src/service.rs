//! `DefaultContextService` (formerly `ContextService` trait + impl).
//!
//! Provides a service-oriented facade over context assembly, compaction, and
//! protection-zone management. `DefaultContextService` wraps the existing
//! `ContextAssembler` algorithms so callers (agents, orchestrators) can rely
//! on a single concrete type instead of stitching the building blocks
//! together themselves.
//!
//! The `compact` method returns the simple
//! [`crate::compaction_service::CompactionResult`] (re-exported as
//! [`ServiceCompactionResult`] from this module). The richer
//! [`crate::compaction::compactor::CompactionResult`] remains the canonical
//! type exported at the crate root.
//!
//! The `ContextService` trait that previously abstracted over
//! implementations was REMOVED on 2026-06-15 in change
//! `2026-06-15-p2-trait-cleanup` because it had 0 trait bounds, 0 dyn
//! dispatch, and exactly 1 real implementation (`DefaultContextService`).
//! Methods (`assemble`/`compact`/`protect`) are now inherent.

use std::sync::Arc;

use synthia_provider::{CachePolicy, CompletionRequest, Message, ToolChoice};

/// Alias for the `CompactionResult` shape returned by `DefaultContextService::compact`.
///
/// This is the simple `{ old_tokens, new_tokens }` variant produced by
/// [`crate::compaction_service::compact_messages`]. Exposed under a distinct
/// name so it does not collide with the richer `CompactionResult` re-exported
/// from the crate root.
pub use crate::compaction_service::CompactionResult as ServiceCompactionResult;
use crate::{
    assembler::ContextAssembler,
    compaction_service::{self, CompactionResult},
    protector::ProtectionZone,
    token_budget::TokenBudget,
    traits::estimate_message_tokens,
};

/// Inputs required to assemble a `CompletionRequest`.
///
/// `ContextRequest` carries the conversation messages, an optional system
/// prompt, the token budget, and the protection-zone configuration to apply.
#[derive(Debug, Clone)]
pub struct ContextRequest {
    /// Conversation messages to assemble into the request.
    pub messages: Vec<Message>,
    /// Optional system prompt to prepend.
    pub system_prompt: Option<String>,
    /// Hard token budget for the assembled context.
    pub max_tokens: usize,
    /// Protection zone determining how many recent rounds to preserve.
    pub protection_zone: ProtectionZone,
}

impl ContextRequest {
    /// Create a new request with default protection zone.
    pub fn new(messages: Vec<Message>, max_tokens: usize) -> Self {
        Self {
            messages,
            system_prompt: None,
            max_tokens,
            protection_zone: ProtectionZone::default(),
        }
    }

    /// Attach a system prompt to the request.
    pub fn with_system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(prompt.into());
        self
    }

    /// Override the default protection zone.
    pub fn with_protection_zone(mut self, zone: ProtectionZone) -> Self {
        self.protection_zone = zone;
        self
    }
}

/// Output of context assembly.
///
/// Wraps the prepared `CompletionRequest` ready to send to a provider.
#[derive(Debug, Clone)]
pub struct ContextResult {
    /// Assembled completion request including any system prompt and trimmed messages.
    pub request: CompletionRequest,
}

/// Service backed by the existing `ContextAssembler` algorithms.
///
/// `ContextService` trait REMOVED 2026-06-15 (change `2026-06-15-p2-trait-cleanup`).
/// Methods (`assemble`/`compact`/`protect`) are now inherent.
#[derive(Debug, Clone)]
pub struct DefaultContextService {
    /// Protection ratio applied during compaction (default 0.35).
    protection_ratio: f64,
}

impl Default for DefaultContextService {
    fn default() -> Self {
        Self::new()
    }
}

impl DefaultContextService {
    /// Construct with the default protection ratio (0.35).
    pub fn new() -> Self {
        Self {
            protection_ratio: 0.35,
        }
    }

    /// Customize the protection ratio used during compaction.
    pub fn with_protection_ratio(mut self, ratio: f64) -> Self {
        self.protection_ratio = ratio;
        self
    }

    /// Mirrors the synchronous portion of `ContextAssembler::prepare`:
    /// keep the protection-zone messages, then trim oldest until the budget
    /// is satisfied.
    fn trim_to_budget(request: &ContextRequest) -> Vec<Message> {
        let total_tokens: usize =
            request.messages.iter().map(estimate_message_tokens).sum();

        let mut messages = request.protection_zone.get_recent_messages_owned(
            &request.messages,
            request.protection_zone.min_rounds,
        );

        if total_tokens > request.max_tokens {
            while messages.iter().map(estimate_message_tokens).sum::<usize>()
                > request.max_tokens
                && messages.len() > 1
            {
                messages.remove(0);
            }
        }

        messages
    }
}

impl DefaultContextService {
    #[allow(deprecated)]
    pub fn assemble(&self, request: &ContextRequest) -> ContextResult {
        let messages = Self::trim_to_budget(request);

        // Delegate system-prompt construction to ContextAssembler so any
        // layered prompt logic (memories, skills, workspace info) stays in
        // one place. We construct a per-call assembler because the request
        // already carries the configuration we need.
        let mut assembler = ContextAssembler::new(request.max_tokens)
            .with_protection_zone(request.protection_zone.clone());
        if let Some(prompt) = &request.system_prompt {
            assembler = assembler.with_system_prompt(prompt.clone());
        }
        let system_prompt = assembler.build_system_prompt();

        let mut final_messages = messages;
        if !system_prompt.is_empty() {
            final_messages.insert(0, Message::system(system_prompt));
        }

        let completion = CompletionRequest {
            model: "default".to_string(),
            messages: Arc::new(final_messages),
            tools: Arc::new(vec![]),
            tool_choice: ToolChoice::Auto,
            temperature: None,
            max_tokens: Some(request.max_tokens),
            stop_sequences: vec![],
            extra_body: None,
            cache_policy: Some(CachePolicy::default()),
        };

        ContextResult {
            request: completion,
        }
    }

    pub fn compact(
        &self,
        messages: &mut Vec<Message>,
        budget: &TokenBudget,
    ) -> Option<CompactionResult> {
        let token_count: usize =
            messages.iter().map(estimate_message_tokens).sum();
        compaction_service::compact_messages(
            messages,
            budget,
            token_count,
            self.protection_ratio,
        )
    }

    pub fn protect(&self, messages: &mut Vec<Message>, zone: &ProtectionZone) {
        let recent = zone.get_recent_messages_owned(messages, zone.min_rounds);
        *messages = recent;
    }
}

#[cfg(test)]
mod tests {
    use synthia_provider::Role;

    use super::*;

    #[test]
    fn test_assemble_includes_system_prompt() {
        let svc = DefaultContextService::new();
        let req = ContextRequest::new(vec![Message::user("hi")], 4096)
            .with_system_prompt("You are helpful");
        let result = svc.assemble(&req);
        assert_eq!(result.request.messages.len(), 2);
        assert_eq!(result.request.messages[0].role, Role::System);
        assert_eq!(result.request.max_tokens, Some(4096));
    }

    #[test]
    fn test_assemble_without_system_prompt() {
        let svc = DefaultContextService::new();
        let req = ContextRequest::new(vec![Message::user("hi")], 4096);
        let result = svc.assemble(&req);
        assert_eq!(result.request.messages.len(), 1);
        assert_eq!(result.request.messages[0].role, Role::User);
    }

    #[test]
    fn test_protect_keeps_recent_rounds() {
        let svc = DefaultContextService::new();
        let mut messages = Vec::new();
        for i in 0..5 {
            messages.push(Message::user(format!("u{}", i)));
            messages.push(Message::assistant(format!("a{}", i)));
        }
        let zone = ProtectionZone::new(2, 0.35);
        svc.protect(&mut messages, &zone);
        // 2 user + 2 assistant = 4 messages preserved
        assert_eq!(messages.len(), 4);
    }

    #[test]
    fn test_compact_no_op_within_budget() {
        let svc = DefaultContextService::new();
        let mut messages =
            vec![Message::user("hi"), Message::assistant("hello")];
        let budget = TokenBudget::new(100_000);
        let result = svc.compact(&mut messages, &budget);
        assert!(result.is_none());
        assert_eq!(messages.len(), 2);
    }

    #[test]
    fn test_default_context_service_default_trait() {
        let svc = DefaultContextService::default();
        let req = ContextRequest::new(vec![Message::user("hi")], 4096);
        let result = svc.assemble(&req);
        assert_eq!(result.request.messages.len(), 1);
    }

    #[test]
    fn test_with_protection_ratio() {
        let svc = DefaultContextService::new().with_protection_ratio(0.5);
        let req = ContextRequest::new(vec![Message::user("hi")], 4096);
        let result = svc.assemble(&req);
        assert_eq!(result.request.messages.len(), 1);
    }
}
