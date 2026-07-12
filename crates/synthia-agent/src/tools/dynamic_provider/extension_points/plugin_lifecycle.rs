//! Plugin Lifecycle extension points: 6 typed hook points fired by the
//! plugin manager. The lifecycle reuses the existing three-state
//! `ExtensionContext` (Loading/Active/Stale) — no new states are
//! added.
//!
//! # Design
//!
//! - **Reuses `ExtensionContext`**: every transition is a state
//!   machine operation on the context passed in. The state machine
//!   itself is owned by `extension_context.rs` (Phase 3.1).
//! - **OTel observability**: every transition fires an
//!   `extension.hook.<point>` span. State transitions emit
//!   `extension.bind_core` (Loading→Active) or `extension.invalidate`
//!   (Active→Stale) per the existing pattern in
//!   `extension_context.rs`.
//! - **`extension.hot_swap`** is a 3-event sequence:
//!   `load(new) + invalidate(old) + bind(new)`. The orchestrator may
//!   implement this as one fire that invokes all three in order.
//! - **`extension.dual_form`** is a meta point that asks the
//!   extension "should you be exposed as a Tool or an ExtensionTool
//!   for the next LLM call?".
//!
//! # Points
//!
//! | Name | Payload | Purpose |
//! |------|---------|---------|
//! | `extension.load` | `LoadRequest` | Transition to `Loading`, queue registrations |
//! | `extension.bind` | `BindRequest` | Transition to `Active`, flush queue |
//! | `extension.invalidate` | `InvalidateRequest` | Transition to `Stale`, retain `last_active` |
//! | `extension.unload` | `UnloadRequest` | Drop `last_active`, fully unload |
//! | `extension.hot_swap` | `HotSwapRequest` | 3-event sequence: load (new) + invalidate (old) + bind (new) |
//! | `extension.dual_form` | `DualFormQuery` → `DualFormResponse` | Tool vs ExtensionTool preference |

use std::sync::Arc;

use dashmap::DashMap;
use serde::{Deserialize, Serialize};

use super::{super::extension_context::ExtensionContext, tool::Action};

// =====================================================================
// Typed payloads
// =====================================================================

/// `extension.load` event payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadRequest {
    pub session_id: String,
    pub extension_id: String,
}

impl LoadRequest {
    pub fn new(
        session_id: impl Into<String>,
        extension_id: impl Into<String>,
    ) -> Self {
        Self {
            session_id: session_id.into(),
            extension_id: extension_id.into(),
        }
    }
}

/// `extension.bind` event payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BindRequest {
    pub session_id: String,
    pub extension_id: String,
}

impl BindRequest {
    pub fn new(
        session_id: impl Into<String>,
        extension_id: impl Into<String>,
    ) -> Self {
        Self {
            session_id: session_id.into(),
            extension_id: extension_id.into(),
        }
    }
}

/// `extension.invalidate` event payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvalidateRequest {
    pub session_id: String,
    pub extension_id: String,
    pub reason: String,
}

impl InvalidateRequest {
    pub fn new(
        session_id: impl Into<String>,
        extension_id: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            session_id: session_id.into(),
            extension_id: extension_id.into(),
            reason: reason.into(),
        }
    }
}

/// `extension.unload` event payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnloadRequest {
    pub session_id: String,
    pub extension_id: String,
}

impl UnloadRequest {
    pub fn new(
        session_id: impl Into<String>,
        extension_id: impl Into<String>,
    ) -> Self {
        Self {
            session_id: session_id.into(),
            extension_id: extension_id.into(),
        }
    }
}

/// `extension.hot_swap` event payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HotSwapRequest {
    pub session_id: String,
    pub old_extension_id: String,
    pub new_extension_id: String,
}

impl HotSwapRequest {
    pub fn new(
        session_id: impl Into<String>,
        old_extension_id: impl Into<String>,
        new_extension_id: impl Into<String>,
    ) -> Self {
        Self {
            session_id: session_id.into(),
            old_extension_id: old_extension_id.into(),
            new_extension_id: new_extension_id.into(),
        }
    }
}

/// `extension.dual_form` event payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DualFormQuery {
    pub session_id: String,
    pub extension_id: String,
    /// Caller's preferred form.
    pub prefer: DualForm,
}

