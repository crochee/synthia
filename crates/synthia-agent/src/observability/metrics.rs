use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

/// Prometheus 指标集合
///
/// DEPRECATED: This struct is deprecated. Use the `metrics` crate macros directly
/// (e.g., `metrics::counter!("name").increment(1)`) for metrics collection.
/// The metrics crate integrates with MetricsServer for Prometheus export.
///
/// This struct is kept for backward compatibility but its methods delegate to
/// the global metrics recorder.
#[non_exhaustive]
#[deprecated(
    since = "0.2.0",
    note = "Use metrics crate macros instead (e.g., metrics::counter!()). See tracing.rs for examples."
)]
pub struct AgentMetrics {
    /// Total API calls (deprecated, use metrics::counter!)
    pub api_calls_total: AtomicU64,
    /// Total tokens used (deprecated, use metrics::counter!)
    pub tokens_used_total: AtomicU64,
    /// Current context usage in tokens (deprecated, use metrics::gauge!)
    pub context_usage_tokens: AtomicUsize,
    /// Context pruning count (deprecated, use metrics::counter!)
    pub context_pruning_total: AtomicU64,
    /// Cache hits (deprecated, use metrics::counter!)
    pub cache_hits_total: AtomicU64,
    /// Cache misses (deprecated, use metrics::counter!)
    pub cache_misses_total: AtomicU64,
    /// Tool calls total (deprecated, use metrics::counter!)
    pub tool_calls_total: AtomicU64,
    /// Tool timeouts total (deprecated, use metrics::counter!)
    pub tool_timeouts_total: AtomicU64,
    /// Error recovery total (deprecated, use metrics::counter!)
    pub error_recovery_total: AtomicU64,
}

#[allow(deprecated)]
impl AgentMetrics {
    /// Creates a new AgentMetrics instance.
    ///
    /// DEPRECATED: AgentMetrics is deprecated. Use `metrics` crate macros directly.
    #[deprecated(since = "0.2.0", note = "Use metrics crate macros instead")]
    pub fn new() -> Self {
        Self {
            api_calls_total: AtomicU64::new(0),
            tokens_used_total: AtomicU64::new(0),
            context_usage_tokens: AtomicUsize::new(0),
            context_pruning_total: AtomicU64::new(0),
            cache_hits_total: AtomicU64::new(0),
            cache_misses_total: AtomicU64::new(0),
            tool_calls_total: AtomicU64::new(0),
            tool_timeouts_total: AtomicU64::new(0),
            error_recovery_total: AtomicU64::new(0),
        }
    }

    /// 记录 API 调用
    pub fn record_api_call(&self) {
        self.api_calls_total.fetch_add(1, Ordering::Relaxed);
    }

    /// 记录 token 使用
    pub fn record_tokens(&self, count: usize) {
        self.tokens_used_total
            .fetch_add(count as u64, Ordering::Relaxed);
        self.context_usage_tokens.store(count, Ordering::Relaxed);
    }

    /// 记录上下文修剪
    pub fn record_pruning(&self) {
        self.context_pruning_total.fetch_add(1, Ordering::Relaxed);
    }

    /// 记录缓存命中
    pub fn record_cache_hit(&self) {
        self.cache_hits_total.fetch_add(1, Ordering::Relaxed);
    }

    /// 记录缓存未命中
    pub fn record_cache_miss(&self) {
        self.cache_misses_total.fetch_add(1, Ordering::Relaxed);
    }

    /// 记录工具调用
    pub fn record_tool_call(&self) {
        self.tool_calls_total.fetch_add(1, Ordering::Relaxed);
    }

    /// 记录工具超时
    pub fn record_tool_timeout(&self) {
        self.tool_timeouts_total.fetch_add(1, Ordering::Relaxed);
    }

    /// 记录错误恢复
    pub fn record_error_recovery(&self) {
        self.error_recovery_total.fetch_add(1, Ordering::Relaxed);
    }

