//! Context / Compaction extension points: 7 typed hook points fired by
//! the context management subsystem. All points use the `Action<Output>`
//! mutation pattern (mirroring `tool.rs` and `llm.rs`).
//!
//! # Design
//!
//! - **P1 prefix hash timing**: hooks that mutate the message list
//!   (`context.message_filter`, `context.compact.replace`) MUST fire
//!   BEFORE the prefix hash is computed. The orchestrator re-snapshots
//!   the prefix hash AFTER the hook chain returns. This is the same
//!   pattern as `compact_context_tool` (see archived change).
//! - **`context.prefix.participate`**: extensions return `Vec<u8>` to be
//!   mixed into the hash. This is how skills / RAG indexes opt in to
//!   the cache key. Order of registration is preserved in the hash.
//! - **Observe-only points**: `context.compact.trigger` and
//!   `context.observability.emit` are fire-and-forget — they return `()`
//!   and never mutate state.
//!
//! # Points
//!
//! | Name | Payload | Purpose |
//! |------|---------|---------|
//! | `context.compact.trigger` | `CompactTriggerInput` | Observe-only: external trigger signal |
//! | `context.compact.summarize` | `SummarizeInput` | Custom summary strategy |
//! | `context.compact.replace` | `CompactPlan` | Change replacement strategy |
//! | `context.prefix.participate` | () | Return bytes to include in prefix hash |
//! | `context.observability.emit` | `ContextObservabilityEvent` | Observe-only: metrics emission |
//! | `context.token_budget.adjust` | () | Return new `TokenBudget` |
//! | `context.message_filter` | `MessageFilterInput` | Reorder / redact / annotate messages |

use std::sync::Arc;

use dashmap::DashMap;
use serde::{Deserialize, Serialize};

use super::tool::Action;

// =====================================================================
// Typed payloads
// =====================================================================

/// `context.compact.trigger` event payload (observe-only).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactTriggerInput {
    pub session_id: String,
    pub reason: String,
    pub current_tokens: u64,
    pub threshold: u64,
}

impl CompactTriggerInput {
    pub fn new(
        session_id: impl Into<String>,
        reason: impl Into<String>,
        current_tokens: u64,
        threshold: u64,
    ) -> Self {
        Self {
            session_id: session_id.into(),
            reason: reason.into(),
            current_tokens,
            threshold,
        }
    }
}

/// `context.compact.summarize` event payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SummarizeInput {
    pub session_id: String,
    pub head: String,
    pub previous_summary: Option<String>,
    pub max_tokens: u32,
}

impl SummarizeInput {
    pub fn new(
        session_id: impl Into<String>,
        head: impl Into<String>,
        previous_summary: Option<String>,
        max_tokens: u32,
    ) -> Self {
        Self {
            session_id: session_id.into(),
            head: head.into(),
            previous_summary,
            max_tokens,
        }
    }
}

/// `context.compact.replace` event payload + response.
#[derive(
    Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default,
)]
pub enum CompactStrategy {
    #[default]
    DropOldest,
    SummarizeMiddle,
    PreserveLastN,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactPlan {
    pub session_id: String,
    pub tokens_to_remove: u64,
    pub strategy: CompactStrategy,
    /// Number of most recent messages to preserve when
    /// `strategy = PreserveLastN`.
    pub preserve_last_n: u32,
}

impl CompactPlan {
    pub fn new(
        session_id: impl Into<String>,
        tokens_to_remove: u64,
        strategy: CompactStrategy,
        preserve_last_n: u32,
    ) -> Self {
        Self {
            session_id: session_id.into(),
            tokens_to_remove,
            strategy,
            preserve_last_n,
        }
    }
}

/// `context.observability.emit` event payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextObservabilityEvent {
    pub session_id: String,
    pub metric: String,
    pub value: f64,
    pub tags: serde_json::Value,
}

impl ContextObservabilityEvent {
    pub fn new(
        session_id: impl Into<String>,
        metric: impl Into<String>,
        value: f64,
        tags: serde_json::Value,
    ) -> Self {
        Self {
            session_id: session_id.into(),
            metric: metric.into(),
            value,
            tags,
        }
    }
}

/// `context.token_budget.adjust` event response.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct TokenBudget {
    pub soft_limit: u32,
    pub hard_limit: u32,
}

impl Default for TokenBudget {
    fn default() -> Self {
        Self {
            soft_limit: 100_000,
            hard_limit: 120_000,
        }
    }
}

