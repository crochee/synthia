//! `EnhancedMetricsCollector` — thread-safe agent metrics collector.

use std::sync::{
    Mutex,
    atomic::{AtomicU64, Ordering},
};

use super::types::{AgentMetricsConfig, AgentMetricsReport, LatencyStats};

#[derive(Debug)]
pub struct EnhancedMetricsCollector {
    config: AgentMetricsConfig,

    llm_call_count: AtomicU64,
    total_llm_latency_ms: AtomicU64,
    total_input_tokens: AtomicU64,
    total_output_tokens: AtomicU64,
    total_cached_tokens: AtomicU64,

    tool_call_count: AtomicU64,
    tool_success_count: AtomicU64,
    tool_failure_count: AtomicU64,
    tool_retry_count: AtomicU64,
    total_tool_latency_ms: AtomicU64,

    self_correction_count: AtomicU64,
    context_truncation_count: AtomicU64,
    compaction_count: AtomicU64,

    prefix_cache_hits: AtomicU64,
    prefix_cache_misses: AtomicU64,

    llm_latencies: Mutex<LatencyStats>,
}

impl EnhancedMetricsCollector {
    pub fn new(config: AgentMetricsConfig) -> Self {
        Self {
            config,
            llm_call_count: AtomicU64::new(0),
            total_llm_latency_ms: AtomicU64::new(0),
            total_input_tokens: AtomicU64::new(0),
            total_output_tokens: AtomicU64::new(0),
            total_cached_tokens: AtomicU64::new(0),
            tool_call_count: AtomicU64::new(0),
            tool_success_count: AtomicU64::new(0),
            tool_failure_count: AtomicU64::new(0),
            tool_retry_count: AtomicU64::new(0),
            total_tool_latency_ms: AtomicU64::new(0),
            self_correction_count: AtomicU64::new(0),
            context_truncation_count: AtomicU64::new(0),
            compaction_count: AtomicU64::new(0),
            prefix_cache_hits: AtomicU64::new(0),
            prefix_cache_misses: AtomicU64::new(0),
            llm_latencies: Mutex::new(LatencyStats::new()),
        }
    }

    pub fn record_llm_call(
        &self,
        latency_ms: u64,
        input_tokens: u64,
        output_tokens: u64,
    ) {
        self.llm_call_count.fetch_add(1, Ordering::Relaxed);
        self.total_llm_latency_ms
            .fetch_add(latency_ms, Ordering::Relaxed);
        self.total_input_tokens
            .fetch_add(input_tokens, Ordering::Relaxed);
        self.total_output_tokens
            .fetch_add(output_tokens, Ordering::Relaxed);

        let mut latencies = self.llm_latencies.lock().expect("poisoned");
        latencies.record(latency_ms);
    }

    /// Returns a snapshot of the LLM latency distribution stats
    /// (count, sum_ms, min_ms, max_ms).
    pub fn llm_latency_stats(&self) -> LatencyStats {
        let s = self.llm_latencies.lock().expect("poisoned");
        LatencyStats {
            count: s.count,
            sum_ms: s.sum_ms,
            min_ms: s.min_ms,
            max_ms: s.max_ms,
        }
    }

    pub fn record_llm_call_with_cache(
        &self,
        latency_ms: u64,
        input_tokens: u64,
        output_tokens: u64,
        cached_tokens: u64,
    ) {
        self.record_llm_call(latency_ms, input_tokens, output_tokens);
        if cached_tokens > 0 {
            self.total_cached_tokens
                .fetch_add(cached_tokens, Ordering::Relaxed);
        }
    }

    pub fn record_tool_call(&self, latency_ms: u64, success: bool) {
        self.tool_call_count.fetch_add(1, Ordering::Relaxed);
        if success {
            self.tool_success_count.fetch_add(1, Ordering::Relaxed);
        } else {
            self.tool_failure_count.fetch_add(1, Ordering::Relaxed);
        }
        self.total_tool_latency_ms
            .fetch_add(latency_ms, Ordering::Relaxed);
    }

