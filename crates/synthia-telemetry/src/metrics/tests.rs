#[cfg(test)]
mod tests {
    use crate::metrics::{MetricsCollector, names};
    #[cfg(feature = "otel")]
    use crate::{
        TelemetryConfig,
        metrics::{TelemetryMetrics, init_metrics},
    };

    #[cfg(feature = "otel")]
    #[test]
    fn test_telemetry_metrics_record_llm_call() {
        let meter = opentelemetry::global::meter("test");
        let metrics = TelemetryMetrics::new(&meter);
        metrics.record_llm_call(150, 100, 50);
        // If no exception is thrown, the instruments work correctly.
    }

    #[cfg(feature = "otel")]
    #[test]
    fn test_telemetry_metrics_record_tool_call() {
        let meter = opentelemetry::global::meter("test");
        let metrics = TelemetryMetrics::new(&meter);
        metrics.record_tool_call("bash", 75, true);
        metrics.record_tool_call("bash", 200, false);
    }

    #[cfg(feature = "otel")]
    #[test]
    fn test_init_metrics_returns_none_without_endpoint() {
        unsafe {
            std::env::remove_var(crate::tracer::SYNTHIA_OTLP_ENDPOINT_ENV)
        };
        let config = TelemetryConfig::default();
        assert!(init_metrics(&config).is_none());
    }

    #[test]
    fn test_metric_names_are_correct() {
        assert_eq!(names::LLM_CALL_COUNT, "synthia.llm.call_count");
        assert_eq!(names::LLM_CALL_DURATION_MS, "synthia.llm.call_duration_ms");
        assert_eq!(names::LLM_TOKEN_USAGE, "synthia.llm.token_usage");
        assert_eq!(names::TOOL_CALL_COUNT, "synthia.tool.call_count");
        assert_eq!(
            names::TOOL_CALL_DURATION_MS,
            "synthia.tool.call_duration_ms"
        );
    }

    #[test]
    fn test_metrics_collector_record_llm_call() {
        let collector = MetricsCollector::new();
        collector.record_llm_call(150);
        collector.record_llm_call(250);

        let report = collector.get_report();
        assert_eq!(report.llm_call_count, 2);
    }

    #[test]
    fn test_metrics_collector_record_tool_call() {
        let collector = MetricsCollector::new();
        collector.record_tool_call(50);
        collector.record_tool_call(75);

        let report = collector.get_report();
        assert_eq!(report.tool_call_count, 2);
    }

    #[test]
    fn test_metrics_collector_avg_llm_latency() {
        let collector = MetricsCollector::new();
        assert_eq!(collector.get_report().avg_llm_latency_ms, 0.0);

        collector.record_llm_call(100);
        collector.record_llm_call(200);
        collector.record_llm_call(300);

        let report = collector.get_report();
        assert!((report.avg_llm_latency_ms - 200.0).abs() < 0.01);
    }

    #[test]
    fn test_metrics_collector_avg_tool_latency() {
        let collector = MetricsCollector::new();
        collector.record_tool_call(60);
        collector.record_tool_call(90);

        let report = collector.get_report();
        assert!((report.avg_tool_latency_ms - 75.0).abs() < 0.01);
    }

    #[test]
    fn test_metrics_collector_prefix_cache_hit_ratio() {
        let collector = MetricsCollector::new();

        // No calls yet: ratio should be 0.0
        assert_eq!(collector.get_report().prefix_cache_hit_ratio, 0.0);

        collector.record_prefix_cache_hit(true);
        collector.record_prefix_cache_hit(true);
        collector.record_prefix_cache_hit(false);
        collector.record_prefix_cache_hit(true);

        let report = collector.get_report();
        // 3 hits out of 4 total = 0.75
        assert!((report.prefix_cache_hit_ratio - 0.75).abs() < 0.01);
    }

    #[test]
    fn test_metrics_collector_prefix_cache_all_misses() {
        let collector = MetricsCollector::new();
        collector.record_prefix_cache_hit(false);
        collector.record_prefix_cache_hit(false);

        let report = collector.get_report();
        assert!((report.prefix_cache_hit_ratio - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_metrics_collector_prefix_cache_all_hits() {
        let collector = MetricsCollector::new();
        collector.record_prefix_cache_hit(true);
        collector.record_prefix_cache_hit(true);

        let report = collector.get_report();
        assert!((report.prefix_cache_hit_ratio - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_metrics_collector_compaction_count() {
        let collector = MetricsCollector::new();
        assert_eq!(collector.get_report().compacted_count, 0);

        collector.record_compaction();
        collector.record_compaction();
        collector.record_compaction();

        assert_eq!(collector.get_report().compacted_count, 3);
    }

    #[test]
    fn test_metrics_collector_full_report() {
        let collector = MetricsCollector::new();

        collector.record_llm_call(100);
        collector.record_llm_call(200);
        collector.record_tool_call(50);
        collector.record_prefix_cache_hit(true);
        collector.record_prefix_cache_hit(false);
        collector.record_compaction();

        let report = collector.get_report();
        assert_eq!(report.llm_call_count, 2);
        assert_eq!(report.tool_call_count, 1);
        assert!((report.avg_llm_latency_ms - 150.0).abs() < 0.01);
        assert!((report.avg_tool_latency_ms - 50.0).abs() < 0.01);
        assert!((report.prefix_cache_hit_ratio - 0.5).abs() < 0.01);
        assert_eq!(report.compacted_count, 1);
    }

    #[test]
    fn test_metrics_collector_default() {
        let collector = MetricsCollector::default();
        let report = collector.get_report();
        assert_eq!(report.llm_call_count, 0);
        assert_eq!(report.tool_call_count, 0);
    }
}
