//! Pluggable, async context-window management.
//!
//! Mirrors [`traitclaw_core::traits::context_manager::ContextManager`]:
//! `prepare` is called before every LLM request to ensure the message
//! list fits within the model's context window. The method is `async`
//! so implementations can call out to a summarisation LLM or a remote
//! tokenizer without blocking the agent runtime.
//!
//! # Contract
//!
//! - Implementations **MUST NOT** remove messages whose role is
//!   [`Role::System`] — those carry the agent's persistent identity,
//!   tool manifest, and skill manifest, and the model relies on them
//!   landing at the high-attention edge of the context.
//! - Implementations **MUST** update `state.estimated_tokens` to
//!   reflect the post-prepare message list so downstream consumers
//!   (the `Tracker` / `Hint` subsystems and the `Usage` system
//!   event) can quote the same number the manager itself used to
//!   make its decision.
//! - Implementations **SHOULD** set `state.last_truncated = true`
//!   whenever they removed or compressed any message. The agent
//!   loop surfaces this flag as a `WarningKind::ContextTruncated`
//!   system event so the UI can render a visible notice.
//!
//! # Default estimation
//!
//! The trait ships with a cheap `4-char ≈ 1-token` heuristic that
//! delegates to [`synthia_provider::token_counter::estimate_messages_token_count`].
//! Synthia's `Message.content` is a `Vec<ContentPart>` that may carry
//! tool-use JSON, images, and audio, all of which that estimator
//! collapses to text-only — it is intentionally conservative.
//! Implementations that need provider-accurate counts (Anthropic
//! tokenizer, OpenAI `tiktoken`) override `estimate_tokens`.
//!
//! [`traitclaw_core::traits::context_manager::ContextManager`]:
//!     https://docs.rs/traitclaw-core

use std::sync::Arc;

use async_trait::async_trait;
use synthia_provider::{
    Message,
    ModelConfig,
    Role,
    TokenUsage,
    token_counter::estimate_messages_token_count,
};

/// Per-category usage attribution for one agent run (dsh
/// token-meter semantics).
/// `AgentState` holds one of these and feeds it via
/// [`AgentState::add_token_usage`]; after the run the caller
/// reads the split for cost dashboards and billing exports.
/// All counters saturate on overflow.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UsageMeter {
    /// Sum of provider-reported prompt tokens.
    pub input_tokens: usize,
    /// Sum of provider-reported completion tokens (includes
    /// reasoning tokens where the provider bills them as output).
    pub output_tokens: usize,
    /// Sum of KV-cache read tokens (Anthropic
    /// `cache_read_input_tokens` / OpenAI `cached_tokens`).
    pub cache_read_tokens: usize,
    /// Sum of KV-cache write tokens (Anthropic
    /// `cache_creation_input_tokens`).
    pub cache_write_tokens: usize,
    /// Sum of reasoning / thinking tokens (OpenAI
    /// `completion_tokens_details.reasoning_tokens`).
    pub reasoning_tokens: usize,
    /// Number of sampling passes recorded.
    pub calls: usize,
}

impl UsageMeter {
    /// Fold one provider response's usage into the meter.
    /// `None` optionals contribute `0`.
    pub fn record(&mut self, usage: &TokenUsage) {
        self.input_tokens =
            self.input_tokens.saturating_add(usage.prompt_tokens);
        self.output_tokens =
            self.output_tokens.saturating_add(usage.completion_tokens);
        self.cache_read_tokens = self
            .cache_read_tokens
            .saturating_add(usage.cache_read_tokens.unwrap_or(0));
        self.cache_write_tokens = self
            .cache_write_tokens
            .saturating_add(usage.cache_write_tokens.unwrap_or(0));
        self.reasoning_tokens = self
            .reasoning_tokens
            .saturating_add(usage.reasoning_tokens.unwrap_or(0));
        self.calls += 1;
    }

    /// Cache hit ratio over the run: cache-read tokens divided
    /// by total input tokens (`0.0` when no input was recorded).
    #[allow(clippy::cast_precision_loss)]
    pub fn cache_hit_ratio(&self) -> f32 {
        if self.input_tokens == 0 {
            return 0.0;
        }
        self.cache_read_tokens as f32 / self.input_tokens as f32
    }
}