impl DualFormQuery {
    pub fn new(
        session_id: impl Into<String>,
        extension_id: impl Into<String>,
        prefer: DualForm,
    ) -> Self {
        Self {
            session_id: session_id.into(),
            extension_id: extension_id.into(),
            prefer,
        }
    }
}

/// Form selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DualForm {
    Tool,
    ExtensionTool,
}

/// `extension.dual_form` event response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DualFormResponse {
    pub form: DualForm,
    pub reason: String,
}

impl DualFormResponse {
    pub fn new(form: DualForm, reason: impl Into<String>) -> Self {
        Self {
            form,
            reason: reason.into(),
        }
    }
}

// =====================================================================
// Handler aliases
// =====================================================================

/// `extension.load` handler. Returns the new `ExtensionContext`
/// (should be in `Loading` state). The handler may also register
/// tools on the context before returning.
pub type LoadHandler =
    Arc<dyn Fn(&LoadRequest) -> Action<ExtensionContext> + Send + Sync>;

/// `extension.bind` handler. Returns the new `ExtensionContext`
/// (should be in `Active` state). The handler should call
/// `bind_core` internally.
pub type BindHandler = Arc<
    dyn Fn(&BindRequest, &ExtensionContext) -> Result<ExtensionContext, String>
        + Send
        + Sync,
>;

/// `extension.invalidate` handler. Returns the new `ExtensionContext`
/// (should be in `Stale` state). The handler should retain
/// `last_active` for diagnostics.
pub type InvalidateHandler = Arc<
    dyn Fn(
            &InvalidateRequest,
            &ExtensionContext,
        ) -> Result<ExtensionContext, String>
        + Send
        + Sync,
>;

/// `extension.unload` handler. Returns the dropped
/// `ExtensionContext` (caller is responsible for the drop).
pub type UnloadHandler =
    Arc<dyn Fn(&UnloadRequest, &ExtensionContext) + Send + Sync>;

/// `extension.hot_swap` handler. Returns the new
/// `ExtensionContext` for the new extension (in `Active` state). The
/// old context is invalidated inside the handler.
pub type HotSwapHandler = Arc<
    dyn Fn(
            &HotSwapRequest,
            &ExtensionContext,
        ) -> Result<ExtensionContext, String>
        + Send
        + Sync,
>;

pub type DualFormHandler =
    Arc<dyn Fn(&DualFormQuery) -> Action<DualFormResponse> + Send + Sync>;

// =====================================================================
// Registry
// =====================================================================

pub struct PluginLifecycleExtensionRegistry {
    load: DashMap<String, Vec<LoadHandler>>,
    bind: DashMap<String, Vec<BindHandler>>,
    invalidate: DashMap<String, Vec<InvalidateHandler>>,
    unload: DashMap<String, Vec<UnloadHandler>>,
    hot_swap: DashMap<String, Vec<HotSwapHandler>>,
    dual_form: DashMap<String, Vec<DualFormHandler>>,
    active_keys: DashMap<String, ()>,
}

impl std::fmt::Debug for PluginLifecycleExtensionRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PluginLifecycleExtensionRegistry")
            .field("load", &self.load.len())
            .field("bind", &self.bind.len())
            .field("invalidate", &self.invalidate.len())
            .field("unload", &self.unload.len())
            .field("hot_swap", &self.hot_swap.len())
            .field("dual_form", &self.dual_form.len())
            .finish()
    }
}

impl Default for PluginLifecycleExtensionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl PluginLifecycleExtensionRegistry {
    pub fn new() -> Self {
        Self {
            load: DashMap::new(),
            bind: DashMap::new(),
            invalidate: DashMap::new(),
            unload: DashMap::new(),
            hot_swap: DashMap::new(),
            dual_form: DashMap::new(),
            active_keys: DashMap::new(),
        }
    }

    pub fn register_load(&self, id: impl Into<String>, handler: LoadHandler) {
        self.load.entry(id.into()).or_default().push(handler);
        self.active_keys.insert("extension.load".into(), ());
    }

    pub fn register_bind(&self, id: impl Into<String>, handler: BindHandler) {
        self.bind.entry(id.into()).or_default().push(handler);
        self.active_keys.insert("extension.bind".into(), ());
    }

    pub fn register_invalidate(
        &self,
        id: impl Into<String>,
        handler: InvalidateHandler,
    ) {
        self.invalidate.entry(id.into()).or_default().push(handler);
        self.active_keys.insert("extension.invalidate".into(), ());
    }

