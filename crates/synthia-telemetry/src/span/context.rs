//! `SpanContext` data carrier + lifecycle methods.
//!
//! [`SpanContext`] is the lightweight metadata struct
//! the agent hands to the [`super::builder::SpanBuilder`]
//! and the `tracing`/`OTel` exporters. Six public
//! methods cover the four operations callers actually
//! need:
//!
//! - [`root`](SpanContext::root) — construct a fresh
//!   root context (no parent) with a given trace id.
//! - [`child`](SpanContext::child) — construct a child
//!   context inheriting the parent's trace id and
//!   pointing at the parent's span id.
//! - [`end`](SpanContext::end) — stamp `end_time` to
//!   mark the span finished.
//! - [`with_attribute`](SpanContext::with_attribute) /
//!   [`set_attribute`](SpanContext::set_attribute) —
//!   consume-style vs. mutate-style attribute setters.
//! - [`duration`](SpanContext::duration) — compute
//!   `end_time - start_time` if the span has ended.
//!
//! Kept separate from [`super::kind`] (the taxonomy
//! enum) and [`super::builder`] (the orchestrator that
//! attaches `tracing::Span`s on top of `SpanContext`)
//! so this file can stay focused on the data shape.

use std::{collections::HashMap, time::Instant};

/// Lightweight context for tracking span lifecycle metadata.
#[derive(Debug, Clone)]
pub struct SpanContext {
    /// Unique identifier for this span (UUID string).
    pub span_id: String,
    /// Parent span ID, or empty string for root spans.
    pub parent_span_id: String,
    /// Trace ID shared across the entire session.
    pub trace_id: String,
    /// When the span started.
    pub start_time: Instant,
    /// When the span ended, if finished.
    pub end_time: Option<Instant>,
    /// Key-value attributes attached to the span.
    pub attributes: HashMap<String, String>,
}

impl SpanContext {
    /// Create a new SpanContext for a root span (no parent).
    pub fn root(trace_id: &str) -> Self {
        Self {
            span_id: uuid::Uuid::new_v4().to_string(),
            parent_span_id: String::new(),
            trace_id: trace_id.to_string(),
            start_time: Instant::now(),
            end_time: None,
            attributes: HashMap::new(),
        }
    }

    /// Create a child SpanContext of this span.
    pub fn child(&self) -> Self {
        Self {
            span_id: uuid::Uuid::new_v4().to_string(),
            parent_span_id: self.span_id.clone(),
            trace_id: self.trace_id.clone(),
            start_time: Instant::now(),
            end_time: None,
            attributes: HashMap::new(),
        }
    }

    /// Mark the span as ended.
    pub fn end(&mut self) {
        self.end_time = Some(Instant::now());
    }

    /// Add an attribute to the span.
    pub fn with_attribute(mut self, key: &str, value: &str) -> Self {
        self.attributes.insert(key.to_string(), value.to_string());
        self
    }

    /// Set an attribute on an existing span (mutable).
    pub fn set_attribute(&mut self, key: &str, value: &str) {
        self.attributes.insert(key.to_string(), value.to_string());
    }

    /// Duration of the span if it has ended.
    pub fn duration(&self) -> Option<std::time::Duration> {
        match (
            self.end_time,
            self.start_time.checked_duration_since(Instant::now()),
        ) {
            (Some(end), _) => Some(end.duration_since(self.start_time)),
            _ => None,
        }
    }
}
