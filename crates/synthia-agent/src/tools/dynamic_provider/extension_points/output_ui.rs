//! Output/UI extension points: 4 typed hook points fired by the
//! output pipeline (text formatting, metadata injection, dialog
//! requests, and rich component rendering).
//!
//! # Design
//!
//! - **Mutation pattern**: most points are mutation-style (rewriting
//!   content, injecting metadata, or producing a `RenderOutput`).
//! - **P1 prefix consistency**: `output.format` is part of the
//!   rendered user view (not the LLM context), so it does not
//!   interact with the prefix hash. The doc comment notes this
//!   explicitly.
//! - **Host capability mapping**: the registry records the host
//!   type (TUI / RPC / Server); the `fire_render_component` method
//!   consults the host's `supports_kind` table to decide whether to
//!   dispatch the render or fall back to plain text.
//! - **Dialog requests**: `ui.dialog.confirm` is the only blocking
//!   point; `notify` is non-blocking. `select` and `input` are
//!   request-style and may block on the host's UI.
//!
//! # Points
//!
//! | Name | Payload | Purpose |
//! |------|---------|---------|
//! | `output.format` | `OutputFormatInput` | Rewrite user-facing text |
//! | `output.metadata.inject` | `MetadataPatch` | Add structured fields |
//! | `ui.dialog.{select, confirm, input, notify}` | `DialogRequest` | Typed dialog requests |
//! | `ui.render.component` | `RenderRequest` → `RenderOutput` | Render a typed widget |

use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

use dashmap::DashMap;
use serde::{Deserialize, Serialize};

use super::tool::Action;

// =====================================================================
// Typed payloads
// =====================================================================

/// MIME type (string, e.g. "text/plain", "application/json").
pub type MimeType = String;

/// Audience selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Audience {
    User,
    Internal,
    System,
}

/// `output.format` event payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputFormatInput {
    pub session_id: String,
    pub content: String,
    pub mime: MimeType,
    pub audience: Audience,
}

impl OutputFormatInput {
    pub fn new(
        session_id: impl Into<String>,
        content: impl Into<String>,
        mime: impl Into<String>,
        audience: Audience,
    ) -> Self {
        Self {
            session_id: session_id.into(),
            content: content.into(),
            mime: mime.into(),
            audience,
        }
    }
}

/// `output.metadata.inject` event response.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct MetadataPatch {
    pub fields: BTreeMap<String, MetadataValue>,
}

impl MetadataPatch {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, key: impl Into<String>, value: MetadataValue) {
        self.fields.insert(key.into(), value);
    }
}

/// A single metadata value.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum MetadataValue {
    String(String),
    Number(f64),
    Integer(i64),
    Boolean(bool),
    Null,
}

impl From<&str> for MetadataValue {
    fn from(s: &str) -> Self {
        Self::String(s.to_string())
    }
}

impl From<String> for MetadataValue {
    fn from(s: String) -> Self {
        Self::String(s)
    }
}

impl From<i64> for MetadataValue {
    fn from(n: i64) -> Self {
        Self::Integer(n)
    }
}

impl From<bool> for MetadataValue {
    fn from(b: bool) -> Self {
        Self::Boolean(b)
    }
}

/// `ui.dialog.notify` event payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotifyRequest {
    pub session_id: String,
    pub message: String,
    pub level: NotificationLevel,
}

impl NotifyRequest {
    pub fn new(
        session_id: impl Into<String>,
        message: impl Into<String>,
        level: NotificationLevel,
    ) -> Self {
        Self {
            session_id: session_id.into(),
            message: message.into(),
            level,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationLevel {
    Info,
    Warning,
    Error,
}

/// `ui.dialog.confirm` event payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfirmRequest {
    pub session_id: String,
    pub prompt: String,
    pub default: bool,
    /// Optional timeout in milliseconds.
    pub timeout_ms: Option<u32>,
}

impl ConfirmRequest {
    pub fn new(
        session_id: impl Into<String>,
        prompt: impl Into<String>,
        default: bool,
    ) -> Self {
        Self {
            session_id: session_id.into(),
            prompt: prompt.into(),
            default,
            timeout_ms: None,
        }
    }
}

/// `ui.render.component` event payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderRequest {
    pub session_id: String,
    pub kind: ComponentKind,
    pub props: serde_json::Value,
}

impl RenderRequest {
    pub fn new(
        session_id: impl Into<String>,
        kind: ComponentKind,
        props: serde_json::Value,
    ) -> Self {
        Self {
            session_id: session_id.into(),
            kind,
            props,
        }
    }
}

/// Typed widget kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComponentKind {
    Text,
    Diff,
    Table,
    Chart,
    Image,
    Code,
}

