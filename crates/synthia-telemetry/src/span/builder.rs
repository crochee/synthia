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