    pub fn register_unload(
        &self,
        id: impl Into<String>,
        handler: UnloadHandler,
    ) {
        self.unload.entry(id.into()).or_default().push(handler);
        self.active_keys.insert("extension.unload".into(), ());
    }

    pub fn register_hot_swap(
        &self,
        id: impl Into<String>,
        handler: HotSwapHandler,
    ) {
        self.hot_swap.entry(id.into()).or_default().push(handler);
        self.active_keys.insert("extension.hot_swap".into(), ());
    }

    pub fn register_dual_form(
        &self,
        id: impl Into<String>,
        handler: DualFormHandler,
    ) {
        self.dual_form.entry(id.into()).or_default().push(handler);
        self.active_keys.insert("extension.dual_form".into(), ());
    }

    pub fn has_handlers(&self, point: &str) -> bool {
        self.active_keys.contains_key(point)
    }

    /// Fire `extension.load`. The chain runs in registration order;
    /// the final `Action::Modify(ctx)` is the new context. Handlers
    /// may register tools on the context before returning.
    pub fn fire_load(&self, req: &LoadRequest) -> Action<ExtensionContext> {
        let mut ctx = ExtensionContext::new_loading(req.session_id.clone());
        for entry in self.load.iter() {
            for (idx, handler) in entry.value().iter().enumerate() {
                let extension_id = format!("{}#{}", entry.key(), idx);
                let _span = tracing::info_span!(
                    target: "synthia.extension",
                    "extension.hook",
                    point = "extension.load",
                    scope = "plugin_lifecycle",
                    extension_id = extension_id.as_str(),
                    session_id = req.session_id.as_str(),
                )
                .entered();
                match handler(req) {
                    Action::Proceed => {}
                    Action::Modify(replacement) => {
                        ctx = replacement;
                    }
                    Action::Skip { reason } => {
                        return Action::Skip { reason };
                    }
                }
            }
        }
        Action::Modify(ctx)
    }

    /// Fire `extension.bind`. The chain runs in registration order;
    /// the first handler that returns `Ok(ctx)` wins. The handler is
    /// expected to call `ExtensionContext::bind_core` internally.
    pub fn fire_bind(
        &self,
        req: &BindRequest,
        ctx: ExtensionContext,
    ) -> Result<ExtensionContext, String> {
        for entry in self.bind.iter() {
            for (idx, handler) in entry.value().iter().enumerate() {
                let extension_id = format!("{}#{}", entry.key(), idx);
                let _span = tracing::info_span!(
                    target: "synthia.extension",
                    "extension.hook",
                    point = "extension.bind",
                    scope = "plugin_lifecycle",
                    extension_id = extension_id.as_str(),
                    session_id = req.session_id.as_str(),
                )
                .entered();
                match handler(req, &ctx) {
                    Ok(bound) => return Ok(bound),
                    Err(e) => {
                        tracing::warn!(
                            target: "synthia.extension",
                            extension_id = extension_id.as_str(),
                            "bind handler failed: {}",
                            e,
                        );
                    }
                }
            }
        }
        Err("no bind handler succeeded".to_string())
    }

    /// Fire `extension.invalidate`. The chain runs in registration
    /// order; the first handler that returns `Ok(ctx)` wins.
    pub fn fire_invalidate(
        &self,
        req: &InvalidateRequest,
        ctx: ExtensionContext,
    ) -> Result<ExtensionContext, String> {
        for entry in self.invalidate.iter() {
            for (idx, handler) in entry.value().iter().enumerate() {
                let extension_id = format!("{}#{}", entry.key(), idx);
                let _span = tracing::info_span!(
                    target: "synthia.extension",
                    "extension.hook",
                    point = "extension.invalidate",
                    scope = "plugin_lifecycle",
                    extension_id = extension_id.as_str(),
                    session_id = req.session_id.as_str(),
                )
                .entered();
                match handler(req, &ctx) {
                    Ok(invalidated) => return Ok(invalidated),
                    Err(e) => {
                        tracing::warn!(
                            target: "synthia.extension",
                            extension_id = extension_id.as_str(),
                            "invalidate handler failed: {}",
                            e,
                        );
                    }
                }
            }
        }
        Err("no invalidate handler succeeded".to_string())
    }