/// `ui.render.component` event response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderOutput {
    pub component: ComponentKind,
    pub rendered: serde_json::Value,
}

impl RenderOutput {
    pub fn new(component: ComponentKind, rendered: serde_json::Value) -> Self {
        Self {
            component,
            rendered,
        }
    }

    /// Fallback: wrap `content` as a plain-text component.
    pub fn fallback(content: impl Into<String>) -> Self {
        Self {
            component: ComponentKind::Text,
            rendered: serde_json::Value::String(content.into()),
        }
    }
}

/// Host type — TUI / RPC / Server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostKind {
    Tui,
    Rpc,
    Server,
}

impl HostKind {
    /// `true` if this host can render `kind` natively.
    pub fn supports_kind(self, kind: ComponentKind) -> bool {
        match (self, kind) {
            (Self::Tui, ComponentKind::Text) => true,
            (Self::Tui, ComponentKind::Diff) => true,
            (Self::Tui, ComponentKind::Code) => true,
            (Self::Rpc, _) => true, // RPC serializes everything to JSON
            (Self::Server, _) => true, // Server renders HTML/SSR
            _ => false,
        }
    }
}

// =====================================================================
// Handler aliases
// =====================================================================

pub type OutputFormatHandler =
    Arc<dyn Fn(&OutputFormatInput) -> Action<OutputFormatInput> + Send + Sync>;

pub type MetadataInjectHandler = Arc<dyn Fn() -> MetadataPatch + Send + Sync>;

pub type DialogNotifyHandler = Arc<dyn Fn(&NotifyRequest) + Send + Sync>;

pub type DialogConfirmHandler =
    Arc<dyn Fn(&ConfirmRequest) -> bool + Send + Sync>;

pub type RenderComponentHandler =
    Arc<dyn Fn(&RenderRequest) -> Action<RenderOutput> + Send + Sync>;

// =====================================================================
// Registry
// =====================================================================

pub struct OutputUiExtensionRegistry {
    output_format: DashMap<String, Vec<OutputFormatHandler>>,
    metadata_inject: DashMap<String, Vec<MetadataInjectHandler>>,
    /// Insertion order for `metadata_inject` (DashMap iteration order is
    /// non-deterministic, but the spec requires first-registered-wins).
    metadata_inject_order: Mutex<Vec<String>>,
    dialog_notify: DashMap<String, Vec<DialogNotifyHandler>>,
    dialog_confirm: DashMap<String, Vec<DialogConfirmHandler>>,
    render_component: DashMap<String, Vec<RenderComponentHandler>>,
    active_keys: DashMap<String, ()>,
    /// Host kind for render-component capability mapping.
    host: HostKind,
}

impl std::fmt::Debug for OutputUiExtensionRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OutputUiExtensionRegistry")
            .field("output_format", &self.output_format.len())
            .field("metadata_inject", &self.metadata_inject.len())
            .field("dialog_notify", &self.dialog_notify.len())
            .field("dialog_confirm", &self.dialog_confirm.len())
            .field("render_component", &self.render_component.len())
            .field("host", &self.host)
            .finish()
    }
}

impl Default for OutputUiExtensionRegistry {
    fn default() -> Self {
        Self::new(HostKind::Tui)
    }
}

impl OutputUiExtensionRegistry {
    pub fn new(host: HostKind) -> Self {
        Self {
            output_format: DashMap::new(),
            metadata_inject: DashMap::new(),
            metadata_inject_order: Mutex::new(Vec::new()),
            dialog_notify: DashMap::new(),
            dialog_confirm: DashMap::new(),
            render_component: DashMap::new(),
            active_keys: DashMap::new(),
            host,
        }
    }

    pub fn register_output_format(
        &self,
        id: impl Into<String>,
        handler: OutputFormatHandler,
    ) {
        self.output_format
            .entry(id.into())
            .or_default()
            .push(handler);
        self.active_keys.insert("output.format".into(), ());
    }

    pub fn register_metadata_inject(
        &self,
        id: impl Into<String>,
        handler: MetadataInjectHandler,
    ) {
        let id = id.into();
        let mut order = self
            .metadata_inject_order
            .lock()
            .expect("metadata_inject_order mutex poisoned");
        if !self.metadata_inject.contains_key(&id) {
            order.push(id.clone());
        }
        self.metadata_inject.entry(id).or_default().push(handler);
        self.active_keys.insert("output.metadata.inject".into(), ());
    }