/// Snapshot of the agent's runtime state passed to [`ContextManager`].
///
/// The agent loop constructs one of these at the start of each
/// session from the resolved [`ModelConfig`] and hands it to
/// every `prepare` call. Implementations read it for
/// context-budget information and may mutate it to publish their
/// own observations (the post-prepare token estimate, whether
/// anything was dropped).
pub struct AgentState {
    /// Maximum token budget for this run. Comes straight from
    /// [`ModelConfig::context_window`] and stays constant for the
    /// lifetime of a session.
    pub context_window: usize,
    /// Post-`prepare` token estimate of the full message list.
    /// Read by the `SamplingResult::usage` pipeline
    /// and by `Hint` implementations that fire when utilisation
    /// crosses a threshold.
    pub estimated_tokens: usize,
    /// Current ReAct iteration. Bumped by `ReActLoop::react_step`
    /// before each sample pass; the manager reads it but never
    /// writes it (the loop owns the canonical counter).
    pub iteration_count: usize,
    /// `true` when the most recent `prepare` actually removed or
    /// compressed a message. Reset to `false` at the start of every
    /// `prepare` call so callers can rely on the flag meaning
    /// "this call did something".
    pub last_truncated: bool,
    /// Provider-reported token totals accumulated over the whole
    /// run (prompt + completion, every sampling pass summed).
    /// Maintained by the agent loop right after each provider
    /// response; read by budget guards and hints.
    pub total_tokens: usize,
    /// Per-category usage attribution accumulated over the whole
    /// run (dsh token-meter semantics): input / output /
    /// cache-read / cache-write / reasoning. Fed by
    /// [`AgentState::add_token_usage`]; queryable after the run
    /// for cost dashboards and billing exports.
    pub usage_meter: UsageMeter,
    /// Number of tool calls dispatched this run (including calls
    /// later denied by a guard — the counter bumps before the
    /// guard pipeline runs). Maintained by the agent loop.
    pub tool_call_count: usize,
    /// Rolling fingerprints of recently dispatched tool calls
    /// (`tool_name` + canonicalised arguments), oldest first.
    /// Capped at [`FINGERPRINT_WINDOW`]. The steering layer's
    /// loop-detection guard reads the identical tail run to spot
    /// a model stuck repeating the exact same call.
    pub recent_tool_fingerprints: Vec<String>,
}

/// Cap on [`AgentState::recent_tool_fingerprints`]. Sixteen
/// entries is far beyond any sane loop-detection window while
/// keeping the per-run state bounded.
pub const FINGERPRINT_WINDOW: usize = 16;

impl AgentState {
    /// Construct a fresh state from a model's resolved
    /// [`ModelConfig`]. `estimated_tokens` starts at `0`; the agent
    /// loop runs `prepare` once at session start to populate it
    /// before the first LLM call.
    pub fn from_config(cfg: &ModelConfig) -> Self {
        Self::with_window(cfg.context_window)
    }

    /// Construct a state with an explicit window. Used by tests and
    /// by callers that do not have a full [`ModelConfig`] in scope
    /// (e.g. the embedded unit tests in this module).
    pub fn with_window(context_window: usize) -> Self {
        Self {
            context_window,
            estimated_tokens: 0,
            iteration_count: 0,
            last_truncated: false,
            total_tokens: 0,
            usage_meter: UsageMeter::default(),
            tool_call_count: 0,
            recent_tool_fingerprints: Vec::new(),
        }
    }

    /// Record one dispatched tool call: bump the counter and push
    /// the call's fingerprint onto the rolling window. Called by
    /// the agent loop before the guard pipeline so guards observe
    /// the call being counted.
    pub fn record_tool_call(&mut self, fingerprint: String) {
        self.tool_call_count += 1;
        self.recent_tool_fingerprints.push(fingerprint);
        if self.recent_tool_fingerprints.len() > FINGERPRINT_WINDOW {
            let overflow =
                self.recent_tool_fingerprints.len() - FINGERPRINT_WINDOW;
            self.recent_tool_fingerprints.drain(0..overflow);
        }
    }

