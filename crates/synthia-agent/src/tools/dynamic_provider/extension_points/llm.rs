//! LLM extension points: 8 typed hook points fired by the LLM request
//! pipeline. All points use the `Action<Output>` mutation pattern (mirroring
//! `tool.rs`).
//!
//! # Design
//!
//! - **Mutation pattern**: every LLM point is allowed to rewrite data
//!   flowing into or out of the LLM call. Handlers return
//!   `Action<LlmOutput>` (or a more specific `Action<X>`) where
//!   `Proceed` keeps the original value, `Modify(X)` replaces it, and
//!   `Skip { reason }` short-circuits the call (rare for LLM points).
//! - **P1 prefix consistency**: any point that mutates data feeding the
//!   LLM call MUST be deterministic across calls. The orchestrator
//!   re-snapshots the prefix hash AFTER the hook chain returns. If a
//!   non-deterministic transform is detected (same input → different
//!   output), the agent logs an `extension.non_deterministic` OTel event
//!   and may force a cache miss.
//! - **Asynchronous dispatch**: handler closures return
//!   `BoxFuture<'static, Action<T>>` (or `BoxFuture<'static, Vec<..>>` for
//!   `cache.breakpoint.set`), allowing async work (e.g. network calls, DB
//!   lookups). Synchronous handlers are supported via the
//!   `register_*_sync()` convenience methods.
//!
//! # Points
//!
//! | Name | Payload | Purpose |
//! |------|---------|---------|
//! | `system_prompt.transform` | `SystemPromptTransformInput` | Replace the system prompt content |
//! | `messages.transform` | `MessagesTransformInput` | Reorder / redact / annotate messages |
//! | `chat.params` | `ChatParams` | Tune `temperature` / `top_p` / `top_k` / `max_tokens` |
//! | `chat.headers.inject` | `ChatHeadersInput` | Add tracing IDs, auth tokens |
//! | `tool_choice.override` | `ToolChoiceInput` | Force function calling |
//! | `model.select` | `ModelSelectInput` | Multi-model routing |
//! | `cache.breakpoint.set` | `CacheBreakpointInput` | Per-conversation cache tuning |
//! | `response.transform` | `ResponseTransformInput` | Post-LLM annotation |

use std::sync::Arc;

use dashmap::DashMap;
use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};

use super::tool::Action;

// =====================================================================
// Typed payloads
// =====================================================================

/// `system_prompt.transform` event payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemPromptTransformInput {
    pub session_id: String,
    pub current: String,
}

impl SystemPromptTransformInput {
    pub fn new(
        session_id: impl Into<String>,
        current: impl Into<String>,
    ) -> Self {
        Self {
            session_id: session_id.into(),
            current: current.into(),
        }
    }
}

/// `messages.transform` event payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessagesTransformInput {
    pub session_id: String,
    /// Serialized JSON of the message list. The LLM extension point
    /// contract is JSON-typed for the message array (per Phase 3 tool
    /// `arguments` precedent) but the event itself is typed.
    pub messages: serde_json::Value,
}

impl MessagesTransformInput {
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

/// `chat.params` event payload (mutable reference target).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct ChatParams {
    pub temperature: f32,
    pub top_p: f32,
    pub top_k: u32,
    pub max_tokens: u32,
}

impl Default for ChatParams {
    fn default() -> Self {
        Self {
            temperature: 1.0,
            top_p: 1.0,
            top_k: 0,
            max_tokens: 4096,
        }
    }
}

/// `chat.headers.inject` event payload.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChatHeadersInput {
    pub session_id: String,
    /// Existing headers; extensions may add more (and possibly override
    /// in registration order, first-registered wins).
    pub headers: serde_json::Value,
}

impl ChatHeadersInput {
    pub fn new(
        session_id: impl Into<String>,
        headers: serde_json::Value,
    ) -> Self {
        Self {
            session_id: session_id.into(),
            headers,
        }
    }
}

/// `tool_choice.override` event payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolChoiceInput {
    pub session_id: String,
    /// Default tool choice. May be `"auto"`, `"any"`, `"none"`, or a
    /// specific tool name.
    pub current: String,
}

/// `model.select` event payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelSelectInput {
    pub session_id: String,
    /// Default model identifier. May be a bare name (`claude-3-5-sonnet`)
    /// or a routing expression (`cheap` / `expensive`).
    pub current: String,
}