    pub fn register_dialog_notify(
        &self,
        id: impl Into<String>,
        handler: DialogNotifyHandler,
    ) {
        self.dialog_notify
            .entry(id.into())
            .or_default()
            .push(handler);
        self.active_keys.insert("ui.dialog.notify".into(), ());
    }

    pub fn register_dialog_confirm(
        &self,
        id: impl Into<String>,
        handler: DialogConfirmHandler,
    ) {
        self.dialog_confirm
            .entry(id.into())
            .or_default()
            .push(handler);
        self.active_keys.insert("ui.dialog.confirm".into(), ());
    }

    pub fn register_render_component(
        &self,
        id: impl Into<String>,
        handler: RenderComponentHandler,
    ) {
        self.render_component
            .entry(id.into())
            .or_default()
            .push(handler);
        self.active_keys.insert("ui.render.component".into(), ());
    }

    pub fn has_handlers(&self, point: &str) -> bool {
        self.active_keys.contains_key(point)
    }

    pub fn host(&self) -> HostKind {
        self.host
    }

    /// Fire `output.format`. The chain runs in registration order;
    /// the final `content` is the rendered user view.
    pub fn fire_output_format(
        &self,
        mut event: OutputFormatInput,
    ) -> Action<OutputFormatInput> {
        for entry in self.output_format.iter() {
            for (idx, handler) in entry.value().iter().enumerate() {
                let extension_id = format!("{}#{}", entry.key(), idx);
                let _span = tracing::info_span!(
                    target: "synthia.extension",
                    "extension.hook",
                    point = "output.format",
                    scope = "output_ui",
                    extension_id = extension_id.as_str(),
                    session_id = event.session_id.as_str(),
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

    /// Fire `output.metadata.inject`. Returns the union of all
    /// `MetadataPatch` patches. First-registered extension wins on
    /// conflicts (per the spec).
    pub fn fire_metadata_inject(&self) -> MetadataPatch {
        let mut out = MetadataPatch::new();
        let order = self
            .metadata_inject_order
            .lock()
            .expect("metadata_inject_order mutex poisoned")
            .clone();
        for id in order {
            if let Some(handlers) = self.metadata_inject.get(&id) {
                for (idx, handler) in handlers.iter().enumerate() {
                    let extension_id = format!("{}#{}", id, idx);
                    let _span = tracing::info_span!(
                        target: "synthia.extension",
                        "extension.hook",
                        point = "output.metadata.inject",
                        scope = "output_ui",
                        extension_id = extension_id.as_str(),
                    )
                    .entered();
                    let patch = handler();
                    for (k, v) in patch.fields {
                        out.fields.entry(k).or_insert(v);
                    }
                }
            }
        }
        out
    }

    /// Fire `ui.dialog.notify` (non-blocking). All handlers are
    /// invoked in registration order.
    pub fn fire_dialog_notify(&self, req: &NotifyRequest) {
        for entry in self.dialog_notify.iter() {
            for (idx, handler) in entry.value().iter().enumerate() {
                let extension_id = format!("{}#{}", entry.key(), idx);
                let _span = tracing::info_span!(
                    target: "synthia.extension",
                    "extension.hook",
                    point = "ui.dialog.notify",
                    scope = "output_ui",
                    extension_id = extension_id.as_str(),
                    session_id = req.session_id.as_str(),
                )
                .entered();
                handler(req);
            }
        }
    }

    /// Fire `ui.dialog.confirm` (blocks for user response). Returns
    /// the first registered handler's response; falls back to
    /// `default` on timeout (timeout handling is the host's job).
    pub fn fire_dialog_confirm(&self, req: &ConfirmRequest) -> bool {
        for entry in self.dialog_confirm.iter() {
            if let Some((idx, handler)) =
                entry.value().iter().enumerate().next()
            {
                let extension_id = format!("{}#{}", entry.key(), idx);
                let _span = tracing::info_span!(
                    target: "synthia.extension",
                    "extension.hook",
                    point = "ui.dialog.confirm",
                    scope = "output_ui",
                    extension_id = extension_id.as_str(),
                    session_id = req.session_id.as_str(),
                )
                .entered();
                return handler(req);
            }
        }
        req.default
    }

    /// Fire `ui.render.component`. If the host does not support
    /// the requested `kind`, falls back to a plain-text
    /// `RenderOutput::fallback` carrying the original `props` as a
    /// string.
    pub fn fire_render_component(
        &self,
        req: &RenderRequest,
    ) -> Action<RenderOutput> {
        if !self.host.supports_kind(req.kind) {
            return Action::Modify(RenderOutput::fallback(
                req.props.to_string(),
            ));
        }
        for entry in self.render_component.iter() {
            for (idx, handler) in entry.value().iter().enumerate() {
                let extension_id = format!("{}#{}", entry.key(), idx);
                let _span = tracing::info_span!(
                    target: "synthia.extension",
                    "extension.hook",
                    point = "ui.render.component",
                    scope = "output_ui",
                    extension_id = extension_id.as_str(),
                    session_id = req.session_id.as_str(),
                )
                .entered();
                match handler(req) {
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
        // Default: wrap props as text.
        Action::Modify(RenderOutput::fallback(req.props.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_registry_is_empty() {
        let reg = OutputUiExtensionRegistry::new(HostKind::Tui);
        assert!(!reg.has_handlers("output.format"));
        assert!(!reg.has_handlers("output.metadata.inject"));
        assert!(!reg.has_handlers("ui.dialog.notify"));
        assert!(!reg.has_handlers("ui.dialog.confirm"));
        assert!(!reg.has_handlers("ui.render.component"));
    }

    #[test]
    fn tui_renders_text_and_diff() {
        let reg = OutputUiExtensionRegistry::new(HostKind::Tui);
        assert!(reg.host().supports_kind(ComponentKind::Text));
        assert!(reg.host().supports_kind(ComponentKind::Diff));
    }

    #[test]
    fn unsupported_kind_falls_back_to_string() {
        let reg = OutputUiExtensionRegistry::new(HostKind::Tui);
        let req = RenderRequest::new(
            "s1",
            ComponentKind::Chart,
            serde_json::json!({"values": [1, 2, 3]}),
        );
        let Action::Modify(out) = reg.fire_render_component(&req) else {
            panic!("expected Modify")
        };
        assert_eq!(out.component, ComponentKind::Text);
        assert!(out.rendered.is_string());
    }

    #[test]
    fn output_format_rewrites_content() {
        let reg = OutputUiExtensionRegistry::new(HostKind::Tui);
        let h: OutputFormatHandler = Arc::new(|ev| {
            let mut next = ev.clone();
            next.content = format!("[formatted] {}", ev.content);
            Action::Modify(next)
        });
        reg.register_output_format("formatter", h);
        let input =
            OutputFormatInput::new("s1", "hello", "text/plain", Audience::User);
        let Action::Modify(out) = reg.fire_output_format(input) else {
            panic!("expected Modify")
        };
        assert_eq!(out.content, "[formatted] hello");
    }

    #[test]
    fn metadata_inject_merges_patches() {
        let reg = OutputUiExtensionRegistry::new(HostKind::Tui);
        let h1: MetadataInjectHandler = Arc::new(|| {
            let mut p = MetadataPatch::new();
            p.insert("trace_id", MetadataValue::from("abc123"));
            p
        });
        let h2: MetadataInjectHandler = Arc::new(|| {
            let mut p = MetadataPatch::new();
            p.insert("trace_id", MetadataValue::from("override-me"));
            p.insert("user", MetadataValue::from("alice"));
            p
        });
        reg.register_metadata_inject("h1", h1);
        reg.register_metadata_inject("h2", h2);
        let patch = reg.fire_metadata_inject();
        // First-registered extension wins on conflicts.
        assert_eq!(
            patch.fields.get("trace_id").unwrap(),
            &MetadataValue::from("abc123")
        );
        assert_eq!(
            patch.fields.get("user").unwrap(),
            &MetadataValue::from("alice")
        );
    }

    #[test]
    fn dialog_notify_is_non_blocking() {
        let reg = OutputUiExtensionRegistry::new(HostKind::Tui);
        let counter = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let c = counter.clone();
        let h: DialogNotifyHandler = Arc::new(move |_req| {
            c.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        });
        reg.register_dialog_notify("toast", h);
        let req = NotifyRequest::new("s1", "x", NotificationLevel::Info);
        reg.fire_dialog_notify(&req);
        assert_eq!(counter.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[test]
    fn dialog_confirm_returns_handler_choice() {
        let reg = OutputUiExtensionRegistry::new(HostKind::Tui);
        let h: DialogConfirmHandler = Arc::new(|_req| true);
        reg.register_dialog_confirm("ui", h);
        let req = ConfirmRequest::new("s1", "Allow?", false);
        assert!(reg.fire_dialog_confirm(&req));
    }

    #[test]
    fn dialog_confirm_falls_back_to_default() {
        let reg = OutputUiExtensionRegistry::new(HostKind::Tui);
        let req = ConfirmRequest::new("s1", "Allow?", true);
        // No handler → default.
        assert!(reg.fire_dialog_confirm(&req));
    }
}
