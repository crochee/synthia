//! Tool extension points: 9 typed hook points for the tool execution
//! pipeline. The first three (`execute.before`, `execute.after`,
//! `definition.transform`) are fully implemented; the remaining 6 are
//! scaffolded for forward-compat (the registry entry points exist but
//! are not yet called from the orchestrator).
//!
//! # Design
//!
//! - **`Action<Output>` return type**: the three implemented points return
//!   an enum that lets the handler either pass through the value
//!   (`Proceed`), substitute it (`Modify`), or short-circuit it
//!   (`Skip { reason }`). This is the only way to give hooks data-flow
//!   influence without breaking the rest of the design.
//! - **Synchronous dispatch**: tool-extension handlers must complete
//!   before the tool call continues (a `Skip` may deny the call).
//!   Asynchronous hooks are explicitly out of scope.
//! - **Per-tool vs all-tools**: handlers can register for a specific tool
//!   (e.g. `"bash"`) or for all tools (use `*` as the tool name).
//!
//! # Implemented points
//!
//! | Name | Purpose |
//! |------|---------|
//! | `tool.execute.before` | Transform or skip the call arguments |
//! | `tool.execute.after` | Transform the tool output |
//! | `tool.definition.transform` | Rewrite name/description/schema |
//!
//! # Scaffolded points (forward-compat)
//!
//! | Name | Purpose |
//! |------|---------|
//! | `tool.registry.register` | Side-effect when a tool is registered |
//! | `tool.registry.unregister` | Side-effect when a tool is unregistered |
//! | `tool.execution_mode.override` | Override Sequential/Parallel |
//! | `tool.parallelism.barrier` | Serialize a group of tool calls |
//! | `tool.output.format` | Reformat the output ContentPart |
//! | `tool.output.metadata.inject` | Add metadata to the output |

use std::sync::Arc;

use dashmap::DashMap;
use serde::{Deserialize, Serialize};

/// Decision a tool-extension handler can return.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub enum Action<T> {
    /// Pass through the original value unchanged.
    #[default]
    Proceed,
    /// Replace the value with `value`.
    Modify(T),
    /// Skip the operation. `reason` is recorded in OTel + the tool output.
    Skip { reason: String },
}

/// `before` event payload — arguments going into a tool call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeforeToolCall {
    pub tool_name: String,
    pub arguments: serde_json::Value,
}

/// `after` event payload — output coming out of a tool call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AfterToolCall {
    pub tool_name: String,
    pub output: serde_json::Value,
    pub is_error: bool,
}

/// `definition.transform` event payload — the tool's metadata as exposed
/// to the LLM. Handlers can rewrite name/description/schema in flight.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinitionView {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// Handler for `tool.execute.before`. Returns an `Action<BeforeToolCall>`.
pub type BeforeHandler =
    Arc<dyn Fn(&BeforeToolCall) -> Action<BeforeToolCall> + Send + Sync>;

/// Handler for `tool.execute.after`. Returns an `Action<AfterToolCall>`.
pub type AfterHandler =
    Arc<dyn Fn(&AfterToolCall) -> Action<AfterToolCall> + Send + Sync>;

/// Handler for `tool.definition.transform`. Returns an `Action<ToolDefinitionView>`.
pub type DefinitionHandler = Arc<
    dyn Fn(&ToolDefinitionView) -> Action<ToolDefinitionView> + Send + Sync,
>;

/// Registry for tool extension points.
pub struct ToolExtensionRegistry {
    before: DashMap<String, Vec<BeforeHandler>>,
    after: DashMap<String, Vec<AfterHandler>>,
    definition: DashMap<String, Vec<DefinitionHandler>>,
    /// Tracks which (point, tool_name) keys are wired so the orchestrator
    /// can check whether calling `fire_*` is necessary.
    active_keys: DashMap<String, ()>,
}

impl std::fmt::Debug for ToolExtensionRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolExtensionRegistry")
            .field("before", &self.before.len())
            .field("after", &self.after.len())
            .field("definition", &self.definition.len())
            .finish()
    }
}

impl Default for ToolExtensionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolExtensionRegistry {
    pub fn new() -> Self {
        Self {
            before: DashMap::new(),
            after: DashMap::new(),
            definition: DashMap::new(),
            active_keys: DashMap::new(),
        }
    }

    /// `tool_name` = `*` matches every tool. Otherwise it matches exactly.
    fn wildcard_keys(tool_name: &str) -> Vec<String> {
        if tool_name == "*" {
            vec!["*".to_string()]
        } else {
            vec![tool_name.to_string(), "*".to_string()]
        }
    }