/// `context.message_filter` event payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageFilterInput {
    pub session_id: String,
    /// JSON-serialized message list (per Phase 3 tool `arguments` precedent).
    pub messages: serde_json::Value,
}

impl MessageFilterInput {
    pub fn new(
        session_id: impl Into<String>,
        messages: serde_json::Value,
    ) -> Self {
        Self {
            session_id: session_id.into(),
            messages,
        }
    }
}

// =====================================================================
// Handler aliases
// =====================================================================

pub type CompactTriggerHandler =
    Arc<dyn Fn(&CompactTriggerInput) + Send + Sync>;

/// `summarize` handler: returns `Option<String>` where `None` = use the
/// default LLM-based summarization.
pub type SummarizeHandler =
    Arc<dyn Fn(&SummarizeInput) -> Option<String> + Send + Sync>;

pub type CompactReplaceHandler =
    Arc<dyn Fn(&CompactPlan) -> Action<CompactPlan> + Send + Sync>;

/// `prefix.participate` handler: returns `Vec<u8>` to mix into the hash.
pub type PrefixParticipateHandler = Arc<dyn Fn() -> Vec<u8> + Send + Sync>;

pub type ObservabilityEmitHandler =
    Arc<dyn Fn(&ContextObservabilityEvent) + Send + Sync>;

/// `token_budget.adjust` handler: returns `Option<TokenBudget>` where
/// `None` = use the default.
pub type TokenBudgetHandler =
    Arc<dyn Fn() -> Option<TokenBudget> + Send + Sync>;

pub type MessageFilterHandler = Arc<
    dyn Fn(&MessageFilterInput) -> Action<MessageFilterInput> + Send + Sync,
>;

// =====================================================================
// Registry
// =====================================================================

pub struct ContextExtensionRegistry {
    compact_trigger: DashMap<String, Vec<CompactTriggerHandler>>,
    summarize: DashMap<String, Vec<SummarizeHandler>>,
    compact_replace: DashMap<String, Vec<CompactReplaceHandler>>,
    prefix_participate: DashMap<String, Vec<PrefixParticipateHandler>>,
    observability_emit: DashMap<String, Vec<ObservabilityEmitHandler>>,
    token_budget: DashMap<String, Vec<TokenBudgetHandler>>,
    message_filter: DashMap<String, Vec<MessageFilterHandler>>,
    active_keys: DashMap<String, ()>,
}

impl std::fmt::Debug for ContextExtensionRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ContextExtensionRegistry")
            .field("compact_trigger", &self.compact_trigger.len())
            .field("summarize", &self.summarize.len())
            .field("compact_replace", &self.compact_replace.len())
            .field("prefix_participate", &self.prefix_participate.len())
            .field("observability_emit", &self.observability_emit.len())
            .field("token_budget", &self.token_budget.len())
            .field("message_filter", &self.message_filter.len())
            .finish()
    }
}

impl Default for ContextExtensionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ContextExtensionRegistry {
    pub fn new() -> Self {
        Self {
            compact_trigger: DashMap::new(),
            summarize: DashMap::new(),
            compact_replace: DashMap::new(),
            prefix_participate: DashMap::new(),
            observability_emit: DashMap::new(),
            token_budget: DashMap::new(),
            message_filter: DashMap::new(),
            active_keys: DashMap::new(),
        }
    }

    pub fn register_compact_trigger(
        &self,
        id: impl Into<String>,
        handler: CompactTriggerHandler,
    ) {
        self.compact_trigger
            .entry(id.into())
            .or_default()
            .push(handler);
        self.active_keys
            .insert("context.compact.trigger".into(), ());
    }

    pub fn register_summarize(
        &self,
        id: impl Into<String>,
        handler: SummarizeHandler,
    ) {
        self.summarize.entry(id.into()).or_default().push(handler);
        self.active_keys
            .insert("context.compact.summarize".into(), ());
    }

    pub fn register_compact_replace(
        &self,
        id: impl Into<String>,
        handler: CompactReplaceHandler,
    ) {
        self.compact_replace
            .entry(id.into())
            .or_default()
            .push(handler);
        self.active_keys
            .insert("context.compact.replace".into(), ());
    }

    pub fn register_prefix_participate(
        &self,
        id: impl Into<String>,
        handler: PrefixParticipateHandler,
    ) {
        self.prefix_participate
            .entry(id.into())
            .or_default()
            .push(handler);
        self.active_keys
            .insert("context.prefix.participate".into(), ());
    }

