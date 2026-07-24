//! Tests for agent metrics.

use super::*;

#[test]
fn test_enhanced_metrics_record_llm_call() {
    let collector =
        EnhancedMetricsCollector::new(AgentMetricsConfig::default());
    collector.record_llm_call(150, 100, 50);

    let report = collector.get_report();
    assert_eq!(report.llm_call_count, 1);
    assert!((report.avg_llm_latency_ms - 150.0).abs() < 0.01);
}

#[test]
fn test_record_llm_call_accumulates_latency_stats() {
    let collector = EnhancedMetricsCollector::default();
    collector.record_llm_call(100, 10, 5);
    collector.record_llm_call(200, 10, 5);
    collector.record_llm_call(300, 10, 5);

    // The bug: LatencyStats was never accumulated (clone dropped).
    // avg_llm_latency_ms comes from atomics (always worked), so we must
    // inspect LatencyStats directly to catch the bug.
    let stats = collector.llm_latency_stats();
    assert_eq!(stats.count, 3, "count should be 3");
    assert_eq!(stats.sum_ms, 600, "sum should be 600");
    assert_eq!(stats.min_ms, 100, "min should be 100");
    assert_eq!(stats.max_ms, 300, "max should be 300");

    // Sanity: report fields (computed from atomics, not LatencyStats).
    let report = collector.get_report();
    assert_eq!(report.llm_call_count, 3);
    assert!(
        (report.avg_llm_latency_ms - 200.0).abs() < 0.01,
        "avg should be 200, got {}",
        report.avg_llm_latency_ms
    );
}

#[test]
fn test_enhanced_metrics_tool_tracking() {
    let collector =
        EnhancedMetricsCollector::new(AgentMetricsConfig::default());
    collector.record_tool_call(75, true);
    collector.record_tool_call(50, true);
    collector.record_tool_call(100, false);

    let report = collector.get_report();
    assert_eq!(report.tool_call_count, 3);
    assert_eq!(report.tool_success_count, 2);
    assert_eq!(report.tool_failure_count, 1);
    assert!((report.tool_success_rate - 2.0 / 3.0).abs() < 0.01);
}

#[test]
fn test_cost_estimation() {
    let config = AgentMetricsConfig {
        token_price_per_1k_input: 0.01,
        token_price_per_1k_output: 0.03,
        ..Default::default()
    };

    let collector = EnhancedMetricsCollector::new(config);
    collector.record_llm_call_with_cache(100, 1000, 500, 200);

    let report = collector.get_report();
    let expected_cost = (800.0 / 1000.0 * 0.01) + (500.0 / 1000.0 * 0.03);
    assert!((report.estimated_cost_usd - expected_cost).abs() < 0.0001);
}

#[test]
fn test_quality_score() {
    let collector =
        EnhancedMetricsCollector::new(AgentMetricsConfig::default());

    for _ in 0..5 {
        collector.record_llm_call(100, 100, 50);
        collector.record_tool_call(50, true);
    }
    collector.record_prefix_cache_hit(true);
    collector.record_prefix_cache_hit(true);
    collector.record_prefix_cache_hit(false);

    let report = collector.get_report();
    assert!(report.quality_score > 0.0 && report.quality_score <= 1.0);
}

#[test]
fn test_metrics_summary() {
    let collector =
        EnhancedMetricsCollector::new(AgentMetricsConfig::default());
    collector.record_llm_call(100, 50, 25);
    collector.record_tool_call(50, true);

    let report = collector.get_report();
    let summary = report.summary();
    assert!(summary.contains("LLM Calls: 1"));
    assert!(summary.contains("Tool Calls: 1"));
}

#[test]
fn test_quality_score_nonzero_after_llm_call() {
    let collector = EnhancedMetricsCollector::default();
    collector.record_llm_call(100, 10, 5);
    collector.record_tool_call(50, true);

    let report = collector.get_report();
    assert!(
        report.quality_score > 0.0,
        "quality_score must be > 0 after record_llm_call, got {}",
        report.quality_score
    );
}
