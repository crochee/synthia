//! Prometheus RED (Rate / Errors / Duration) metrics + text exposition.
//!
//! The HTTP request counters / histograms in [`HTTP_REQUESTS_TOTAL`] and
//! [`HTTP_REQUESTS_DURATION_SECONDS`] are the canonical "what is this
//! server doing" signal pulled by Prometheus scrapes against the
//! `/metrics` endpoint. Labels (`method`, `path`) are taken from the
//! matched axum route template (not the raw request URI), so a
//! high-cardinality path like `/api/v1/chat/sessions/{id}/messages`
//! collapses to a single time series regardless of how many distinct
//! session ids are queried.
//!
//! # Lazy registration
//!
//! The vectors register lazily on first dereference (via `lazy_static`)
//! and emit no families until a labeled child is created —
//! `prometheus::gather()` drops childless families, so a scrape between
//! boot and the first tracked request is empty by design (Prometheus
//! treats it as no data, not an error).

use lazy_static::lazy_static;
use prometheus::{
    Encoder,
    HistogramVec,
    IntCounterVec,
    TextEncoder,
    register_histogram_vec,
    register_int_counter_vec,
};

lazy_static! {
    /// Total number of HTTP requests, labeled by method and matched path.
    pub static ref HTTP_REQUESTS_TOTAL: IntCounterVec = register_int_counter_vec!(
        "http_requests_total",
        "Total number of HTTP requests",
        &["method", "path"]
    )
    .unwrap();

    /// Per-endpoint request latency histogram in seconds, labeled by
    /// method and matched path. Buckets cover the 5 ms … 10 s range —
    /// anything outside that window falls into `+Inf` and is still
    /// exported.
    pub static ref HTTP_REQUESTS_DURATION_SECONDS: HistogramVec = register_histogram_vec!(
        "http_requests_duration_seconds",
        "Duration of HTTP requests in seconds",
        &["method", "path"],
        vec![0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0]
    )
    .unwrap();

    /// Pre-built [`TextEncoder`] reused on every `/metrics` scrape to
    /// avoid a per-request allocation.
    static ref TEXT_ENCODER: TextEncoder = TextEncoder::new();
}

/// Encode all registered Prometheus metric families into the standard
/// text exposition format. Returns the serialized text ready to ship
/// as a `Content-Type: text/plain; version=0.0.4` response body.
///
/// `prometheus::TextEncoder::encode` only fails when the supplied
/// `Write` sink fails; we hand it a `Vec<u8>` that cannot fail, so
/// the `expect` is total. A future change to a streaming writer would
/// need to revisit this.
pub fn gather_text() -> Vec<u8> {
    let metric_families = prometheus::gather();
    let mut buffer = Vec::new();
    TEXT_ENCODER
        .encode(&metric_families, &mut buffer)
        .expect("Vec<u8> write cannot fail");
    buffer
}

/// MIME type for the Prometheus text exposition format (version 0.0.4).
///
/// `TextEncoder::format_type()` returns the same value; we expose the
/// constant so handlers don't need a `TextEncoder` instance to set the
/// header.
pub const TEXT_EXPOSITION_CONTENT_TYPE: &str = "text/plain; version=0.0.4";

#[cfg(test)]
mod tests {
    use super::*;

    /// `gather_text` MUST return a non-empty payload after at least one
    /// labeled sample has been observed (without a sample, the family
    /// carries no children and `prometheus::gather()` drops it).
    #[test]
    fn gather_text_emits_families_after_labeled_sample() {
        HTTP_REQUESTS_TOTAL
            .with_label_values(&["GET", "/probe"])
            .inc();
        let body = gather_text();
        let text = std::str::from_utf8(&body).expect("utf-8 text");
        assert!(
            text.contains("http_requests_total"),
            "expected http_requests_total family in scrape body, got: {text}"
        );
        assert!(
            text.contains("path=\"/probe\""),
            "expected labeled child in scrape body, got: {text}"
        );
    }

    /// `gather_text` MUST observe `http_requests_duration_seconds`
    /// samples when the histogram is fed. Verifies the histogram
    /// family is exported (independent of the counter family).
    #[test]
    fn gather_text_emits_histogram_family() {
        HTTP_REQUESTS_DURATION_SECONDS
            .with_label_values(&["POST", "/probe"])
            .observe(0.042);
        let body = gather_text();
        let text = std::str::from_utf8(&body).expect("utf-8 text");
        assert!(
            text.contains("http_requests_duration_seconds"),
            "expected histogram family in scrape body, got: {text}"
        );
        assert!(
            text.contains("http_requests_duration_seconds_count"),
            "expected histogram count suffix, got: {text}"
        );
    }

    /// `TEXT_EXPOSITION_CONTENT_TYPE` MUST pin the standard
    /// Prometheus text format MIME so handlers don't drift from the
    /// wire format.
    #[test]
    fn text_exposition_content_type_matches_text_encoder_format_type() {
        let encoder = TextEncoder::new();
        assert_eq!(encoder.format_type(), TEXT_EXPOSITION_CONTENT_TYPE);
    }
}