    /// Fire `extension.unload`. The chain runs in registration order;
    /// all handlers are invoked (idempotent terminal operation).
    pub fn fire_unload(&self, req: &UnloadRequest, ctx: ExtensionContext) {
        for entry in self.unload.iter() {
            for (idx, handler) in entry.value().iter().enumerate() {
                let extension_id = format!("{}#{}", entry.key(), idx);
                let _span = tracing::info_span!(
                    target: "synthia.extension",
                    "extension.hook",
                    point = "extension.unload",
                    scope = "plugin_lifecycle",
                    extension_id = extension_id.as_str(),
                    session_id = req.session_id.as_str(),
                )
                .entered();
                handler(req, &ctx);
            }
        }
    }

    /// Fire `extension.hot_swap` (3-event sequence: load new →
    /// invalidate old → bind new). The handler is given the OLD
    /// context and is expected to invalidate it internally and return
    /// the NEW (active) context.
    pub fn fire_hot_swap(
        &self,
        req: &HotSwapRequest,
        old_ctx: ExtensionContext,
    ) -> Result<ExtensionContext, String> {
        for entry in self.hot_swap.iter() {
            if let Some((idx, handler)) =
                entry.value().iter().enumerate().next()
            {
                let extension_id = format!("{}#{}", entry.key(), idx);
                let _span = tracing::info_span!(
                    target: "synthia.extension",
                    "extension.hook",
                    point = "extension.hot_swap",
                    scope = "plugin_lifecycle",
                    extension_id = extension_id.as_str(),
                    session_id = req.session_id.as_str(),
                )
                .entered();
                let new_ctx = handler(req, &old_ctx)?;
                tracing::info!(
                    target: "synthia.extension",
                    extension_id = extension_id.as_str(),
                    old = req.old_extension_id.as_str(),
                    new = req.new_extension_id.as_str(),
                    "extension.hot_swap_completed"
                );
                return Ok(new_ctx);
            }
        }
        Err("no hot_swap handler registered".to_string())
    }

