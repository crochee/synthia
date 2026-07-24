use std::sync::atomic::{AtomicU64, Ordering};

/// A lightweight, in-memory metrics collector using atomic counters.
///
/// Designed for use by the agent's `run_stream` to track LLM and tool
/// call statistics without depending on OpenTelemetry infrastructure.
#[derive(Debug)]
pub struct MetricsCollector {
    llm_call_count: AtomicU64,
    tool_call_count: AtomicU64,
    total_llm_latency_ms: AtomicU64,
    total_tool_latency_ms: AtomicU64,
    prefix_hash_hits: AtomicU64,
    prefix_hash_misses: AtomicU64,
    compacted_count: AtomicU64,
}

/// A snapshot report computed from the current state of `MetricsCollector`.
#[derive(Debug, Clone)]
pub struct MetricsReport {
    pub llm_call_count: u64,
    pub tool_call_count: u64,
    pub avg_llm_latency_ms: f64,
    pub avg_tool_latency_ms: f64,
    pub prefix_cache_hit_ratio: f64,
    pub compacted_count: u64,
}

impl MetricsCollector {
    /// Creates a new collector with all counters initialized to zero.
    pub fn new() -> Self {
        Self {
            llm_call_count: AtomicU64::new(0),
            tool_call_count: AtomicU64::new(0),
            total_llm_latency_ms: AtomicU64::new(0),
            total_tool_latency_ms: AtomicU64::new(0),
            prefix_hash_hits: AtomicU64::new(0),
            prefix_hash_misses: AtomicU64::new(0),
            compacted_count: AtomicU64::new(0),
        }
    }

    /// Records a completed LLM call with its latency.
    pub fn record_llm_call(&self, latency_ms: u64) {
        self.llm_call_count.fetch_add(1, Ordering::Relaxed);
        self.total_llm_latency_ms
            .fetch_add(latency_ms, Ordering::Relaxed);
    }

    /// Records a completed tool call with its latency.
    pub fn record_tool_call(&self, latency_ms: u64) {
        self.tool_call_count.fetch_add(1, Ordering::Relaxed);
        self.total_tool_latency_ms
            .fetch_add(latency_ms, Ordering::Relaxed);
    }

    /// Records whether a prefix cache lookup was a hit or miss.
    pub fn record_prefix_cache_hit(&self, hit: bool) {
        if hit {
            self.prefix_hash_hits.fetch_add(1, Ordering::Relaxed);
        } else {
            self.prefix_hash_misses.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Records that a context compaction occurred.
    pub fn record_compaction(&self) {
        self.compacted_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Computes a `MetricsReport` from the current counter values.
    pub fn get_report(&self) -> MetricsReport {
        let llm_calls = self.llm_call_count.load(Ordering::Relaxed);
        let tool_calls = self.tool_call_count.load(Ordering::Relaxed);
        let total_llm_latency =
            self.total_llm_latency_ms.load(Ordering::Relaxed);
        let total_tool_latency =
            self.total_tool_latency_ms.load(Ordering::Relaxed);
        let hits = self.prefix_hash_hits.load(Ordering::Relaxed);
        let misses = self.prefix_hash_misses.load(Ordering::Relaxed);

        let avg_llm_latency_ms = if llm_calls > 0 {
            total_llm_latency as f64 / llm_calls as f64
        } else {
            0.0
        };

        let avg_tool_latency_ms = if tool_calls > 0 {
            total_tool_latency as f64 / tool_calls as f64
        } else {
            0.0
        };

        let prefix_cache_hit_ratio = if hits + misses > 0 {
            hits as f64 / (hits + misses) as f64
        } else {
            0.0
        };

        MetricsReport {
            llm_call_count: llm_calls,
            tool_call_count: tool_calls,
            avg_llm_latency_ms,
            avg_tool_latency_ms,
            prefix_cache_hit_ratio,
            compacted_count: self.compacted_count.load(Ordering::Relaxed),
        }
    }
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}
