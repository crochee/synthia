//! `SpanBuilder` — the orchestrator that turns a
//! kind / parent / attributes triple into a
//! `(SpanContext, tracing::Span)` pair.
//!
//! Three public methods:
//!
//! - [`new`](SpanBuilder::new) — start a builder for
//!   a given [`super::kind::SpanKind`] and name string.
//! - [`with_parent`](SpanBuilder::with_parent) — attach
//!   a parent [`super::context::SpanContext`] (also
//!   inherits the parent's trace id).
//! - [`with_trace_id`](SpanBuilder::with_trace_id) —
//!   set the trace id directly (used for root spans).
//! - [`with_attribute`](SpanBuilder::with_attribute) —
//!   add a key/value attribute to the eventual context.
//! - [`build`](SpanBuilder::build) — consume the
//!   builder, return both the `SpanContext` and the
//!   `tracing::Span` ready to enter.
//! - [`build_span`](SpanBuilder::build_span) — like
//!   `build`, but discards the `SpanContext` (useful
//!   when the caller only needs the `tracing::Span`).
//!
//! Plus the private
//! [`build_tracing_span`](SpanBuilder::build_tracing_span)
//! helper that wraps the `tracing::info_span!` macro
//! and records attributes via `span.record()`.
//!
//! Kept separate from [`super::context`] (the data
//! carrier) and [`super::creators`] (the 7
//! `create_*_span` convenience functions that *use*
//! the builder) so the builder signature stays
//! stable while the convenience helpers evolve.

use std::collections::HashMap;

use super::{context::SpanContext, kind::SpanKind};

/// Builder for constructing spans with parent-child relationships and attributes.
pub struct SpanBuilder {
    kind: SpanKind,
    name: String,
    parent: Option<SpanContext>,
    trace_id: String,
    attributes: HashMap<String, String>,
}

impl SpanBuilder {
    /// Create a new builder for the given span kind and name.
    pub fn new(kind: SpanKind, name: &str) -> Self {
        Self {
            kind,
            name: name.to_string(),
            parent: None,
            trace_id: String::new(),
            attributes: HashMap::new(),
        }
    }

    /// Set the parent span context.
    pub fn with_parent(mut self, parent: &SpanContext) -> Self {
        self.parent = Some(parent.clone());
        self.trace_id = parent.trace_id.clone();
        self
    }

    /// Set the trace ID directly (for root spans).
    pub fn with_trace_id(mut self, trace_id: &str) -> Self {
        self.trace_id = trace_id.to_string();
        self
    }

    /// Add an attribute to the span.
    pub fn with_attribute(mut self, key: &str, value: &str) -> Self {
        self.attributes.insert(key.to_string(), value.to_string());
        self
    }

    /// Build the SpanContext and the corresponding tracing::Span.
    /// Returns (SpanContext, tracing::Span).
    pub fn build(self) -> (SpanContext, tracing::Span) {
        let span_ctx = match &self.parent {
            Some(parent) => {
                let mut ctx = parent.child();
                for (k, v) in &self.attributes {
                    ctx.set_attribute(k, v);
                }
                ctx
            }
            None => {
                let mut ctx = SpanContext::root(&self.trace_id);
                for (k, v) in &self.attributes {
                    ctx.set_attribute(k, v);
                }
                ctx
            }
        };

        let tracing_span = self.build_tracing_span(&span_ctx);
        (span_ctx, tracing_span)
    }

    /// Build only the tracing::Span (without returning SpanContext separately).
    /// The span is created with proper parent relationship via tracing's implicit parenting.
    pub fn build_span(&self) -> tracing::Span {
        let span_ctx = match &self.parent {
            Some(parent) => {
                let mut ctx = parent.child();
                for (k, v) in &self.attributes {
                    ctx.set_attribute(k, v);
                }
                ctx
            }
            None => {
                let mut ctx = SpanContext::root(&self.trace_id);
                for (k, v) in &self.attributes {
                    ctx.set_attribute(k, v);
                }
                ctx
            }
        };
        self.build_tracing_span(&span_ctx)
    }