    /// Length of the trailing run of identical fingerprints (0 when
    /// the window is empty). `n` means the last `n` dispatched
    /// tool calls were indistinguishable from each other.
    pub fn identical_tail_run(&self) -> usize {
        let Some(last) = self.recent_tool_fingerprints.last() else {
            return 0;
        };
        self.recent_tool_fingerprints
            .iter()
            .rev()
            .take_while(|fp| *fp == last)
            .count()
    }

    /// Accumulate provider-reported usage into [`AgentState::total_tokens`]
    /// (counting prompt + completion tokens; cache reads stay excluded
    pub fn add_token_usage(&mut self, usage: &TokenUsage) {
        self.total_tokens =
            self.total_tokens.saturating_add(usage.total_tokens);
        self.usage_meter.record(usage);
    }

    /// Fraction of the context window that the current message list
    /// is consuming (0.0 - 1.0). Returns `0.0` for an unset
    /// window so callers never divide by zero.
    #[allow(clippy::cast_precision_loss)]
    pub fn context_utilization(&self) -> f32 {
        if self.context_window == 0 {
            return 0.0;
        }
        self.estimated_tokens as f32 / self.context_window as f32
    }
}

/// Async trait for pluggable context-window management.
///
/// Called by `ReActLoop::prepare` before every provider call. The
/// implementation decides how to keep the message list under
/// `state.context_window` — typical strategies are pairwise
/// user/assistant drop, LLM-driven summarisation of the oldest
/// turn, and tool-result eviction.
/// A `ContextManager` trims the message list to fit inside the
/// configured context window before each LLM call.
///
/// The trait exposes **two** surfaces:
///
/// - [`ContextManager::prepare`] (mutating, legacy). Accepts
///   `&mut Vec<Message>`; implementations may shrink in place. The
///   synthesia core has used this shape since v0; the agent loop
///   continues to call it on the *first* iteration where no
///   cached `Arc` exists yet.
/// - [`ContextManager::prepare_arc`] (Arc-shared, R6-6). Accepts
///   `Arc<Vec<Message>>` and returns the same `Arc` when no
///   truncation happened, or a fresh `Arc::new(...)` when it
///   did. The agent loop's hot path uses this surface so the
///   downstream [`synthia_provider::cache_policy::CachePolicyApplier`]
///   can `Arc::ptr_eq` the messages Arc against the previous call
///   and short-circuit cache-marker recomputation when nothing
///   actually changed.
///
/// All shipped managers (`Noop`, `Truncating`, `Dag`,
/// `Summarizing`) implement `prepare_arc` natively; the
/// `prepare` default impl falls back to the legacy surface via
/// `Arc::get_mut` so callers that *only* override `prepare` still
/// get correct semantics — they just lose the cache shortcut.
#[async_trait]
pub trait ContextManager: Send + Sync {
    /// Mutate `messages` in place until it fits within
    /// `state.context_window`. May call out to other async
    /// services (an LLM, a remote tokenizer, a memory store).
    /// Prefer [`ContextManager::prepare_arc`] on the agent loop's
    /// hot path so the Arc-shared cache contract survives.
    async fn prepare(
        &self,
        messages: &mut Vec<Message>,
        state: &mut AgentState,
    );

    /// Arc-shared variant of [`ContextManager::prepare`]. Returns
    /// the input `Arc` unchanged when the manager did not need
    /// to mutate the list, and a fresh `Arc` when it did. The
    /// default implementation materialises a copy of `messages`
    /// via `Arc::get_mut` (which requires unique ownership, so
    /// the input `Arc` must be the sole holder) and dispatches
    /// to the legacy [`ContextManager::prepare`]; this default
    /// is sound but loses the cache shortcut. Override
    /// `prepare_arc` directly to keep the Arc alive on the
    /// no-op fast path.
    async fn prepare_arc(
        &self,
        messages: Arc<Vec<Message>>,
        state: &mut AgentState,
    ) -> Arc<Vec<Message>> {
        match Arc::try_unwrap(messages) {
            Ok(mut owned) => {
                self.prepare(&mut owned, state).await;
                Arc::new(owned)
            }
            Err(shared) => {
                // Another consumer (cache policy, prompt assembler)
                // still holds the Arc. Materialise a fresh copy,
                // apply the manager, return.
                let mut copy = shared.as_ref().clone();
                self.prepare(&mut copy, state).await;
                Arc::new(copy)
            }
        }
    }