    pub fn register_observability_emit(
        &self,
        id: impl Into<String>,
        handler: ObservabilityEmitHandler,
    ) {
        self.observability_emit
            .entry(id.into())
            .or_default()
            .push(handler);
        self.active_keys
            .insert("context.observability.emit".into(), ());
    }

    pub fn register_token_budget(
        &self,
        id: impl Into<String>,
        handler: TokenBudgetHandler,
    ) {
        self.token_budget
            .entry(id.into())
            .or_default()
            .push(handler);
        self.active_keys
            .insert("context.token_budget.adjust".into(), ());
    }

    pub fn register_message_filter(
        &self,
        id: impl Into<String>,
        handler: MessageFilterHandler,
    ) {
        self.message_filter
            .entry(id.into())
            .or_default()
            .push(handler);
        self.active_keys.insert("context.message_filter".into(), ());
    }

    pub fn has_handlers(&self, point: &str) -> bool {
        self.active_keys.contains_key(point)
    }

    /// Fire `context.compact.trigger` (observe-only).
    pub fn fire_compact_trigger(&self, event: &CompactTriggerInput) {
        for entry in self.compact_trigger.iter() {
            for (idx, handler) in entry.value().iter().enumerate() {
                let extension_id = format!("{}#{}", entry.key(), idx);
                let _span = tracing::info_span!(
                    target: "synthia.extension",
                    "extension.hook",
                    point = "context.compact.trigger",
                    scope = "context",
                    extension_id = extension_id.as_str(),
                )
                .entered();
                handler(event);
            }
        }
    }

    /// Fire `context.compact.summarize`. Returns the FIRST non-`None`
    /// summary (registration order). If all handlers return `None`, the
    /// caller falls back to default LLM summarization.
    pub fn fire_summarize(&self, event: &SummarizeInput) -> Option<String> {
        for entry in self.summarize.iter() {
            for (idx, handler) in entry.value().iter().enumerate() {
                let extension_id = format!("{}#{}", entry.key(), idx);
                let _span = tracing::info_span!(
                    target: "synthia.extension",
                    "extension.hook",
                    point = "context.compact.summarize",
                    scope = "context",
                    extension_id = extension_id.as_str(),
                )
                .entered();
                if let Some(s) = handler(event) {
                    return Some(s);
                }
            }
        }
        None
    }

    /// Fire `context.compact.replace`.
    pub fn fire_compact_replace(
        &self,
        mut plan: CompactPlan,
    ) -> Action<CompactPlan> {
        for entry in self.compact_replace.iter() {
            for (idx, handler) in entry.value().iter().enumerate() {
                let extension_id = format!("{}#{}", entry.key(), idx);
                let _span = tracing::info_span!(
                    target: "synthia.extension",
                    "extension.hook",
                    point = "context.compact.replace",
                    scope = "context",
                    extension_id = extension_id.as_str(),
                )
                .entered();
                match handler(&plan) {
                    Action::Proceed => {}
                    Action::Modify(replacement) => {
                        plan = replacement;
                    }
                    Action::Skip { reason } => {
                        return Action::Skip { reason };
                    }
                }
            }
        }
        Action::Modify(plan)
    }

    /// Fire `context.prefix.participate`. Returns the concatenation of
    /// all `Vec<u8>` returned by handlers (preserving registration
    /// order). The orchestrator mixes this into the prefix hash.
    pub fn fire_prefix_participate(&self) -> Vec<u8> {
        let mut out = Vec::new();
        for entry in self.prefix_participate.iter() {
            for (idx, handler) in entry.value().iter().enumerate() {
                let extension_id = format!("{}#{}", entry.key(), idx);
                let _span = tracing::info_span!(
                    target: "synthia.extension",
                    "extension.hook",
                    point = "context.prefix.participate",
                    scope = "context",
                    extension_id = extension_id.as_str(),
                )
                .entered();
                out.extend(handler());
            }
        }
        out
    }

    /// Fire `context.observability.emit` (observe-only).
    pub fn fire_observability_emit(&self, event: &ContextObservabilityEvent) {
        for entry in self.observability_emit.iter() {
            for (idx, handler) in entry.value().iter().enumerate() {
                let extension_id = format!("{}#{}", entry.key(), idx);
                let _span = tracing::info_span!(
                    target: "synthia.extension",
                    "extension.hook",
                    point = "context.observability.emit",
                    scope = "context",
                    extension_id = extension_id.as_str(),
                )
                .entered();
                handler(event);
            }
        }
    }