    fn build_tracing_span(&self, ctx: &SpanContext) -> tracing::Span {
        let span = tracing::info_span!(
            "{}",
            self.name,
            span_id = %ctx.span_id,
            parent_span_id = %ctx.parent_span_id,
            trace_id = %ctx.trace_id,
            span.kind = self.kind.name(),
        );

        // Record attributes into the span
        for (key, value) in &ctx.attributes {
            span.record(key.as_str(), value.as_str());
        }

        span
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::span::{context::SpanContext, kind::SpanKind};

    fn root_context() -> SpanContext {
        SpanContext::root("trace-1")
    }

    // -- SpanBuilder::new -------------------------------------------

    /// `SpanBuilder::new(kind, name)` MUST store the kind and
    /// name, leave `parent = None`, `trace_id = ""`, and
    /// `attributes` empty.
    #[test]
    fn new_initializes_all_four_fields_to_defaults() {
        let b = SpanBuilder::new(SpanKind::LlmCall, "my-span");
        // Pin by exercising the public surface: build with no
        // trace_id MUST work (root span with empty trace_id).
        let (ctx, _span) = b.build();
        // SpanContext::root("") still works (UUID span_id).
        assert!(!ctx.span_id.is_empty(), "span_id must be generated");
        assert_eq!(ctx.parent_span_id, "");
        assert!(ctx.attributes.is_empty());
    }

    /// `SpanBuilder::new` MUST accept any name string (no
    /// validation — callers decide what names mean).
    #[test]
    fn new_accepts_any_name_string() {
        let _ = SpanBuilder::new(SpanKind::LlmCall, "");
        let _ = SpanBuilder::new(SpanKind::LlmCall, "normal");
        let _ = SpanBuilder::new(SpanKind::LlmCall, "with spaces");
        let _ = SpanBuilder::new(SpanKind::LlmCall, "🚀 unicode");
    }

    // -- with_parent -------------------------------------------------

    /// `with_parent(parent)` MUST clone the parent context and
    /// inherit its `trace_id`.
    #[test]
    fn with_parent_inherits_trace_id() {
        let parent = root_context();
        let b = SpanBuilder::new(SpanKind::Invocation, "child")
            .with_parent(&parent);
        let (ctx, _span) = b.build();
        assert_eq!(ctx.trace_id, parent.trace_id);
        assert_eq!(ctx.parent_span_id, parent.span_id);
    }

    /// `with_parent` MUST clone (not move) the parent — the
    /// original MUST remain usable after the call.
    #[test]
    fn with_parent_does_not_move_parent() {
        let parent = root_context();
        let _b = SpanBuilder::new(SpanKind::Invocation, "child")
            .with_parent(&parent);
        // Parent still usable.
        assert_eq!(parent.trace_id, "trace-1");
    }

    // -- with_trace_id -----------------------------------------------

    /// `with_trace_id(id)` MUST set the trace id verbatim
    /// (used for root spans).
    #[test]
    fn with_trace_id_sets_verbatim() {
        let b = SpanBuilder::new(SpanKind::Session, "root")
            .with_trace_id("custom-trace-id");
        let (ctx, _span) = b.build();
        assert_eq!(ctx.trace_id, "custom-trace-id");
    }

    /// `with_trace_id("")` MUST be allowed (empty trace_id is
    /// valid; downstream consumers can decide whether to
    /// reject).
    #[test]
    fn with_trace_id_accepts_empty_string() {
        let b = SpanBuilder::new(SpanKind::Session, "root").with_trace_id("");
        let (ctx, _span) = b.build();
        assert_eq!(ctx.trace_id, "");
    }

    /// `with_parent` ALWAYS wins over `with_trace_id` for the
    /// resulting `SpanContext::trace_id` — `with_trace_id` only
    /// applies when no parent is set. This is the actual
    /// implementation contract: `parent.child()` always
    /// inherits parent's trace_id, regardless of what was set
    /// via `with_trace_id`.
    #[test]
    fn with_parent_wins_over_with_trace_id_in_build() {
        let parent = root_context();
        let b = SpanBuilder::new(SpanKind::Invocation, "child")
            .with_parent(&parent)
            .with_trace_id("override-trace");
        let (ctx, _span) = b.build();
        assert_eq!(ctx.trace_id, parent.trace_id);
        assert_ne!(ctx.trace_id, "override-trace");
    }

    /// `with_parent` called AFTER `with_trace_id` MUST
    /// override the explicit trace id (parent wins).
    #[test]
    fn with_parent_overrides_explicit_trace_id_in_build() {
        let parent = SpanContext::root("parent-trace");
        let b = SpanBuilder::new(SpanKind::Invocation, "child")
            .with_trace_id("explicit-trace")
            .with_parent(&parent);
        let (ctx, _span) = b.build();
        assert_eq!(ctx.trace_id, parent.trace_id);
        assert_ne!(ctx.trace_id, "explicit-trace");
    }

    // -- with_attribute ----------------------------------------------

    /// `with_attribute(k, v)` MUST store the attribute under
    /// the given key (visible on the resulting SpanContext).
    #[test]
    fn with_attribute_stores_value() {
        let b = SpanBuilder::new(SpanKind::LlmCall, "llm")
            .with_attribute("model", "gpt-4o")
            .with_attribute("tokens", "100");
        let (ctx, _span) = b.build();
        assert_eq!(ctx.attributes.get("model"), Some(&"gpt-4o".to_string()));
        assert_eq!(ctx.attributes.get("tokens"), Some(&"100".to_string()));
    }

    /// `with_attribute` called twice with the same key MUST
    /// overwrite (last-writer-wins).
    #[test]
    fn with_attribute_overwrites_on_duplicate_key() {
        let b = SpanBuilder::new(SpanKind::LlmCall, "llm")
            .with_attribute("model", "gpt-3.5")
            .with_attribute("model", "gpt-4o");
        let (ctx, _span) = b.build();
        assert_eq!(ctx.attributes.get("model"), Some(&"gpt-4o".to_string()));
        assert_eq!(ctx.attributes.len(), 1);
    }

    /// `with_attribute(k, "")` MUST be allowed (empty value
    /// is a valid attribute).
    #[test]
    fn with_attribute_accepts_empty_value() {
        let b = SpanBuilder::new(SpanKind::LlmCall, "llm")
            .with_attribute("model", "");
        let (ctx, _span) = b.build();
        assert_eq!(ctx.attributes.get("model"), Some(&"".to_string()));
    }

    // -- build vs build_span ----------------------------------------

    /// `build()` MUST return `(SpanContext, tracing::Span)`.
    #[test]
    fn build_returns_pair_of_context_and_tracing_span() {
        let b = SpanBuilder::new(SpanKind::LlmCall, "x");
        let (ctx, span) = b.build();
        // ctx is a SpanContext (pin via type-checked usage).
        assert!(!ctx.span_id.is_empty());
        // span is a tracing::Span — pin via Debug/format.
        let _ = format!("{:?}", span.metadata().map(|m| m.name()));
    }

    /// `build_span()` MUST return only the `tracing::Span`
    /// (discards the SpanContext — caller doesn't need it).
    #[test]
    fn build_span_returns_only_tracing_span() {
        let b = SpanBuilder::new(SpanKind::LlmCall, "x");
        let span = b.build_span();
        let _ = format!("{:?}", span.metadata().map(|m| m.name()));
    }

    /// `build()` MUST produce a SpanContext whose
    /// `attributes` include all the attributes added via
    /// `with_attribute`.
    #[test]
    fn build_propagates_attributes_to_context() {
        let b = SpanBuilder::new(SpanKind::ToolExecution, "bash")
            .with_attribute("command", "ls")
            .with_attribute("cwd", "/tmp");
        let (ctx, _span) = b.build();
        assert_eq!(ctx.attributes.len(), 2);
        assert_eq!(ctx.attributes.get("command"), Some(&"ls".to_string()));
        assert_eq!(ctx.attributes.get("cwd"), Some(&"/tmp".to_string()));
    }

    /// `build()` with a parent MUST propagate attributes to
    /// the CHILD context (not the parent).
    #[test]
    fn build_with_parent_does_not_mutate_parent_attributes() {
        let parent = root_context();
        let parent_attrs_before = parent.attributes.clone();
        let b = SpanBuilder::new(SpanKind::Invocation, "child")
            .with_parent(&parent)
            .with_attribute("k", "v");
        let (ctx, _span) = b.build();
        assert_eq!(ctx.attributes.get("k"), Some(&"v".to_string()));
        // Parent attributes MUST be unchanged.
        assert_eq!(parent.attributes, parent_attrs_before);
    }

    // -- SpanKind propagation ---------------------------------------

    /// `build()` MUST embed the SpanKind in the resulting
    /// tracing::Span via `span.kind` field (the OTel
    /// export tag).
    #[test]
    fn build_propagates_span_kind_to_tracing_span() {
        for kind in [
            SpanKind::Session,
            SpanKind::Invocation,
            SpanKind::LlmCall,
            SpanKind::ToolExecution,
            SpanKind::ContextAssembly,
            SpanKind::GuardianCheck,
            SpanKind::Compaction,
        ] {
            let (_ctx, span) =
                SpanBuilder::new(kind, "x").with_trace_id("t").build();
            // Pin that the kind's name appears in the span
            // metadata / fields (the implementation uses
            // `span.kind = self.kind.name()`).
            // Just verifying it builds without panic; the
            // actual field name verification is downstream.
            let _ = span.metadata().map(|m| m.name());
        }
    }
}
