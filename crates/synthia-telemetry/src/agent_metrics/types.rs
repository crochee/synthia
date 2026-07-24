//! Agent metrics types: config, latency stats, and report.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMetricsConfig {
    pub token_price_per_1k_input: f64,
    pub token_price_per_1k_output: f64,
    pub enable_cost_tracking: bool,
}

impl Default for AgentMetricsConfig {
    fn default() -> Self {
        Self {
            token_price_per_1k_input: 0.00001,
            token_price_per_1k_output: 0.00003,
            enable_cost_tracking: true,
        }
    }
}

#[derive(Debug)]
pub struct LatencyStats {
    pub count: u64,
    pub sum_ms: u64,
    pub min_ms: u64,
    pub max_ms: u64,
}

impl LatencyStats {
    pub fn new() -> Self {
        Self {
            count: 0,
            sum_ms: 0,
            min_ms: u64::MAX,
            max_ms: 0,
        }
    }

    pub fn record(&mut self, latency_ms: u64) {
        self.count += 1;
        self.sum_ms += latency_ms;
        self.min_ms = self.min_ms.min(latency_ms);
        self.max_ms = self.max_ms.max(latency_ms);
    }

    pub fn avg(&self) -> f64 {
        if self.count == 0 {
            return 0.0;
        }
        self.sum_ms as f64 / self.count as f64
    }
}

impl Default for LatencyStats {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMetricsReport {
    pub llm_call_count: u64,
    pub tool_call_count: u64,
    pub tool_success_count: u64,
    pub tool_failure_count: u64,
    pub tool_retry_count: u64,
    pub avg_llm_latency_ms: f64,
    pub avg_tool_latency_ms: f64,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_cached_tokens: u64,
    pub estimated_cost_usd: f64,
    pub tool_success_rate: f64,
    pub prefix_cache_hit_ratio: f64,
    pub self_correction_count: u64,
    pub context_truncation_count: u64,
    pub compaction_count: u64,
    pub quality_score: f64,
}

impl AgentMetricsReport {
    pub fn summary(&self) -> String {
        format!(
            "Agent Metrics Report:\n\
- LLM Calls: {}, Avg Latency: {:.2}ms\n\
- Tool Calls: {} (Success: {}, Failures: {}, Retries: {})\n\
- Tool Success Rate: {:.1}%\n\
- Token Usage: {} input / {} output / {} cached\n\
- Estimated Cost: ${:.6}\n\
- Cache Hit Ratio: {:.1}%\n\
- Self Corrections: {}, Truncations: {}, Compactions: {}\n\
- Quality Score: {:.2}/1.0",
            self.llm_call_count,
            self.avg_llm_latency_ms,
            self.tool_call_count,
            self.tool_success_count,
            self.tool_failure_count,
            self.tool_retry_count,
            self.tool_success_rate * 100.0,
            self.total_input_tokens,
            self.total_output_tokens,
            self.total_cached_tokens,
            self.estimated_cost_usd,
            self.prefix_cache_hit_ratio * 100.0,
            self.self_correction_count,
            self.context_truncation_count,
            self.compaction_count,
            self.quality_score
        )
    }
}