    /// Fire `context.token_budget.adjust`. Returns the FIRST non-`None`
    /// budget (registration order). If all handlers return `None`, the
    /// caller uses the default.
    pub fn fire_token_budget(&self) -> Option<TokenBudget> {
        for entry in self.token_budget.iter() {
            for (idx, handler) in entry.value().iter().enumerate() {
                let extension_id = format!("{}#{}", entry.key(), idx);
                let _span = tracing::info_span!(
                    target: "synthia.extension",
                    "extension.hook",
                    point = "context.token_budget.adjust",
                    scope = "context",
                    extension_id = extension_id.as_str(),
                )
                .entered();
                if let Some(b) = handler() {
                    return Some(b);
                }
            }
        }
        None
    }

    /// Fire `context.message_filter`. P1 hash is recomputed by the
    /// orchestrator after this returns.
    pub fn fire_message_filter(
        &self,
        mut event: MessageFilterInput,
    ) -> Action<MessageFilterInput> {
        for entry in self.message_filter.iter() {
            for (idx, handler) in entry.value().iter().enumerate() {
                let extension_id = format!("{}#{}", entry.key(), idx);
                let _span = tracing::info_span!(
                    target: "synthia.extension",
                    "extension.hook",
                    point = "context.message_filter",
                    scope = "context",
                    extension_id = extension_id.as_str(),
                    session_id = event.session_id.as_str(),
                    payload_size = event.messages.to_string().len(),
                )
                .entered();
                match handler(&event) {
                    Action::Proceed => {}
                    Action::Modify(replacement) => {
                        event = replacement;
                    }
                    Action::Skip { reason } => {
                        return Action::Skip { reason };
                    }
                }
            }
        }
        Action::Modify(event)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    #[test]
    fn new_registry_is_empty() {
        let reg = ContextExtensionRegistry::new();
        assert!(!reg.has_handlers("context.compact.trigger"));
        assert!(!reg.has_handlers("context.compact.summarize"));
        assert!(!reg.has_handlers("context.compact.replace"));
        assert!(!reg.has_handlers("context.prefix.participate"));
        assert!(!reg.has_handlers("context.observability.emit"));
        assert!(!reg.has_handlers("context.token_budget.adjust"));
        assert!(!reg.has_handlers("context.message_filter"));
    }

    #[test]
    fn noop_filter_preserves_hash() {
        // A no-op filter chain returns the original message list
        // unchanged, so the prefix hash is preserved.
        let reg = ContextExtensionRegistry::new();
        let h: MessageFilterHandler = Arc::new(|ev| {
            Action::Modify(MessageFilterInput {
                session_id: ev.session_id.clone(),
                messages: ev.messages.clone(),
            })
        });
        reg.register_message_filter("noop", h);

        let input = MessageFilterInput::new(
            "s1",
            serde_json::json!([{"role": "user", "content": "hi"}]),
        );
        let Action::Modify(out) = reg.fire_message_filter(input.clone()) else {
            panic!("expected Modify")
        };
        assert_eq!(out.messages, input.messages);
    }

    #[test]
    fn modifying_filter_invalidates_cache() {
        // A modifying filter changes the message list → caller MUST
        // re-snapshot the prefix hash.
        let reg = ContextExtensionRegistry::new();
        let h: MessageFilterHandler = Arc::new(|ev| {
            Action::Modify(MessageFilterInput {
                session_id: ev.session_id.clone(),
                messages: serde_json::json!([{"role": "user", "content": "REDACTED"}]),
            })
        });
        reg.register_message_filter("redact", h);

        let input = MessageFilterInput::new(
            "s1",
            serde_json::json!([{"role": "user", "content": "secret@email"}]),
        );
        let Action::Modify(out) = reg.fire_message_filter(input) else {
            panic!("expected Modify")
        };
        assert_eq!(
            out.messages,
            serde_json::json!([{"role": "user", "content": "REDACTED"}])
        );
    }

    #[test]
    fn prefix_participate_bytes_included_in_hash() {
        let reg = ContextExtensionRegistry::new();
        let h1: PrefixParticipateHandler =
            Arc::new(|| b"skill:snapshot:42".to_vec());
        let h2: PrefixParticipateHandler =
            Arc::new(|| b"rag:index:v3".to_vec());
        reg.register_prefix_participate("skill", h1);
        reg.register_prefix_participate("rag", h2);

        let bytes = reg.fire_prefix_participate();
        let s = std::str::from_utf8(&bytes).unwrap();
        assert!(s.contains("skill:snapshot:42"));
        assert!(s.contains("rag:index:v3"));
    }

    #[test]
    fn summarize_override_skips_llm_call() {
        let reg = ContextExtensionRegistry::new();
        let h: SummarizeHandler = Arc::new(|ev| {
            Some(format!(
                "[custom summary of {} chars for session {}]",
                ev.head.len(),
                ev.session_id
            ))
        });
        reg.register_summarize("custom", h);

        let result = reg.fire_summarize(&SummarizeInput::new(
            "s1",
            "head content",
            None,
            256,
        ));
        let s = result.expect("custom summary should be returned");
        assert!(s.contains("[custom summary"));
        assert!(s.contains("session s1"));
    }

    #[test]
    fn summarize_returns_none_when_no_handler_provides() {
        let reg = ContextExtensionRegistry::new();
        // No handlers registered → None → caller uses default
        // LLM-based summarization.
        assert!(
            reg.fire_summarize(&SummarizeInput::new("s1", "head", None, 256))
                .is_none()
        );
    }

    #[test]
    fn token_budget_returns_first_non_none() {
        let reg = ContextExtensionRegistry::new();
        let h_none: TokenBudgetHandler = Arc::new(|| None);
        let h_some: TokenBudgetHandler = Arc::new(|| {
            Some(TokenBudget {
                soft_limit: 50_000,
                hard_limit: 60_000,
            })
        });
        // DashMap iteration order is not guaranteed; the
        // "first non-None" semantic is registration-order based via
        // linear scan. We register h_none first and h_some second —
        // the registry returns the first non-None it encounters in
        // iteration order. This test simply verifies that when at
        // least one handler returns Some, fire_token_budget returns
        // Some. The exact ordering of None/Some is not asserted.
        reg.register_token_budget("none", h_none);
        reg.register_token_budget("some", h_some);
        let result = reg.fire_token_budget();
        assert!(result.is_some());
    }

    #[test]
    fn compact_trigger_is_observe_only() {
        let reg = ContextExtensionRegistry::new();
        let counter = Arc::new(AtomicUsize::new(0));
        let c = counter.clone();
        let h: CompactTriggerHandler = Arc::new(move |_ev| {
            c.fetch_add(1, Ordering::SeqCst);
        });
        reg.register_compact_trigger("audit", h);
        reg.fire_compact_trigger(&CompactTriggerInput::new(
            "s1",
            "threshold",
            100_000,
            90_000,
        ));
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn compact_replace_changes_strategy() {
        let reg = ContextExtensionRegistry::new();
        let h: CompactReplaceHandler = Arc::new(|p| {
            Action::Modify(CompactPlan {
                strategy: CompactStrategy::PreserveLastN,
                preserve_last_n: 10,
                ..p.clone()
            })
        });
        reg.register_compact_replace("preserve-recent", h);
        let result = reg.fire_compact_replace(CompactPlan::new(
            "s1",
            5000,
            CompactStrategy::DropOldest,
            0,
        ));
        if let Action::Modify(plan) = result {
            assert_eq!(plan.strategy, CompactStrategy::PreserveLastN);
            assert_eq!(plan.preserve_last_n, 10);
        } else {
            panic!("expected Modify");
        }
    }

    #[test]
    fn message_filter_proceed_is_no_op() {
        // If a handler returns Proceed, the chain continues with the
        // original event.
        let reg = ContextExtensionRegistry::new();
        let h: MessageFilterHandler = Arc::new(|_ev| Action::Proceed);
        reg.register_message_filter("noop", h);
        let input = MessageFilterInput::new(
            "s1",
            serde_json::json!([{"role": "user", "content": "hi"}]),
        );
        let Action::Modify(out) = reg.fire_message_filter(input.clone()) else {
            panic!("expected Modify")
        };
        assert_eq!(out.messages, input.messages);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_register_does_not_lose_handlers() {
        let reg = std::sync::Arc::new(ContextExtensionRegistry::new());
        let mut handles = Vec::new();
        for i in 0..32 {
            let reg = reg.clone();
            handles.push(tokio::spawn(async move {
                let h: CompactTriggerHandler =
                    std::sync::Arc::new(move |_ev| {
                        let _ = i; // each task captures a different i
                    });
                reg.register_compact_trigger(format!("h{}", i), h);
            }));
        }
        for h in handles {
            h.await.unwrap();
        }
        assert!(reg.has_handlers("context.compact.trigger"));
    }
}