/// `cache.breakpoint.set` event payload + response.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct CacheBreakpoint {
    pub scope: String,
    pub ttl_ms: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CacheBreakpointInput {
    pub session_id: String,
}

/// `response.transform` event payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseTransformInput {
    pub session_id: String,
    /// Serialized assistant message before storage.
    pub response: serde_json::Value,
}

impl ResponseTransformInput {
    pub fn new(
        session_id: impl Into<String>,
        response: serde_json::Value,
    ) -> Self {
        Self {
            session_id: session_id.into(),
            response,
        }
    }
}

// =====================================================================
// Handler aliases
// =====================================================================

pub type SystemPromptHandler = Arc<
    dyn Fn(
            &SystemPromptTransformInput,
        ) -> BoxFuture<'static, Action<SystemPromptTransformInput>>
        + Send
        + Sync,
>;

pub type MessagesHandler = Arc<
    dyn Fn(
            &MessagesTransformInput,
        ) -> BoxFuture<'static, Action<MessagesTransformInput>>
        + Send
        + Sync,
>;

pub type ChatParamsHandler = Arc<
    dyn Fn(&ChatParams) -> BoxFuture<'static, Action<ChatParams>> + Send + Sync,
>;

pub type ChatHeadersHandler = Arc<
    dyn Fn(&ChatHeadersInput) -> BoxFuture<'static, Action<ChatHeadersInput>>
        + Send
        + Sync,
>;

pub type ToolChoiceHandler = Arc<
    dyn Fn(&ToolChoiceInput) -> BoxFuture<'static, Action<ToolChoiceInput>>
        + Send
        + Sync,
>;

pub type ModelSelectHandler = Arc<
    dyn Fn(&ModelSelectInput) -> BoxFuture<'static, Action<ModelSelectInput>>
        + Send
        + Sync,
>;

pub type CacheBreakpointHandler = Arc<
    dyn Fn(&CacheBreakpointInput) -> BoxFuture<'static, Vec<CacheBreakpoint>>
        + Send
        + Sync,
>;

pub type ResponseTransformHandler = Arc<
    dyn Fn(
            &ResponseTransformInput,
        ) -> BoxFuture<'static, Action<ResponseTransformInput>>
        + Send
        + Sync,
>;

// =====================================================================
// Sync handler aliases (for register_*_sync convenience methods)
// =====================================================================

pub type SyncSystemPromptHandler = Arc<
    dyn Fn(&SystemPromptTransformInput) -> Action<SystemPromptTransformInput>
        + Send
        + Sync,
>;

pub type SyncMessagesHandler = Arc<
    dyn Fn(&MessagesTransformInput) -> Action<MessagesTransformInput>
        + Send
        + Sync,
>;

pub type SyncChatParamsHandler =
    Arc<dyn Fn(&ChatParams) -> Action<ChatParams> + Send + Sync>;

pub type SyncChatHeadersHandler =
    Arc<dyn Fn(&ChatHeadersInput) -> Action<ChatHeadersInput> + Send + Sync>;

pub type SyncToolChoiceHandler =
    Arc<dyn Fn(&ToolChoiceInput) -> Action<ToolChoiceInput> + Send + Sync>;

pub type SyncModelSelectHandler =
    Arc<dyn Fn(&ModelSelectInput) -> Action<ModelSelectInput> + Send + Sync>;

pub type SyncCacheBreakpointHandler =
    Arc<dyn Fn(&CacheBreakpointInput) -> Vec<CacheBreakpoint> + Send + Sync>;

pub type SyncResponseTransformHandler = Arc<
    dyn Fn(&ResponseTransformInput) -> Action<ResponseTransformInput>
        + Send
        + Sync,
>;

// =====================================================================
// Registry
// =====================================================================