    /// Estimate the token cost of a message list. The default
    /// delegates to the cheap
    /// [`synthia_provider::token_counter::estimate_messages_token_count`]
    /// helper; override with a provider-accurate counter
    /// (tiktoken, Anthropic tokenizer) when tight-window
    /// decisions matter.
    fn estimate_tokens(&self, messages: &[Message]) -> usize {
        synthia_provider::token_counter::estimate_messages_token_count(messages)
    }

    /// R29-Phase-K: hand the manager the file activity observed
    /// since its last `prepare`, so the *next* compaction's
    /// summariser prompt can name what touched disk.
    ///
    /// Default: ignore. Only managers that actually summarise
    /// (e.g. [`SummarizingContextManager`]) consume this; the
    /// truncating / noop strategies have nothing to say to a
    /// summariser. Implementations MUST tolerate being called on
    /// every iteration — a manager that ignores it pays one
    /// discarded call per tool batch, not a reallocation.
    ///
    /// [`SummarizingContextManager`]: crate::SummarizingContextManager
    fn set_compaction_details(
        &self,
        _details: crate::compaction_settings::CompactionDetails,
    ) {
    }
}

/// Boxed, type-erased handle. Most call sites hold this rather than
/// the concrete type so they can be swapped at runtime (the
/// `synthia-server` layer may pick the strategy based on the
/// `ModelConfig`).
pub type SharedContextManager = Arc<dyn ContextManager>;

/// No-op manager — never mutates `messages`, only refreshes the
/// token estimate. Use for very small context windows (e.g. a
/// 4 kB model where the prompt itself dominates) or for tests
/// that need a deterministic message list.
pub struct NoopContextManager;

#[async_trait]
impl ContextManager for NoopContextManager {
    async fn prepare(
        &self,
        messages: &mut Vec<Message>,
        state: &mut AgentState,
    ) {
        state.estimated_tokens = self.estimate_tokens(messages);
        state.last_truncated = false;
        state.estimated_tokens = self.estimate_tokens(messages);
        state.last_truncated = false;
    }

    /// No-op manager: return the input `Arc` unchanged so the
    /// provider-side cache policy can `Arc::ptr_eq` against the
    /// previous request.
    async fn prepare_arc(
        &self,
        messages: Arc<Vec<Message>>,
        state: &mut AgentState,
    ) -> Arc<Vec<Message>> {
        state.estimated_tokens = self.estimate_tokens(&messages);
        state.last_truncated = false;
        messages
    }
}

/// Default pairing-drop strategy.
///
/// Walks the message list from oldest to newest, evicting the
/// oldest **user** message and the assistant reply that follows it
/// as a single atomic pair, repeating until the list fits within
/// `state.context_window`. System messages are preserved verbatim
/// and never reordered. Tool-result messages attached to a dropped
/// assistant turn are dropped with it (no orphaned tool results
/// left in the visible history).
///
/// # Why pairwise drop
///
/// Dropping only the user message would leave the assistant reply
/// ungrounded, and dropping only the assistant reply would leave
/// the user question dangling — both shapes confuse the model in
/// practice. Pairwise drop keeps the conversation surface
/// continuous and is the cheapest correct strategy for
/// provider-side context windows that lack a built-in
/// summariser.
pub struct TruncatingContextManager;