    pub fn register_before(&self, tool_name: &str, handler: BeforeHandler) {
        let key = tool_name.to_string();
        self.before.entry(key.clone()).or_default().push(handler);
        self.active_keys.insert(format!("before:{}", key), ());
    }

    pub fn register_after(&self, tool_name: &str, handler: AfterHandler) {
        let key = tool_name.to_string();
        self.after.entry(key.clone()).or_default().push(handler);
        self.active_keys.insert(format!("after:{}", key), ());
    }

    pub fn register_definition(
        &self,
        tool_name: &str,
        handler: DefinitionHandler,
    ) {
        let key = tool_name.to_string();
        self.definition
            .entry(key.clone())
            .or_default()
            .push(handler);
        self.active_keys.insert(format!("definition:{}", key), ());
    }

    /// `true` if any handler is registered for `point` and `tool_name`
    /// (considering wildcards).
    pub fn has_handlers(&self, point: &str, tool_name: &str) -> bool {
        for k in Self::wildcard_keys(tool_name) {
            if self.active_keys.contains_key(&format!("{}:{}", point, k)) {
                return true;
            }
        }
        false
    }

    /// Fire the `tool.execute.before` chain. Returns the final
    /// `Action<BeforeToolCall>`.
    ///
    /// Emits a `tracing::info_span!` per dispatched handler with
    /// `point = "tool.execute.before"`, `scope = "tool"`, and
    /// `extension_id = handler_idx` so OTel consumers can attribute
    /// each handler fire to the specific extension (P9 observability
    /// requirement). The span is a no-op without the `otel` feature.
    pub fn fire_before(
        &self,
        mut event: BeforeToolCall,
    ) -> Action<BeforeToolCall> {
        for key in Self::wildcard_keys(&event.tool_name) {
            if let Some(handlers) = self.before.get(&key) {
                for (idx, handler) in handlers.value().iter().enumerate() {
                    let extension_id = format!("{}#{}", key, idx);
                    let _span = tracing::info_span!(
                        target: "synthia.extension",
                        "extension.hook",
                        point = "tool.execute.before",
                        scope = "tool",
                        extension_id = extension_id.as_str(),
                        tool_name = event.tool_name.as_str(),
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
        }
        Action::Modify(event)
    }

    /// Fire the `tool.execute.after` chain. Returns the final
    /// `Action<AfterToolCall>`.
    ///
    /// Emits a `tracing::info_span!` per dispatched handler with
    /// `point = "tool.execute.after"`, `scope = "tool"`, and
    /// `extension_id = handler_idx`.
    pub fn fire_after(
        &self,
        mut event: AfterToolCall,
    ) -> Action<AfterToolCall> {
        for key in Self::wildcard_keys(&event.tool_name) {
            if let Some(handlers) = self.after.get(&key) {
                for (idx, handler) in handlers.value().iter().enumerate() {
                    let extension_id = format!("{}#{}", key, idx);
                    let _span = tracing::info_span!(
                        target: "synthia.extension",
                        "extension.hook",
                        point = "tool.execute.after",
                        scope = "tool",
                        extension_id = extension_id.as_str(),
                        tool_name = event.tool_name.as_str(),
                        is_error = event.is_error,
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
        }
        Action::Modify(event)
    }

    /// Fire the `tool.definition.transform` chain. Returns the final
    /// `Action<ToolDefinitionView>`.
    ///
    /// Emits a `tracing::info_span!` per dispatched handler with
    /// `point = "tool.definition.transform"`, `scope = "tool"`, and
    /// `extension_id = handler_idx`.
    pub fn fire_definition(
        &self,
        mut view: ToolDefinitionView,
    ) -> Action<ToolDefinitionView> {
        for key in Self::wildcard_keys(&view.name) {
            if let Some(handlers) = self.definition.get(&key) {
                for (idx, handler) in handlers.value().iter().enumerate() {
                    let extension_id = format!("{}#{}", key, idx);
                    let _span = tracing::info_span!(
                        target: "synthia.extension",
                        "extension.hook",
                        point = "tool.definition.transform",
                        scope = "tool",
                        extension_id = extension_id.as_str(),
                        tool_name = view.name.as_str(),
                    )
                    .entered();
                    match handler(&view) {
                        Action::Proceed => {}
                        Action::Modify(replacement) => {
                            view = replacement;
                        }
                        Action::Skip { reason } => {
                            return Action::Skip { reason };
                        }
                    }
                }
            }
        }
        Action::Modify(view)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    fn make_before() -> (BeforeHandler, Arc<AtomicUsize>) {
        let calls = Arc::new(AtomicUsize::new(0));
        let c = calls.clone();
        let h: BeforeHandler = Arc::new(move |ev| {
            c.fetch_add(1, Ordering::SeqCst);
            Action::Modify(BeforeToolCall {
                tool_name: ev.tool_name.clone(),
                arguments: serde_json::json!({ "rewritten": true }),
            })
        });
        (h, calls)
    }

    fn make_after() -> (AfterHandler, Arc<AtomicUsize>) {
        let calls = Arc::new(AtomicUsize::new(0));
        let c = calls.clone();
        let h: AfterHandler = Arc::new(move |ev| {
            c.fetch_add(1, Ordering::SeqCst);
            Action::Modify(AfterToolCall {
                tool_name: ev.tool_name.clone(),
                output: serde_json::json!({ "wrapped": ev.output.clone() }),
                is_error: ev.is_error,
            })
        });
        (h, calls)
    }

    fn make_defn() -> (DefinitionHandler, Arc<AtomicUsize>) {
        let calls = Arc::new(AtomicUsize::new(0));
        let c = calls.clone();
        let h: DefinitionHandler = Arc::new(move |v| {
            c.fetch_add(1, Ordering::SeqCst);
            Action::Modify(ToolDefinitionView {
                name: format!("{}_v2", v.name),
                description: v.description.clone(),
                parameters: v.parameters.clone(),
            })
        });
        (h, calls)
    }

    #[test]
    fn new_registry_is_empty() {
        let reg = ToolExtensionRegistry::new();
        assert!(!reg.has_handlers("before", "bash"));
        assert!(!reg.has_handlers("after", "bash"));
        assert!(!reg.has_handlers("definition", "bash"));
    }

    #[test]
    fn before_handler_modifies_arguments() {
        let reg = ToolExtensionRegistry::new();
        let (h, calls) = make_before();
        reg.register_before("bash", h);

        let result = reg.fire_before(BeforeToolCall {
            tool_name: "bash".to_string(),
            arguments: serde_json::json!({"cmd": "ls"}),
        });
        match result {
            Action::Modify(ev) => {
                assert_eq!(
                    ev.arguments,
                    serde_json::json!({"rewritten": true})
                );
            }
            _ => panic!("expected Modify"),
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn after_handler_modifies_output() {
        let reg = ToolExtensionRegistry::new();
        let (h, _) = make_after();
        reg.register_after("read_file", h);
        let result = reg.fire_after(AfterToolCall {
            tool_name: "read_file".to_string(),
            output: serde_json::json!("hello"),
            is_error: false,
        });
        if let Action::Modify(ev) = result {
            assert_eq!(ev.output, serde_json::json!({"wrapped": "hello"}));
        } else {
            panic!("expected Modify");
        }
    }

    #[test]
    fn definition_handler_rewrites_name() {
        let reg = ToolExtensionRegistry::new();
        let (h, _) = make_defn();
        reg.register_definition("bash", h);
        let result = reg.fire_definition(ToolDefinitionView {
            name: "bash".to_string(),
            description: "run shell".to_string(),
            parameters: serde_json::json!({}),
        });
        if let Action::Modify(v) = result {
            assert_eq!(v.name, "bash_v2");
        } else {
            panic!("expected Modify");
        }
    }

    #[test]
    fn wildcard_handler_matches_every_tool() {
        let reg = ToolExtensionRegistry::new();
        let (h, calls) = make_before();
        reg.register_before("*", h);

        reg.fire_before(BeforeToolCall {
            tool_name: "any_tool".to_string(),
            arguments: serde_json::json!({}),
        });
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(reg.has_handlers("before", "any_tool"));
    }

    #[test]
    fn skip_short_circuits_the_chain() {
        let reg = ToolExtensionRegistry::new();
        let skipper: BeforeHandler = Arc::new(|_| Action::Skip {
            reason: "policy denied".to_string(),
        });
        let (modifier, calls) = make_before();
        reg.register_before("bash", skipper);
        reg.register_before("bash", modifier);

        let result = reg.fire_before(BeforeToolCall {
            tool_name: "bash".to_string(),
            arguments: serde_json::json!({}),
        });
        match result {
            Action::Skip { reason } => assert_eq!(reason, "policy denied"),
            _ => panic!("expected Skip"),
        }
        // Modifier should not have run because the skipper short-circuited.
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn multiple_modifiers_apply_in_registration_order() {
        let reg = ToolExtensionRegistry::new();
        let h1: BeforeHandler = Arc::new(|ev| {
            Action::Modify(BeforeToolCall {
                tool_name: ev.tool_name.clone(),
                arguments: serde_json::json!({ "step": 1 }),
            })
        });
        let h2: BeforeHandler = Arc::new(|ev| {
            Action::Modify(BeforeToolCall {
                tool_name: ev.tool_name.clone(),
                arguments: serde_json::json!({ "step": 2 }),
            })
        });
        reg.register_before("bash", h1);
        reg.register_before("bash", h2);
        let result = reg.fire_before(BeforeToolCall {
            tool_name: "bash".to_string(),
            arguments: serde_json::json!({}),
        });
        if let Action::Modify(ev) = result {
            assert_eq!(ev.arguments, serde_json::json!({ "step": 2 }));
        } else {
            panic!("expected Modify");
        }
    }

    #[test]
    fn has_handlers_distinguishes_specific_vs_wildcard() {
        let reg = ToolExtensionRegistry::new();
        let (h, _) = make_before();
        reg.register_before("bash", h);
        assert!(reg.has_handlers("before", "bash"));
        assert!(!reg.has_handlers("before", "read_file"));
    }

    #[test]
    fn fire_with_no_handlers_returns_proceed() {
        let reg = ToolExtensionRegistry::new();
        let result = reg.fire_before(BeforeToolCall {
            tool_name: "nope".to_string(),
            arguments: serde_json::json!({}),
        });
        // No handlers means the event passes through unchanged.
        if let Action::Modify(ev) = result {
            assert_eq!(ev.tool_name, "nope");
        } else {
            panic!("expected Modify with original event");
        }
    }

    // --- Phase 3.4: concurrent dispatch tests (DashMap is internally synchronized) ---

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_register_before_does_not_lose_handlers() {
        let reg = std::sync::Arc::new(ToolExtensionRegistry::new());
        let counter =
            std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let mut handles = Vec::new();
        for _ in 0..64 {
            let reg = reg.clone();
            let counter = counter.clone();
            handles.push(tokio::spawn(async move {
                let counter = counter.clone();
                let h: BeforeHandler = std::sync::Arc::new(move |_ev| {
                    counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    Action::Proceed
                });
                reg.register_before("bash", h);
            }));
        }
        for h in handles {
            h.await.unwrap();
        }
        assert!(reg.has_handlers("before", "bash"));

        reg.fire_before(BeforeToolCall {
            tool_name: "bash".to_string(),
            arguments: serde_json::json!({}),
        });
        assert_eq!(counter.load(std::sync::atomic::Ordering::SeqCst), 64);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_fire_before_is_thread_safe() {
        let reg = std::sync::Arc::new(ToolExtensionRegistry::new());
        let counter =
            std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter_clone = counter.clone();
        let h: BeforeHandler = std::sync::Arc::new(move |ev| {
            counter_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Action::Modify(BeforeToolCall {
                tool_name: ev.tool_name.clone(),
                arguments: serde_json::json!({ "wrapped": true }),
            })
        });
        reg.register_before("bash", h);

        let mut handles = Vec::new();
        for _ in 0..32 {
            let reg = reg.clone();
            handles.push(tokio::spawn(async move {
                for _ in 0..16 {
                    reg.fire_before(BeforeToolCall {
                        tool_name: "bash".to_string(),
                        arguments: serde_json::json!({}),
                    });
                }
            }));
        }
        for h in handles {
            h.await.unwrap();
        }
        // 32 tasks * 16 fires = 512 deliveries.
        assert_eq!(counter.load(std::sync::atomic::Ordering::SeqCst), 512);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_register_and_fire_does_not_deadlock() {
        // Mixed workload: some tasks register, some fire — DashMap
        // must support this without deadlocking.
        let reg = std::sync::Arc::new(ToolExtensionRegistry::new());

        let mut handles = Vec::new();
        for i in 0..8 {
            let reg = reg.clone();
            handles.push(tokio::spawn(async move {
                for j in 0..8 {
                    let h: BeforeHandler = std::sync::Arc::new(move |ev| {
                        Action::Modify(BeforeToolCall {
                            tool_name: ev.tool_name.clone(),
                            arguments: serde_json::json!({
                                "r": i,
                                "h": j,
                            }),
                        })
                    });
                    reg.register_before("bash", h);
                }
            }));
        }
        for _ in 0..8 {
            let reg = reg.clone();
            handles.push(tokio::spawn(async move {
                for _ in 0..16 {
                    reg.fire_before(BeforeToolCall {
                        tool_name: "bash".to_string(),
                        arguments: serde_json::json!({}),
                    });
                }
            }));
        }
        for h in handles {
            h.await.unwrap();
        }
        // 8 register tasks * 8 handlers each = 64 registered; no panics.
        assert!(reg.has_handlers("before", "bash"));
    }
}