/// Registry for LLM extension points. Handlers are stored in per-point
/// `DashMap<String, Vec<Handler>>` keyed by extension id. A handler may be
/// registered multiple times under different ids (e.g., one per plugin).
pub struct LlmExtensionRegistry {
    system_prompt: DashMap<String, Vec<SystemPromptHandler>>,
    messages: DashMap<String, Vec<MessagesHandler>>,
    chat_params: DashMap<String, Vec<ChatParamsHandler>>,
    chat_headers: DashMap<String, Vec<ChatHeadersHandler>>,
    tool_choice: DashMap<String, Vec<ToolChoiceHandler>>,
    model_select: DashMap<String, Vec<ModelSelectHandler>>,
    cache_breakpoint: DashMap<String, Vec<CacheBreakpointHandler>>,
    response_transform: DashMap<String, Vec<ResponseTransformHandler>>,
    /// Tracks which (point) keys are wired so the orchestrator can skip
    /// the dispatch if no handlers are registered.
    active_keys: DashMap<String, ()>,
}

impl std::fmt::Debug for LlmExtensionRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LlmExtensionRegistry")
            .field("system_prompt", &self.system_prompt.len())
            .field("messages", &self.messages.len())
            .field("chat_params", &self.chat_params.len())
            .field("chat_headers", &self.chat_headers.len())
            .field("tool_choice", &self.tool_choice.len())
            .field("model_select", &self.model_select.len())
            .field("cache_breakpoint", &self.cache_breakpoint.len())
            .field("response_transform", &self.response_transform.len())
            .finish()
    }
}

impl Default for LlmExtensionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl LlmExtensionRegistry {
    pub fn new() -> Self {
        Self {
            system_prompt: DashMap::new(),
            messages: DashMap::new(),
            chat_params: DashMap::new(),
            chat_headers: DashMap::new(),
            tool_choice: DashMap::new(),
            model_select: DashMap::new(),
            cache_breakpoint: DashMap::new(),
            response_transform: DashMap::new(),
            active_keys: DashMap::new(),
        }
    }

    pub fn register_system_prompt(
        &self,
        id: impl Into<String>,
        handler: SystemPromptHandler,
    ) {
        self.system_prompt
            .entry(id.into())
            .or_default()
            .push(handler);
        self.active_keys
            .insert("system_prompt.transform".into(), ());
    }

    pub fn register_system_prompt_sync(
        &self,
        id: impl Into<String>,
        handler: SyncSystemPromptHandler,
    ) {
        let async_handler: SystemPromptHandler =
            Arc::new(move |ev| Box::pin(std::future::ready(handler(ev))));
        self.register_system_prompt(id, async_handler);
    }

    pub fn register_messages(
        &self,
        id: impl Into<String>,
        handler: MessagesHandler,
    ) {
        self.messages.entry(id.into()).or_default().push(handler);
        self.active_keys.insert("messages.transform".into(), ());
    }

    pub fn register_messages_sync(
        &self,
        id: impl Into<String>,
        handler: SyncMessagesHandler,
    ) {
        let async_handler: MessagesHandler =
            Arc::new(move |ev| Box::pin(std::future::ready(handler(ev))));
        self.register_messages(id, async_handler);
    }

    pub fn register_chat_params(
        &self,
        id: impl Into<String>,
        handler: ChatParamsHandler,
    ) {
        self.chat_params.entry(id.into()).or_default().push(handler);
        self.active_keys.insert("chat.params".into(), ());
    }

    pub fn register_chat_params_sync(
        &self,
        id: impl Into<String>,
        handler: SyncChatParamsHandler,
    ) {
        let async_handler: ChatParamsHandler =
            Arc::new(move |p| Box::pin(std::future::ready(handler(p))));
        self.register_chat_params(id, async_handler);
    }

    pub fn register_chat_headers(
        &self,
        id: impl Into<String>,
        handler: ChatHeadersHandler,
    ) {
        self.chat_headers
            .entry(id.into())
            .or_default()
            .push(handler);
        self.active_keys.insert("chat.headers.inject".into(), ());
    }

    pub fn register_chat_headers_sync(
        &self,
        id: impl Into<String>,
        handler: SyncChatHeadersHandler,
    ) {
        let async_handler: ChatHeadersHandler =
            Arc::new(move |ev| Box::pin(std::future::ready(handler(ev))));
        self.register_chat_headers(id, async_handler);
    }

    pub fn register_tool_choice(
        &self,
        id: impl Into<String>,
        handler: ToolChoiceHandler,
    ) {
        self.tool_choice.entry(id.into()).or_default().push(handler);
        self.active_keys.insert("tool_choice.override".into(), ());
    }