#[async_trait]
impl ContextManager for TruncatingContextManager {
    async fn prepare(
        &self,
        messages: &mut Vec<Message>,
        state: &mut AgentState,
    ) {
        // Reset the truncation flag every call — its semantic is
        // "this call removed something", not "any call ever
        // removed something".
        state.last_truncated = false;
        state.estimated_tokens = self.estimate_tokens(messages);

        // Already fits. No work to do; the manager returns
        // silently and the loop proceeds to the provider.
        if state.estimated_tokens <= state.context_window {
            return;
        }

        // Partition into leading system prefix + mutable tail.
        // ReActLoop always injects the assembled system prompt
        // first, so this prefix is guaranteed non-empty under
        // production code paths. We `debug_assert!` rather than
        // `return` so a malformed caller surfaces as a test
        // failure instead of silent bypass.
        let mut tail: Vec<Message> = Vec::with_capacity(messages.len());
        let mut sys_prefix: Vec<Message> = Vec::new();
        let mut in_system = true;
        for msg in messages.drain(..) {
            if in_system && msg.role == Role::System {
                sys_prefix.push(msg);
            } else {
                in_system = false;
                tail.push(msg);
            }
        }
        debug_assert!(
            !sys_prefix.is_empty(),
            "ReActLoop must inject at least one System message before ContextManager::prepare"
        );

        // Pairwise drop. The cursor `i` advances only when the
        // current slot is not a user message we can evict, so we
        // never re-evaluate an already-visited slot.
        //
        // The running total is maintained by *subtracting* what a drop
        // removed instead of re-estimating the whole tail after every
        // pair. The estimator is a sum over messages
        // (`estimate_messages_token_count`), so subtracting a dropped
        // message's own estimate is exactly equal to re-summing the
        // survivors — and the previous shape was quadratic: on a
        // 200-message history that does not fit the window it re-walked
        // (and re-serialised every tool call in) the tail once per
        // dropped pair. The `hot_paths` benchmark measures it.
        let mut tail_tokens = estimate_messages_token_count(&tail);
        // The entry condition is the *whole-list* estimate (`state` was
        // filled above), while every later iteration compares the
        // tail-only sum. That hybrid is deliberate and pinned by
        // `truncating_never_drops_system_messages`: a system prompt that
        // alone exceeds the window must still let the tail be evicted
        // rather than short-circuiting the loop before it drops anything.
        let mut over_budget = state.estimated_tokens;
        let mut i = 0;
        while over_budget > state.context_window && i < tail.len() {
            if tail[i].role == Role::User {
                // Drop the user turn. Re-check before deciding
                // whether the following assistant turn also has
                // to go (the assistant turn is the answer to this
                // question; keeping it without the question is
                // incoherent).
                let dropped = tail.remove(i);
                tail_tokens =
                    tail_tokens.saturating_sub(estimate_messages_token_count(
                        std::slice::from_ref(&dropped),
                    ));
                over_budget = tail_tokens;
                state.last_truncated = true;

                if i < tail.len() && tail[i].role == Role::Assistant {
                    let dropped = tail.remove(i);
                    tail_tokens = tail_tokens.saturating_sub(
                        estimate_messages_token_count(std::slice::from_ref(
                            &dropped,
                        )),
                    );
                    over_budget = tail_tokens;
                }
                // Cursor does not advance: the next element has
                // shifted into the slot we just freed.
            } else {
                i += 1;
            }
        }

        messages.extend(sys_prefix);
        messages.extend(tail);
        state.estimated_tokens = self.estimate_tokens(messages);
    }