    /// 计算缓存命中率
    pub fn cache_hit_ratio(&self) -> f64 {
        let hits = self.cache_hits_total.load(Ordering::Relaxed) as f64;
        let misses = self.cache_misses_total.load(Ordering::Relaxed) as f64;
        let total = hits + misses;
        if total > 0.0 { hits / total } else { 1.0 }
    }

    /// 生成 Prometheus 格式指标
    pub fn prometheus_format(&self) -> String {
        format!(
            "# HELP synthia_api_calls_total Total API calls
# TYPE synthia_api_calls_total counter
synthia_api_calls_total {}

# HELP synthia_tokens_used_total Total tokens used
# TYPE synthia_tokens_used_total counter
synthia_tokens_used_total {}

# HELP synthia_context_usage_tokens Current context token usage
# TYPE synthia_context_usage_tokens gauge
synthia_context_usage_tokens {}

# HELP synthia_context_pruning_total Context pruning count
# TYPE synthia_context_pruning_total counter
synthia_context_pruning_total {}

# HELP synthia_cache_hit_ratio Cache hit ratio
# TYPE synthia_cache_hit_ratio gauge
synthia_cache_hit_ratio {:.3}

# HELP synthia_tool_calls_total Total tool calls
# TYPE synthia_tool_calls_total counter
synthia_tool_calls_total {}

# HELP synthia_tool_timeouts_total Tool timeout count
# TYPE synthia_tool_timeouts_total counter
synthia_tool_timeouts_total {}

# HELP synthia_error_recovery_total Error recovery count
# TYPE synthia_error_recovery_total counter
synthia_error_recovery_total {}
",
            self.api_calls_total.load(Ordering::Relaxed),
            self.tokens_used_total.load(Ordering::Relaxed),
            self.context_usage_tokens.load(Ordering::Relaxed),
            self.context_pruning_total.load(Ordering::Relaxed),
            self.cache_hit_ratio(),
            self.tool_calls_total.load(Ordering::Relaxed),
            self.tool_timeouts_total.load(Ordering::Relaxed),
            self.error_recovery_total.load(Ordering::Relaxed),
        )
    }
}

#[allow(deprecated)]
impl Default for AgentMetrics {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[allow(deprecated)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_hit_ratio() {
        let metrics = AgentMetrics::new();
        assert_eq!(metrics.cache_hit_ratio(), 1.0);

        metrics.record_cache_hit();
        metrics.record_cache_hit();
        metrics.record_cache_miss();

        let ratio = metrics.cache_hit_ratio();
        assert!((ratio - 2.0 / 3.0).abs() < 0.01);
    }

    #[test]
    fn test_prometheus_format() {
        let metrics = AgentMetrics::new();
        metrics.record_api_call();
        metrics.record_tokens(1000);

        let output = metrics.prometheus_format();
        assert!(output.contains("synthia_api_calls_total 1"));
        assert!(output.contains("synthia_tokens_used_total 1000"));
    }

    #[test]
    fn test_record_api_call_increments() {
        let metrics = AgentMetrics::new();
        metrics.record_api_call();
        metrics.record_api_call();
        metrics.record_api_call();
        assert_eq!(metrics.api_calls_total.load(Ordering::Relaxed), 3);
    }

    #[test]
    fn test_record_tokens_updates_both() {
        let metrics = AgentMetrics::new();
        metrics.record_tokens(500);
        metrics.record_tokens(300);
        assert_eq!(metrics.tokens_used_total.load(Ordering::Relaxed), 800);
        assert_eq!(metrics.context_usage_tokens.load(Ordering::Relaxed), 300);
    }

    #[test]
    fn test_default_impl() {
        let metrics = AgentMetrics::default();
        assert_eq!(metrics.api_calls_total.load(Ordering::Relaxed), 0);
        assert_eq!(metrics.cache_hit_ratio(), 1.0);
    }
}