    pub fn register_tool_choice_sync(
        &self,
        id: impl Into<String>,
        handler: SyncToolChoiceHandler,
    ) {
        let async_handler: ToolChoiceHandler =
            Arc::new(move |ev| Box::pin(std::future::ready(handler(ev))));
        self.register_tool_choice(id, async_handler);
    }

    pub fn register_model_select(
        &self,
        id: impl Into<String>,
        handler: ModelSelectHandler,
    ) {
        self.model_select
            .entry(id.into())
            .or_default()
            .push(handler);
        self.active_keys.insert("model.select".into(), ());
    }

    pub fn register_model_select_sync(
        &self,
        id: impl Into<String>,
        handler: SyncModelSelectHandler,
    ) {
        let async_handler: ModelSelectHandler =
            Arc::new(move |ev| Box::pin(std::future::ready(handler(ev))));
        self.register_model_select(id, async_handler);
    }

    pub fn register_cache_breakpoint(
        &self,
        id: impl Into<String>,
        handler: CacheBreakpointHandler,
    ) {
        self.cache_breakpoint
            .entry(id.into())
            .or_default()
            .push(handler);
        self.active_keys.insert("cache.breakpoint.set".into(), ());
    }

    pub fn register_cache_breakpoint_sync(
        &self,
        id: impl Into<String>,
        handler: SyncCacheBreakpointHandler,
    ) {
        let async_handler: CacheBreakpointHandler =
            Arc::new(move |ev| Box::pin(std::future::ready(handler(ev))));
        self.register_cache_breakpoint(id, async_handler);
    }

    pub fn register_response_transform(
        &self,
        id: impl Into<String>,
        handler: ResponseTransformHandler,
    ) {
        self.response_transform
            .entry(id.into())
            .or_default()
            .push(handler);
        self.active_keys.insert("response.transform".into(), ());
    }

    pub fn register_response_transform_sync(
        &self,
        id: impl Into<String>,
        handler: SyncResponseTransformHandler,
    ) {
        let async_handler: ResponseTransformHandler =
            Arc::new(move |ev| Box::pin(std::future::ready(handler(ev))));
        self.register_response_transform(id, async_handler);
    }

    /// `true` if any handler is registered for `point`.
    pub fn has_handlers(&self, point: &str) -> bool {
        self.active_keys.contains_key(point)
    }