    /// Fire `extension.dual_form`. Returns the final
    /// `DualFormResponse` (mutation pattern). The first `Skip` or
    /// `Modify` short-circuits the chain.
    pub fn fire_dual_form(
        &self,
        query: &DualFormQuery,
    ) -> Action<DualFormResponse> {
        for entry in self.dual_form.iter() {
            for (idx, handler) in entry.value().iter().enumerate() {
                let extension_id = format!("{}#{}", entry.key(), idx);
                let _span = tracing::info_span!(
                    target: "synthia.extension",
                    "extension.hook",
                    point = "extension.dual_form",
                    scope = "plugin_lifecycle",
                    extension_id = extension_id.as_str(),
                    session_id = query.session_id.as_str(),
                )
                .entered();
                match handler(query) {
                    Action::Proceed => {}
                    Action::Modify(replacement) => {
                        return Action::Modify(replacement);
                    }
                    Action::Skip { reason } => {
                        return Action::Skip { reason };
                    }
                }
            }
        }
        // Default: honor the caller's preference.
        Action::Modify(DualFormResponse::new(
            query.prefer,
            "default-preference",
        ))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicUsize;

    use super::*;

    #[test]
    fn new_registry_is_empty() {
        let reg = PluginLifecycleExtensionRegistry::new();
        assert!(!reg.has_handlers("extension.load"));
        assert!(!reg.has_handlers("extension.bind"));
        assert!(!reg.has_handlers("extension.invalidate"));
        assert!(!reg.has_handlers("extension.unload"));
        assert!(!reg.has_handlers("extension.hot_swap"));
        assert!(!reg.has_handlers("extension.dual_form"));
    }

    #[test]
    fn fire_load_returns_loading_context() {
        let reg = PluginLifecycleExtensionRegistry::new();
        let req = LoadRequest::new("s1", "plugin-1");
        let Action::Modify(ctx) = reg.fire_load(&req) else {
            panic!("expected Modify")
        };
        assert!(ctx.is_loading());
    }

    #[test]
    fn fire_bind_transitions_to_active() {
        let reg = PluginLifecycleExtensionRegistry::new();
        // Register a bind handler that calls bind_core on a fresh
        // context seeded from the input session_id.
        let h: BindHandler = Arc::new(|req, _ctx| {
            ExtensionContext::new_loading(req.session_id.clone())
                .bind_core()
                .map_err(|e| format!("{:?}", e))
        });
        reg.register_bind("core", h);
        let req = BindRequest::new("s1", "plugin-1");
        let ctx = ExtensionContext::new_loading("s1");
        let bound = reg.fire_bind(&req, ctx).unwrap();
        assert!(bound.is_active());
    }

    #[test]
    fn fire_invalidate_transitions_to_stale() {
        let reg = PluginLifecycleExtensionRegistry::new();
        let h: InvalidateHandler = Arc::new(|req, _ctx| {
            Ok(ExtensionContext::Stale {
                reason: req.reason.clone(),
                last_active: None,
            })
        });
        reg.register_invalidate("core", h);
        let req = InvalidateRequest::new("s1", "plugin-1", "shutdown");
        let ctx = ExtensionContext::new_loading("s1");
        let invalidated = reg.fire_invalidate(&req, ctx).unwrap();
        assert!(invalidated.is_stale());
    }

    #[test]
    fn fire_dual_form_returns_default_preference() {
        let reg = PluginLifecycleExtensionRegistry::new();
        let query = DualFormQuery::new("s1", "plugin-1", DualForm::Tool);
        let Action::Modify(resp) = reg.fire_dual_form(&query) else {
            panic!("expected Modify")
        };
        assert_eq!(resp.form, DualForm::Tool);
    }

    #[test]
    fn fire_dual_form_honors_handler() {
        let reg = PluginLifecycleExtensionRegistry::new();
        let h: DualFormHandler = Arc::new(|_q| {
            Action::Modify(DualFormResponse::new(
                DualForm::ExtensionTool,
                "context-aware",
            ))
        });
        reg.register_dual_form("smart", h);
        let query = DualFormQuery::new("s1", "plugin-1", DualForm::Tool);
        let Action::Modify(resp) = reg.fire_dual_form(&query) else {
            panic!("expected Modify")
        };
        assert_eq!(resp.form, DualForm::ExtensionTool);
    }

    #[test]
    fn state_machine_integrity_under_100_iterations() {
        let reg = PluginLifecycleExtensionRegistry::new();
        let h_load: LoadHandler = Arc::new(|_req| {
            Action::Modify(ExtensionContext::new_loading("s1"))
        });
        let h_bind: BindHandler = Arc::new(|req, _ctx| {
            ExtensionContext::new_loading(req.session_id.clone())
                .bind_core()
                .map_err(|e| format!("{:?}", e))
        });
        let h_invalidate: InvalidateHandler = Arc::new(|_req, _ctx| {
            Ok(ExtensionContext::Stale {
                reason: "test".to_string(),
                last_active: None,
            })
        });
        let unload_count = Arc::new(AtomicUsize::new(0));
        let uc = unload_count.clone();
        let h_unload: UnloadHandler = Arc::new(move |_req, _ctx| {
            uc.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        });
        reg.register_load("core", h_load);
        reg.register_bind("core", h_bind);
        reg.register_invalidate("core", h_invalidate);
        reg.register_unload("core", h_unload);

        for _ in 0..100 {
            let load_req = LoadRequest::new("s1", "plugin-1");
            let Action::Modify(ctx) = reg.fire_load(&load_req) else {
                panic!("expected Modify")
            };
            assert!(ctx.is_loading());
            let bind_req = BindRequest::new("s1", "plugin-1");
            let ctx = reg.fire_bind(&bind_req, ctx).unwrap();
            assert!(ctx.is_active());
            let inv_req = InvalidateRequest::new("s1", "plugin-1", "test");
            let ctx = reg.fire_invalidate(&inv_req, ctx).unwrap();
            assert!(ctx.is_stale());
            let unload_req = UnloadRequest::new("s1", "plugin-1");
            reg.fire_unload(&unload_req, ctx);
        }
        assert_eq!(unload_count.load(std::sync::atomic::Ordering::SeqCst), 100);
    }

    #[test]
    fn hot_swap_transitions_through_valid_states() {
        let reg = PluginLifecycleExtensionRegistry::new();
        let h: HotSwapHandler = Arc::new(|req, _old_ctx| {
            // Invalidate old (drop)
            // Create new loading context and bind it
            ExtensionContext::new_loading(req.session_id.clone())
                .bind_core()
                .map_err(|e| format!("{:?}", e))
        });
        reg.register_hot_swap("core", h);
        let req = HotSwapRequest::new("s1", "old-plugin", "new-plugin");
        let old_ctx = ExtensionContext::new_loading("s1");
        let new_ctx = reg.fire_hot_swap(&req, old_ctx).unwrap();
        assert!(new_ctx.is_active());
    }
}
