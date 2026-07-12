//! W3C Trace Context propagation.
//!
//! Adopts the `traceparent` / `tracestate` header format
//! (W3C Trace Context Level 2, https://www.w3.org/TR/trace-context/).
//!
//! In synthia v3, every `Submission` carries an optional `trace: Option<W3cTraceContext>`.
//! Every `EventMsg` carries an optional `trace` field. CLI/server/IDE clients must
//! preserve the trace context end-to-end.

use serde::{Deserialize, Serialize};

/// W3C Trace Context — propagation carrier.
///
/// `traceparent` is required; `tracestate` is optional vendor-specific.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct W3cTraceContext {
    /// `00-<trace-id 32 hex>-<parent-id 16 hex>-<trace-flags 2 hex>`
    pub traceparent: String,
    /// Vendor-specific tracestate (`key=value,key=value`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tracestate: Option<String>,
}

impl W3cTraceContext {
    /// Sentinel for placeholder when no trace context is active.
    pub fn invalid() -> Self {
        Self {
            traceparent:
                "00-00000000000000000000000000000000-0000000000000000-00"
                    .to_string(),
            tracestate: None,
        }
    }

    /// Returns true if this is the placeholder, not a real context.
    pub fn is_invalid(&self) -> bool {
        self.traceparent
            .starts_with("00-00000000000000000000000000000000-")
    }
}

#[cfg(feature = "otel")]
mod otel {
    use opentelemetry::trace::SpanContext;

    use super::W3cTraceContext;

    impl W3cTraceContext {
        /// Extract current OTel context using global propagator.
        /// Returns `None` if no span is active.
        pub fn from_current_otel() -> Option<Self> {
            use opentelemetry::trace::TraceContextExt;
            use tracing_opentelemetry::OpenTelemetrySpanExt;
            let span = tracing::Span::current();
            let cx = span.context();
            let span_ref = cx.span();
            let sc = span_ref.span_context();
            Self::from_span_context(sc)
        }

        /// Build a `W3cTraceContext` from an OTel `SpanContext`.
        ///
        /// Returns `None` if the span context is invalid (no active trace).
        /// Preserves vendor tracestate when present (P8: no information loss).
        #[doc(hidden)]
        pub fn from_span_context(sc: &SpanContext) -> Option<Self> {
            if !sc.is_valid() {
                return None;
            }
            let trace_id = sc.trace_id().to_string();
            let span_id = sc.span_id().to_string();
            let flags = format!("{:02x}", sc.trace_flags().to_u8());
            let tracestate = sc.trace_state().header();
            let tracestate = if tracestate.is_empty() {
                None
            } else {
                Some(tracestate)
            };
            Some(W3cTraceContext {
                traceparent: format!("00-{}-{}-{}", trace_id, span_id, flags),
                tracestate,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_is_invalid() {
        let ctx = W3cTraceContext::invalid();
        assert!(ctx.is_invalid());
    }

    #[test]
    fn roundtrip_serde() {
        let ctx = W3cTraceContext {
            traceparent:
                "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01"
                    .to_string(),
            tracestate: Some("vendor=value".to_string()),
        };
        let json = serde_json::to_string(&ctx).unwrap();
        let parsed: W3cTraceContext = serde_json::from_str(&json).unwrap();
        assert_eq!(ctx, parsed);
    }

    #[cfg(feature = "otel")]
    #[test]
    fn from_span_context_preserves_tracestate() {
        // Regression for P8: previously `from_current_otel()` hardcoded
        // `tracestate: None`, silently dropping vendor tracestate.
        use opentelemetry::trace::{
            SpanContext,
            SpanId,
            TraceFlags,
            TraceId,
            TraceState,
        };

        let trace_id = TraceId::from_hex("0af7651916cd43dd8448eb211c80319c")
            .expect("valid trace id");
        let span_id =
            SpanId::from_hex("b7ad6b7169203331").expect("valid span id");
        let ts = TraceState::from_key_value([("vendor", "value")])
            .expect("valid tracestate");
        let sc =
            SpanContext::new(trace_id, span_id, TraceFlags::SAMPLED, true, ts);

        let ctx = W3cTraceContext::from_span_context(&sc)
            .expect("valid span context must produce Some");
        assert_eq!(
            ctx.tracestate.as_deref(),
            Some("vendor=value"),
            "tracestate must be preserved (P8: no information loss)"
        );
    }

    #[cfg(feature = "otel")]
    #[test]
    fn from_span_context_drops_empty_tracestate_to_none() {
        use opentelemetry::trace::{
            SpanContext,
            SpanId,
            TraceFlags,
            TraceId,
            TraceState,
        };

        let trace_id = TraceId::from_hex("0af7651916cd43dd8448eb211c80319c")
            .expect("valid trace id");
        let span_id =
            SpanId::from_hex("b7ad6b7169203331").expect("valid span id");
        let sc = SpanContext::new(
            trace_id,
            span_id,
            TraceFlags::SAMPLED,
            true,
            TraceState::default(),
        );

        let ctx = W3cTraceContext::from_span_context(&sc)
            .expect("valid span context must produce Some");
        assert_eq!(ctx.tracestate, None);
    }

    #[cfg(feature = "otel")]
    #[test]
    fn from_span_context_returns_none_for_invalid() {
        use opentelemetry::trace::SpanContext;
        let sc = SpanContext::empty_context();
        assert!(W3cTraceContext::from_span_context(&sc).is_none());
    }
}