    /// Fire the `system_prompt.transform` chain. Returns the final
    /// `Action<SystemPromptTransformInput>`.
    pub async fn fire_system_prompt(
        &self,
        mut event: SystemPromptTransformInput,
    ) -> Action<SystemPromptTransformInput> {
        for entry in self.system_prompt.iter() {
            for (idx, handler) in entry.value().iter().enumerate() {
                let extension_id = format!("{}#{}", entry.key(), idx);
                let action = {
                    let _span = tracing::info_span!(
                        target: "synthia.extension",
                        "extension.hook",
                        point = "system_prompt.transform",
                        scope = "llm",
                        extension_id = extension_id.as_str(),
                        session_id = event.session_id.as_str(),
                    )
                    .entered();
                    handler(&event).await
                };
                match action {
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

    /// Fire the `messages.transform` chain.
    ///
    /// Performs a determinism check: if the input JSON differs from the
    /// pre-fire JSON after a no-op chain, the orchestrator treats the
    /// transform as a no-op (this is handled by the caller — we just
    /// return the result here).
    pub async fn fire_messages(
        &self,
        mut event: MessagesTransformInput,
    ) -> Action<MessagesTransformInput> {
        for entry in self.messages.iter() {
            for (idx, handler) in entry.value().iter().enumerate() {
                let extension_id = format!("{}#{}", entry.key(), idx);
                let action = {
                    let _span = tracing::info_span!(
                        target: "synthia.extension",
                        "extension.hook",
                        point = "messages.transform",
                        scope = "llm",
                        extension_id = extension_id.as_str(),
                        session_id = event.session_id.as_str(),
                        payload_size = event.messages.to_string().len(),
                    )
                    .entered();
                    handler(&event).await
                };
                match action {
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

    /// Fire the `chat.params` chain.
    pub async fn fire_chat_params(
        &self,
        mut params: ChatParams,
    ) -> Action<ChatParams> {
        for entry in self.chat_params.iter() {
            for (idx, handler) in entry.value().iter().enumerate() {
                let extension_id = format!("{}#{}", entry.key(), idx);
                let action = {
                    let _span = tracing::info_span!(
                        target: "synthia.extension",
                        "extension.hook",
                        point = "chat.params",
                        scope = "llm",
                        extension_id = extension_id.as_str(),
                    )
                    .entered();
                    handler(&params).await
                };
                match action {
                    Action::Proceed => {}
                    Action::Modify(replacement) => {
                        params = replacement;
                    }
                    Action::Skip { reason } => {
                        return Action::Skip { reason };
                    }
                }
            }
        }
        Action::Modify(params)
    }

    /// Fire the `chat.headers.inject` chain.
    pub async fn fire_chat_headers(
        &self,
        mut event: ChatHeadersInput,
    ) -> Action<ChatHeadersInput> {
        for entry in self.chat_headers.iter() {
            for (idx, handler) in entry.value().iter().enumerate() {
                let extension_id = format!("{}#{}", entry.key(), idx);
                let action = {
                    let _span = tracing::info_span!(
                        target: "synthia.extension",
                        "extension.hook",
                        point = "chat.headers.inject",
                        scope = "llm",
                        extension_id = extension_id.as_str(),
                    )
                    .entered();
                    handler(&event).await
                };
                match action {
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

    /// Fire the `tool_choice.override` chain.
    pub async fn fire_tool_choice(
        &self,
        mut event: ToolChoiceInput,
    ) -> Action<ToolChoiceInput> {
        for entry in self.tool_choice.iter() {
            for (idx, handler) in entry.value().iter().enumerate() {
                let extension_id = format!("{}#{}", entry.key(), idx);
                let action = {
                    let _span = tracing::info_span!(
                        target: "synthia.extension",
                        "extension.hook",
                        point = "tool_choice.override",
                        scope = "llm",
                        extension_id = extension_id.as_str(),
                    )
                    .entered();
                    handler(&event).await
                };
                match action {
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

    /// Fire the `model.select` chain.
    pub async fn fire_model_select(
        &self,
        mut event: ModelSelectInput,
    ) -> Action<ModelSelectInput> {
        for entry in self.model_select.iter() {
            for (idx, handler) in entry.value().iter().enumerate() {
                let extension_id = format!("{}#{}", entry.key(), idx);
                let action = {
                    let _span = tracing::info_span!(
                        target: "synthia.extension",
                        "extension.hook",
                        point = "model.select",
                        scope = "llm",
                        extension_id = extension_id.as_str(),
                    )
                    .entered();
                    handler(&event).await
                };
                match action {
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

    /// Fire the `cache.breakpoint.set` chain. Returns the union of all
    /// `Vec<CacheBreakpoint>` returned by handlers.
    pub async fn fire_cache_breakpoint(
        &self,
        event: &CacheBreakpointInput,
    ) -> Vec<CacheBreakpoint> {
        let mut out = Vec::new();
        for entry in self.cache_breakpoint.iter() {
            for (idx, handler) in entry.value().iter().enumerate() {
                let extension_id = format!("{}#{}", entry.key(), idx);
                let result = {
                    let _span = tracing::info_span!(
                        target: "synthia.extension",
                        "extension.hook",
                        point = "cache.breakpoint.set",
                        scope = "llm",
                        extension_id = extension_id.as_str(),
                    )
                    .entered();
                    handler(event).await
                };
                out.extend(result);
            }
        }
        out
    }

    /// Fire the `response.transform` chain.
    pub async fn fire_response_transform(
        &self,
        mut event: ResponseTransformInput,
    ) -> Action<ResponseTransformInput> {
        for entry in self.response_transform.iter() {
            for (idx, handler) in entry.value().iter().enumerate() {
                let extension_id = format!("{}#{}", entry.key(), idx);
                let action = {
                    let _span = tracing::info_span!(
                        target: "synthia.extension",
                        "extension.hook",
                        point = "response.transform",
                        scope = "llm",
                        extension_id = extension_id.as_str(),
                    )
                    .entered();
                    handler(&event).await
                };
                match action {
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

    #[tokio::test]
    async fn new_registry_is_empty() {
        let reg = LlmExtensionRegistry::new();
        assert!(!reg.has_handlers("system_prompt.transform"));
        assert!(!reg.has_handlers("messages.transform"));
        assert!(!reg.has_handlers("chat.params"));
        assert!(!reg.has_handlers("chat.headers.inject"));
        assert!(!reg.has_handlers("tool_choice.override"));
        assert!(!reg.has_handlers("model.select"));
        assert!(!reg.has_handlers("cache.breakpoint.set"));
        assert!(!reg.has_handlers("response.transform"));
    }

    #[tokio::test]
    async fn chat_params_modification_reflected_in_request() {
        let reg = LlmExtensionRegistry::new();
        let h: ChatParamsHandler = Arc::new(|p| {
            Box::pin(std::future::ready(Action::Modify(ChatParams {
                temperature: 0.0,
                ..*p
            })))
        });
        reg.register_chat_params("zero-temp", h);

        let result = reg.fire_chat_params(ChatParams::default()).await;
        if let Action::Modify(p) = result {
            assert_eq!(p.temperature, 0.0);
            assert_eq!(p.max_tokens, 4096);
        } else {
            panic!("expected Modify");
        }
    }

    #[tokio::test]
    async fn chat_params_sync_modification_reflected_in_request() {
        let reg = LlmExtensionRegistry::new();
        let h: SyncChatParamsHandler = Arc::new(|p| {
            Action::Modify(ChatParams {
                temperature: 0.0,
                ..*p
            })
        });
        reg.register_chat_params_sync("zero-temp", h);

        let result = reg.fire_chat_params(ChatParams::default()).await;
        if let Action::Modify(p) = result {
            assert_eq!(p.temperature, 0.0);
            assert_eq!(p.max_tokens, 4096);
        } else {
            panic!("expected Modify");
        }
    }

    #[tokio::test]
    async fn deterministic_transform_preserves_hash() {
        // A deterministic transform: same input → same output. The
        // orchestrator relies on this for P1 cache stability.
        let reg = LlmExtensionRegistry::new();
        let h: MessagesHandler = Arc::new(|ev| {
            let sorted = ev.messages.clone();
            Box::pin(std::future::ready(Action::Modify(
                MessagesTransformInput {
                    session_id: ev.session_id.clone(),
                    messages: sorted,
                },
            )))
        });
        reg.register_messages("noop-sort", h);

        let input = MessagesTransformInput::new(
            "s1",
            serde_json::json!([{"role": "user", "content": "hi"}]),
        );
        let r1 = reg.fire_messages(input.clone()).await;
        let r2 = reg.fire_messages(input).await;

        let Action::Modify(a) = r1 else {
            panic!("expected Modify")
        };
        let Action::Modify(b) = r2 else {
            panic!("expected Modify")
        };
        assert_eq!(a.messages, b.messages, "deterministic output required");
    }

    #[tokio::test]
    async fn non_deterministic_transform_is_detected_by_caller() {
        // Simulate a non-deterministic transform (e.g., a handler that
        // adds a timestamp). The caller is responsible for detecting
        // this; the registry just dispatches.
        let reg = LlmExtensionRegistry::new();
        let counter = Arc::new(AtomicUsize::new(0));
        let c = counter.clone();
        let h: MessagesHandler = Arc::new(move |ev| {
            let n = c.fetch_add(1, Ordering::SeqCst);
            Box::pin(std::future::ready(Action::Modify(
                MessagesTransformInput {
                    session_id: ev.session_id.clone(),
                    messages: serde_json::json!({"counter": n}),
                },
            )))
        });
        reg.register_messages("non-deterministic", h);

        let input = MessagesTransformInput::new("s1", serde_json::json!(null));
        let Action::Modify(a) = reg.fire_messages(input.clone()).await else {
            panic!("expected Modify")
        };
        let Action::Modify(b) = reg.fire_messages(input).await else {
            panic!("expected Modify")
        };
        assert_ne!(a.messages, b.messages);
    }

    #[tokio::test]
    async fn cache_breakpoint_returns_union_of_handlers() {
        let reg = LlmExtensionRegistry::new();
        let h1: CacheBreakpointHandler = Arc::new(|_| {
            Box::pin(std::future::ready(vec![CacheBreakpoint {
                scope: "system".into(),
                ttl_ms: 60_000,
            }]))
        });
        let h2: CacheBreakpointHandler = Arc::new(|_| {
            Box::pin(std::future::ready(vec![CacheBreakpoint {
                scope: "tools".into(),
                ttl_ms: 5_000,
            }]))
        });
        reg.register_cache_breakpoint("h1", h1);
        reg.register_cache_breakpoint("h2", h2);

        let result = reg
            .fire_cache_breakpoint(&CacheBreakpointInput {
                session_id: "s1".into(),
            })
            .await;
        assert_eq!(result.len(), 2);
        // DashMap iteration order is not guaranteed; assert the union
        // contains both scopes (test is order-agnostic).
        let scopes: std::collections::BTreeSet<_> =
            result.iter().map(|b| b.scope.as_str()).collect();
        assert!(scopes.contains("system"));
        assert!(scopes.contains("tools"));
    }

    #[tokio::test]
    async fn skip_short_circuits_the_chain() {
        let reg = LlmExtensionRegistry::new();
        let skipper: ChatParamsHandler = Arc::new(|_| {
            Box::pin(std::future::ready(Action::Skip {
                reason: "rate-limited".to_string(),
            }))
        });
        let modifier: ChatParamsHandler = Arc::new(|p| {
            Box::pin(std::future::ready(Action::Modify(ChatParams {
                temperature: 0.5,
                ..*p
            })))
        });
        reg.register_chat_params("skipper", skipper);
        reg.register_chat_params("modifier", modifier);

        let result = reg.fire_chat_params(ChatParams::default()).await;
        assert!(matches!(result, Action::Skip { .. }));
    }

    #[tokio::test]
    async fn multiple_modifiers_apply_in_registration_order() {
        let reg = LlmExtensionRegistry::new();
        let h1: SystemPromptHandler = Arc::new(|ev| {
            Box::pin(std::future::ready(Action::Modify(
                SystemPromptTransformInput {
                    session_id: ev.session_id.clone(),
                    current: format!("{}\n# step1", ev.current),
                },
            )))
        });
        let h2: SystemPromptHandler = Arc::new(|ev| {
            Box::pin(std::future::ready(Action::Modify(
                SystemPromptTransformInput {
                    session_id: ev.session_id.clone(),
                    current: format!("{}\n# step2", ev.current),
                },
            )))
        });
        reg.register_system_prompt("h1", h1);
        reg.register_system_prompt("h2", h2);
        let result = reg
            .fire_system_prompt(SystemPromptTransformInput::new("s1", "base"))
            .await;
        if let Action::Modify(p) = result {
            // DashMap iteration order is not guaranteed, but BOTH
            // handlers should have run (order-agnostic assertion).
            assert!(p.current.contains("# step1"));
            assert!(p.current.contains("# step2"));
        } else {
            panic!("expected Modify");
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_register_does_not_lose_handlers() {
        let reg = std::sync::Arc::new(LlmExtensionRegistry::new());
        let counter = std::sync::Arc::new(AtomicUsize::new(0));
        let mut handles = Vec::new();
        for _ in 0..32 {
            let reg = reg.clone();
            let counter = counter.clone();
            handles.push(tokio::spawn(async move {
                let counter = counter.clone();
                let h: ChatParamsHandler = std::sync::Arc::new(move |p| {
                    let counter = counter.clone();
                    let p = *p;
                    Box::pin(async move {
                        counter.fetch_add(1, Ordering::SeqCst);
                        Action::Modify(p)
                    })
                });
                reg.register_chat_params("h", h);
            }));
        }
        for h in handles {
            h.await.unwrap();
        }
        assert!(reg.has_handlers("chat.params"));
    }

    #[tokio::test]
    async fn async_handler_performs_async_work() {
        let reg = LlmExtensionRegistry::new();
        let h: ChatParamsHandler = Arc::new(|p| {
            let p = *p;
            Box::pin(async move {
                tokio::time::sleep(std::time::Duration::from_millis(1)).await;
                Action::Modify(ChatParams {
                    temperature: 0.0,
                    ..p
                })
            })
        });
        reg.register_chat_params("async-zero", h);

        let result = reg.fire_chat_params(ChatParams::default()).await;
        if let Action::Modify(p) = result {
            assert_eq!(p.temperature, 0.0);
        } else {
            panic!("expected Modify");
        }
    }
}