    pub fn record_tool_retry(&self) {
        self.tool_retry_count.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_self_correction(&self) {
        self.self_correction_count.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_context_truncation(&self) {
        self.context_truncation_count
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_compaction(&self) {
        self.compaction_count.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_prefix_cache_hit(&self, hit: bool) {
        if hit {
            self.prefix_cache_hits.fetch_add(1, Ordering::Relaxed);
        } else {
            self.prefix_cache_misses.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub fn estimate_cost_usd(&self) -> f64 {
        if !self.config.enable_cost_tracking {
            return 0.0;
        }

        let input_tokens =
            self.total_input_tokens.load(Ordering::Relaxed) as f64;
        let output_tokens =
            self.total_output_tokens.load(Ordering::Relaxed) as f64;
        let cached_tokens =
            self.total_cached_tokens.load(Ordering::Relaxed) as f64;

        let non_cached_input = input_tokens - cached_tokens;
        let input_cost =
            non_cached_input / 1000.0 * self.config.token_price_per_1k_input;
        let output_cost =
            output_tokens / 1000.0 * self.config.token_price_per_1k_output;

        input_cost + output_cost
    }

    pub fn get_report(&self) -> AgentMetricsReport {
        let llm_calls = self.llm_call_count.load(Ordering::Relaxed);
        let tool_calls = self.tool_call_count.load(Ordering::Relaxed);
        let tool_successes = self.tool_success_count.load(Ordering::Relaxed);
        let tool_failures = self.tool_failure_count.load(Ordering::Relaxed);

        let total_llm_latency =
            self.total_llm_latency_ms.load(Ordering::Relaxed);
        let total_tool_latency =
            self.total_tool_latency_ms.load(Ordering::Relaxed);

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

        let tool_success_rate = if tool_calls > 0 {
            tool_successes as f64 / tool_calls as f64
        } else {
            0.0
        };

        let prefix_cache_hit_ratio = {
            let hits = self.prefix_cache_hits.load(Ordering::Relaxed);
            let misses = self.prefix_cache_misses.load(Ordering::Relaxed);
            if hits + misses > 0 {
                hits as f64 / (hits + misses) as f64
            } else {
                0.0
            }
        };

        let quality_score = self.compute_quality_score(
            llm_calls,
            tool_calls,
            tool_success_rate,
            prefix_cache_hit_ratio,
        );

        AgentMetricsReport {
            llm_call_count: llm_calls,
            tool_call_count: tool_calls,
            tool_success_count: tool_successes,
            tool_failure_count: tool_failures,
            tool_retry_count: self.tool_retry_count.load(Ordering::Relaxed),
            avg_llm_latency_ms,
            avg_tool_latency_ms,
            total_input_tokens: self.total_input_tokens.load(Ordering::Relaxed),
            total_output_tokens: self
                .total_output_tokens
                .load(Ordering::Relaxed),
            total_cached_tokens: self
                .total_cached_tokens
                .load(Ordering::Relaxed),
            estimated_cost_usd: self.estimate_cost_usd(),
            tool_success_rate,
            prefix_cache_hit_ratio,
            self_correction_count: self
                .self_correction_count
                .load(Ordering::Relaxed),
            context_truncation_count: self
                .context_truncation_count
                .load(Ordering::Relaxed),
            compaction_count: self.compaction_count.load(Ordering::Relaxed),
            quality_score,
        }
    }

    fn compute_quality_score(
        &self,
        llm_calls: u64,
        tool_calls: u64,
        tool_success_rate: f64,
        prefix_cache_hit_ratio: f64,
    ) -> f64 {
        let mut score = 0.0;
        let mut weight_sum = 0.0;

        if llm_calls > 0 {
            score += 0.3 * 1.0;
            weight_sum += 0.3;
        }

        if tool_calls > 0 {
            score += 0.4 * tool_success_rate;
            weight_sum += 0.4;
        }

        score += 0.2 * prefix_cache_hit_ratio;
        weight_sum += 0.2;

        let self_corrections =
            self.self_correction_count.load(Ordering::Relaxed);
        let correction_penalty = if llm_calls > 0 {
            (self_corrections as f64 / llm_calls as f64).min(0.5)
        } else {
            0.0
        };
        score += 0.1 * (1.0 - correction_penalty);
        weight_sum += 0.1;

        if weight_sum > 0.0 {
            score / weight_sum
        } else {
            0.0
        }
    }
}

impl Default for EnhancedMetricsCollector {
    fn default() -> Self {
        Self::new(AgentMetricsConfig::default())
    }
}
