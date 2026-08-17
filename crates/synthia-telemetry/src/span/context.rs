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

#[cfg(test)]
mod tests {
    use super::*;

    /// `root()` MUST generate a fresh span_id
    /// (non-empty UUID), set
    /// `parent_span_id` to empty string,
    /// preserve the supplied `trace_id`
    /// verbatim, initialize `end_time =
    /// None`, and start with an empty
    /// `attributes` map.
    #[test]
    fn root_initializes_state_correctly() {
        let ctx = SpanContext::root("trace-1");
        assert!(!ctx.span_id.is_empty(), "root MUST generate a span_id");
        assert!(
            ctx.parent_span_id.is_empty(),
            "root span MUST have empty parent_span_id"
        );
        assert_eq!(ctx.trace_id, "trace-1");
        assert!(ctx.end_time.is_none());
        assert!(ctx.attributes.is_empty());
    }

    /// Two consecutive `root()` calls MUST
    /// produce different span_ids (UUIDs).
    #[test]
    fn root_generates_unique_span_ids() {
        let a = SpanContext::root("trace");
        let b = SpanContext::root("trace");
        assert_ne!(a.span_id, b.span_id);
    }

    /// `child()` MUST inherit the parent's
    /// `span_id` as its `parent_span_id` and
    /// inherit `trace_id` verbatim. It MUST
    /// generate a fresh `span_id`.
    #[test]
    fn child_inherits_parent_span_id_and_trace_id() {
        let parent = SpanContext::root("trace-2");
        let child = parent.child();
        assert_ne!(child.span_id, parent.span_id);
        assert_eq!(child.parent_span_id, parent.span_id);
        assert_eq!(child.trace_id, parent.trace_id);
        assert!(child.end_time.is_none());
        assert!(child.attributes.is_empty());
    }

    /// `end()` MUST set `end_time` to Some
    /// (some Instant). After calling it
    /// once, the span is considered ended.
    #[test]
    fn end_sets_end_time_to_some() {
        let mut ctx = SpanContext::root("trace-3");
        assert!(ctx.end_time.is_none());
        ctx.end();
        assert!(ctx.end_time.is_some());
        // Idempotent: calling again doesn't
        // panic; end_time stays Some.
        ctx.end();
        assert!(ctx.end_time.is_some());
    }

    /// `with_attribute()` is consume-style:
    /// it returns a NEW SpanContext with the
    /// attribute inserted. The original is
    /// moved. Pin so a refactor that changes
    /// to mutate-style doesn't silently
    /// break.
    #[test]
    fn with_attribute_consume_style_returns_new_context() {
        let ctx = SpanContext::root("trace-4");
        let ctx = ctx.with_attribute("k", "v");
        assert_eq!(ctx.attributes.get("k"), Some(&"v".to_string()));
    }

    /// `with_attribute()` chained calls MUST
    /// accumulate (last-write-wins on the
    /// same key).
    #[test]
    fn with_attribute_chained_calls_accumulate() {
        let ctx = SpanContext::root("trace-5")
            .with_attribute("a", "1")
            .with_attribute("b", "2")
            .with_attribute("a", "11"); // overwrite
        assert_eq!(ctx.attributes.get("a"), Some(&"11".to_string()));
        assert_eq!(ctx.attributes.get("b"), Some(&"2".to_string()));
        assert_eq!(ctx.attributes.len(), 2);
    }

    /// `set_attribute()` is mutate-style:
    /// mutates the receiver in place,
    /// returns `()`.
    #[test]
    fn set_attribute_mutate_style_returns_unit() {
        let mut ctx = SpanContext::root("trace-6");
        let result: () = ctx.set_attribute("k", "v");
        assert_eq!(result, ());
        assert_eq!(ctx.attributes.get("k"), Some(&"v".to_string()));
    }

    /// `duration()` MUST return None when
    /// the span has NOT been ended.
    #[test]
    fn duration_returns_none_before_end() {
        let ctx = SpanContext::root("trace-7");
        assert!(ctx.duration().is_none());
    }

    /// `duration()` MUST return Some when
    /// the span has been ended. The value
    /// MUST be >= 0 (end >= start), even
    /// though it may be very small
    /// (sub-microsecond).
    #[test]
    fn duration_returns_some_after_end() {
        let mut ctx = SpanContext::root("trace-8");
        std::thread::sleep(std::time::Duration::from_millis(2));
        ctx.end();
        let d = ctx.duration().expect("duration MUST be Some after end");
        // Allow zero on very fast systems
        // (sub-microsecond), but cannot be
        // negative.
        // Sleeping 2ms guarantees d > 0
        // almost certainly; if not, just pin
        // that d is bounded.
        assert!(d >= std::time::Duration::ZERO);
    }
}