    /// Arc-shared variant: when the input already fits, return it
    /// unchanged so the cache policy's `Arc::ptr_eq` shortcut
    /// survives. Only materialises a fresh `Arc` when the
    /// pairwise drop actually fires.
    async fn prepare_arc(
        &self,
        messages: Arc<Vec<Message>>,
        state: &mut AgentState,
    ) -> Arc<Vec<Message>> {
        state.last_truncated = false;
        state.estimated_tokens = self.estimate_tokens(&messages);

        if state.estimated_tokens <= state.context_window {
            return messages;
        }

        // Truncation needed. Materialise a fresh Vec, hand off to
        // the legacy `prepare` (which already does pairwise drop
        // and final token re-estimation), and re-wrap.
        match Arc::try_unwrap(messages) {
            Ok(mut owned) => {
                self.prepare(&mut owned, state).await;
                Arc::new(owned)
            }
            Err(shared) => {
                let mut copy = shared.as_ref().clone();
                self.prepare(&mut copy, state).await;
                Arc::new(copy)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use synthia_provider::{Content, ContentPart, TextContent};

    use super::*;

    fn sys(text: &str) -> Message {
        Message::new(Role::System, Content::text(text))
    }

    fn user(text: &str) -> Message {
        Message::new(Role::User, Content::text(text))
    }

    fn assistant(text: &str) -> Message {
        Message::new(Role::Assistant, Content::text(text))
    }

    fn tool_result(id: &str, text: &str) -> Message {
        Message::new(
            Role::Tool,
            Content::Single(ContentPart::ToolResult(
                synthia_provider::ToolResult {
                    tool_use_id: id.to_string(),
                    tool_name: None,
                    content: vec![ContentPart::Text(TextContent {
                        text: text.to_string(),
                        cache_control: None,
                    })],
                    structured_content: None,
                    is_error: None,
                    metadata: serde_json::Map::new(),
                    truncated_by: None,
                },
            )),
        )
    }
    #[tokio::test]
    async fn truncating_drops_oldest_user_assistant_pair() {
        // Token math (4 ASCII bytes ≈ 1 token + 5% overhead):
        // - sys: 21 chars → 5 + 0 = 5 tokens
        // - one user+assistant pair of 80 chars each → 20 + 1 + 20 + 1 = 42 tokens
        // A 50-token window drops exactly one pair: 5 + 42 = 47 ≤ 50,
        // so the second pair survives.
        let mut msgs = vec![
            sys("you are synthia, helpful"),
            user(&"q1".repeat(40)),      // 80 chars
            assistant(&"a1".repeat(40)), // 80 chars
            user(&"q2".repeat(40)),      // 80 chars
            assistant(&"a2".repeat(40)), // 80 chars
        ];
        let mut state = AgentState::with_window(50);

        TruncatingContextManager
            .prepare(&mut msgs, &mut state)
            .await;

        assert!(state.last_truncated, "flag must reflect the eviction");
        assert!(state.estimated_tokens <= state.context_window);
        // System + second user/assistant pair survives.
        assert_eq!(msgs.len(), 3);
        assert_eq!(msgs[0].role, Role::System);
        assert_eq!(msgs[1].role, Role::User);
        assert_eq!(msgs[2].role, Role::Assistant);
    }

    #[tokio::test]
    async fn truncating_never_drops_system_messages() {
        // System prompt alone already exceeds the window; the
        // manager must still keep it and report overflow.
        let mut msgs = vec![sys(&"S".repeat(200)), user("q")];
        let mut state = AgentState::with_window(10);

        TruncatingContextManager
            .prepare(&mut msgs, &mut state)
            .await;

        assert_eq!(msgs[0].role, Role::System);
        assert!(state.estimated_tokens > state.context_window);
        assert!(
            state.last_truncated,
            "user message must still be dropped even when the system prompt itself overflows"
        );
    }

    #[tokio::test]
    async fn truncating_leaves_under_limit_history_untouched() {
        let mut msgs =
            vec![sys("you are synthia"), user("hi"), assistant("hello")];
        let before = msgs.len();
        let mut state = AgentState::with_window(1_000_000);

        TruncatingContextManager
            .prepare(&mut msgs, &mut state)
            .await;

        assert_eq!(msgs.len(), before, "no eviction when window is huge");
        assert!(!state.last_truncated);
        assert!(state.estimated_tokens <= state.context_window);
    }

    #[tokio::test]
    async fn truncating_keeps_orphaned_tool_results_when_no_assistant_follows()
    {
        // A trailing tool_result without a subsequent assistant
        // message (e.g. session was cancelled mid-tool) should be
        // preserved — the manager never invents user/assistant
        // pairs out of tool messages.
        let mut msgs = vec![
            sys("sys"),
            user("do thing"),
            assistant("doing"),
            tool_result("call-1", "result"),
        ];
        let mut state = AgentState::with_window(1_000_000);

        TruncatingContextManager
            .prepare(&mut msgs, &mut state)
            .await;

        assert_eq!(msgs.len(), 4);
        assert_eq!(msgs[3].role, Role::Tool);
    }

    #[tokio::test]
    async fn noop_leaves_messages_alone() {
        // Inputs sized so the estimator returns >0: 50-char system
        // prompt → 12 tokens + 0 overhead = 12.
        let mut msgs = vec![
            sys(&"S".repeat(50)),
            user(&"U".repeat(50)),
            assistant(&"A".repeat(50)),
        ];
        let before = msgs.clone();
        let mut state = AgentState::with_window(10);

        NoopContextManager.prepare(&mut msgs, &mut state).await;

        assert_eq!(msgs, before);
        assert!(!state.last_truncated);
        assert!(state.estimated_tokens > 0);
    }

    #[test]
    fn estimate_tokens_matches_helper_for_text_only() {
        // The trait's default `estimate_tokens` delegates to the
        // shared helper; this test pins that delegation so a
        // refactor of the helper cannot silently change the
        // behaviour every manager inherits.
        let msgs = vec![sys("system prompt here")];
        let via_trait = NoopContextManager.estimate_tokens(&msgs);
        let via_helper =
            synthia_provider::token_counter::estimate_messages_token_count(
                &msgs,
            );
        assert_eq!(via_trait, via_helper);
    }

    #[test]
    fn agent_state_utilisation_handles_zero_window() {
        let s = AgentState::with_window(0);
        assert_eq!(s.context_utilization(), 0.0);
    }

    #[test]
    fn agent_state_from_config_copies_window() {
        let cfg = ModelConfig {
            name: "test-model".to_string(),
            provider: "openai".to_string(),
            context_window: 128_000,
            max_output_tokens: 4096,
            supports_tools: true,
            supports_streaming: true,
            supports_reasoning: false,
        };
        let s = AgentState::from_config(&cfg);
        assert_eq!(s.context_window, 128_000);
        assert_eq!(s.estimated_tokens, 0);
        assert!(!s.last_truncated);
        // Steering fields start neutral for every run.
        assert_eq!(s.total_tokens, 0);
        assert_eq!(s.tool_call_count, 0);
        assert!(s.recent_tool_fingerprints.is_empty());
    }

    /// `record_tool_call` MUST bump the counter, append the
    /// fingerprint, and cap the window at [`FINGERPRINT_WINDOW`]
    /// (oldest entries evicted first).
    #[test]
    fn record_tool_call_caps_fingerprint_window() {
        let mut s = AgentState::with_window(1000);
        for i in 0..(FINGERPRINT_WINDOW + 4) {
            s.record_tool_call(format!("call-{i}"));
        }
        assert_eq!(s.tool_call_count, FINGERPRINT_WINDOW + 4);
        assert_eq!(s.recent_tool_fingerprints.len(), FINGERPRINT_WINDOW);
        assert_eq!(
            s.recent_tool_fingerprints.first().map(String::as_str),
            Some("call-4")
        );
    }

    /// `identical_tail_run` MUST count the trailing run of
    /// identical fingerprints and report 0 on an empty window.
    #[test]
    fn identical_tail_run_counts_trailing_duplicates() {
        let mut s = AgentState::with_window(1000);
        assert_eq!(s.identical_tail_run(), 0);
        s.record_tool_call("a".to_string());
        s.record_tool_call("b".to_string());
        s.record_tool_call("b".to_string());
        s.record_tool_call("b".to_string());
        assert_eq!(s.identical_tail_run(), 3);
    }

    /// `add_token_usage` MUST accumulate totals across responses
    /// and saturate instead of overflowing.
    #[test]
    fn add_token_usage_accumulates_and_saturates() {
        let mut s = AgentState::with_window(1000);
        s.add_token_usage(&TokenUsage {
            prompt_tokens: 100,
            completion_tokens: 50,
            total_tokens: 150,
            cached_prompt_tokens: None,
            cache_read_tokens: Some(30),
            cache_write_tokens: Some(10),
            reasoning_tokens: None,
        });
        s.add_token_usage(&TokenUsage {
            prompt_tokens: 200,
            completion_tokens: 10,
            total_tokens: 210,
            cached_prompt_tokens: None,
            cache_read_tokens: None,
            cache_write_tokens: None,
            reasoning_tokens: Some(7),
        });
        assert_eq!(s.total_tokens, 360);
        // The meter carries the per-category split.
        assert_eq!(s.usage_meter.input_tokens, 300);
        assert_eq!(s.usage_meter.output_tokens, 60);
        assert_eq!(s.usage_meter.cache_read_tokens, 30);
        assert_eq!(s.usage_meter.cache_write_tokens, 10);
        assert_eq!(s.usage_meter.reasoning_tokens, 7);
        assert_eq!(s.usage_meter.calls, 2);
        assert!((s.usage_meter.cache_hit_ratio() - 0.1).abs() < 1e-6);
        s.add_token_usage(&TokenUsage {
            prompt_tokens: usize::MAX,
            completion_tokens: usize::MAX,
            total_tokens: usize::MAX,
            cached_prompt_tokens: None,
            cache_read_tokens: None,
            cache_write_tokens: None,
            reasoning_tokens: None,
        });
        assert_eq!(s.total_tokens, usize::MAX);
        assert_eq!(s.usage_meter.input_tokens, usize::MAX);
        assert_eq!(s.total_tokens, usize::MAX);
        assert_eq!(s.usage_meter.input_tokens, usize::MAX);
    }

    #[tokio::test]
    async fn noop_prepare_arc_returns_input_arc_unchanged() {
        // R6-6 contract: when the manager does not need to mutate
        // the list, it must hand the input `Arc` back so the
        // provider-side cache policy's `Arc::ptr_eq` shortcut can
        // detect "no change since last request".
        let manager = NoopContextManager;
        let mut state = AgentState::with_window(1_000);
        let msgs: Vec<Message> = vec![sys("sys"), user("hi")];
        let arc_in = Arc::new(msgs);
        let arc_out =
            manager.prepare_arc(Arc::clone(&arc_in), &mut state).await;
        // Same allocation — `Arc::ptr_eq` would short-circuit on this.
        assert!(Arc::ptr_eq(&arc_in, &arc_out));
        assert!(!state.last_truncated);
    }

    #[tokio::test]
    async fn truncating_prepare_arc_returns_input_when_no_truncation_needed() {
        let manager = TruncatingContextManager;
        let mut state = AgentState::with_window(1_000);
        // Tiny list — well under the window.
        let arc_in = Arc::new(vec![sys("sys"), user("hi")]);
        let arc_out =
            manager.prepare_arc(Arc::clone(&arc_in), &mut state).await;
        assert!(Arc::ptr_eq(&arc_in, &arc_out));
        assert!(!state.last_truncated);
    }

    #[tokio::test]
    async fn truncating_prepare_arc_returns_new_arc_when_truncation_fires() {
        let manager = TruncatingContextManager;
        let mut state = AgentState::with_window(50);
        // Build a list that overflows the window so pairwise drop
        // has to fire.
        let mut msgs = vec![sys("sys")];
        for i in 0..20 {
            msgs.push(user(&format!(
                "question {i} with extra padding to push tokens up"
            )));
            msgs.push(assistant(&format!(
                "answer {i} with extra padding to push tokens up"
            )));
        }
        let arc_in = Arc::new(msgs);
        let arc_out = manager.prepare_arc(arc_in.clone(), &mut state).await;
        // Truncation happened: a fresh allocation must back the result.
        assert!(!Arc::ptr_eq(&arc_in, &arc_out));
        assert!(state.last_truncated);
        assert!(state.estimated_tokens <= state.context_window);
    }

    #[tokio::test]
    async fn prepare_arc_copies_when_input_arc_is_shared() {
        // Default `prepare_arc` path: when the input Arc is shared
        // (e.g. cache policy still holds a ref), the manager must
        // materialise a fresh copy and apply `prepare` rather than
        // panicking on `Arc::try_unwrap` failure.
        let manager = TruncatingContextManager;
        let mut state = AgentState::with_window(1_000);
        let arc_a = Arc::new(vec![sys("sys"), user("hi")]);
        // Hold a second strong reference so `Arc::try_unwrap` fails.
        let _arc_b = Arc::clone(&arc_a);
        let arc_out = manager.prepare_arc(arc_a, &mut state).await;
        // Untouched because the list fits — the input was kept alive.
        assert_eq!(arc_out.len(), 2);
    }
}
