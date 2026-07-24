#[cfg(test)]
#[allow(clippy::module_inception)]
mod tests {
    use crate::prompt::{
        cache::{CacheBreakDetector, create_prompt_snapshot},
        mark::CacheControlMark,
    };

    fn default_mark() -> CacheControlMark {
        CacheControlMark::default()
    }

    #[test]
    fn test_cache_break_detector_new() {
        let detector = CacheBreakDetector::new();
        assert!(detector.state_by_source.is_empty());
    }

    #[test]
    fn test_record_prompt_state() {
        let mut detector = CacheBreakDetector::new();
        let snapshot = create_prompt_snapshot(
            "system",
            "tools",
            "claude-3",
            false,
            &default_mark(),
        );
        detector.record_prompt_state("test_source", snapshot);

        let count = detector.get_call_count("test_source");
        assert_eq!(count, Some(1));
    }

    #[test]
    fn test_record_prompt_state_increments_call_count() {
        let mut detector = CacheBreakDetector::new();
        let snapshot1 = create_prompt_snapshot(
            "system",
            "tools",
            "claude-3",
            false,
            &default_mark(),
        );
        detector.record_prompt_state("test_source", snapshot1);

        let snapshot2 = create_prompt_snapshot(
            "system2",
            "tools2",
            "claude-3",
            false,
            &default_mark(),
        );
        detector.record_prompt_state("test_source", snapshot2);

        let count = detector.get_call_count("test_source");
        assert_eq!(count, Some(2));
    }

    #[test]
    fn test_cache_break_detection_no_break() {
        let mut detector = CacheBreakDetector::new();
        let snapshot = create_prompt_snapshot(
            "system",
            "tools",
            "claude-3",
            false,
            &default_mark(),
        );
        detector.record_prompt_state("test_source", snapshot);

        let result = detector.check_cache_break("test_source", 10000, 5000);
        assert!(result.is_none());
    }

    #[test]
    fn test_notify_cache_deletion() {
        let mut detector = CacheBreakDetector::new();
        let snapshot = create_prompt_snapshot(
            "system",
            "tools",
            "claude-3",
            false,
            &default_mark(),
        );
        detector.record_prompt_state("test_source", snapshot);

        detector.notify_cache_deletion("test_source");

        let state = detector.state_by_source.get("test_source");
        assert!(state.unwrap().cache_deletions_pending);
    }

    #[test]
    fn test_notify_compaction() {
        let mut detector = CacheBreakDetector::new();
        let snapshot = create_prompt_snapshot(
            "system",
            "tools",
            "claude-3",
            false,
            &default_mark(),
        );
        detector.record_prompt_state("test_source", snapshot);

        detector.notify_compaction("test_source");

        let state = detector.state_by_source.get("test_source");
        assert!(state.unwrap().prev_cache_read_tokens.is_none());
    }

    #[test]
    fn test_cleanup_source() {
        let mut detector = CacheBreakDetector::new();
        let snapshot = create_prompt_snapshot(
            "system",
            "tools",
            "claude-3",
            false,
            &default_mark(),
        );
        detector.record_prompt_state("test_source", snapshot);

        detector.cleanup_source("test_source");

        assert!(detector.get_call_count("test_source").is_none());
    }

    #[test]
    fn test_reset() {
        let mut detector = CacheBreakDetector::new();
        let snapshot = create_prompt_snapshot(
            "system",
            "tools",
            "claude-3",
            false,
            &default_mark(),
        );
        detector.record_prompt_state("test_source", snapshot);

        detector.reset();

        assert!(detector.state_by_source.is_empty());
    }

    #[test]
    fn test_create_prompt_snapshot() {
        let snapshot = create_prompt_snapshot(
            "system content",
            "tools content",
            "claude-3",
            true,
            &default_mark(),
        );

        assert_eq!(snapshot.model, "claude-3");
        assert!(snapshot.fast_mode);
        assert!(snapshot.system_hash != 0);
        assert!(snapshot.tools_hash != 0);
    }

    #[test]
    fn cache_control_hash_independent_of_system() {
        use crate::prompt::mark::{CacheScope, CacheTtl};
        let mark_default = CacheControlMark::default();
        let mark_long = CacheControlMark {
            ttl: CacheTtl::Long,
            scope: CacheScope::new("alice", "s1"),
            pinned: true,
        };
        let s1 = create_prompt_snapshot(
            "system",
            "tools",
            "m",
            false,
            &mark_default,
        );
        let s2 =
            create_prompt_snapshot("system", "tools", "m", false, &mark_long);
        assert_eq!(s1.system_hash, s2.system_hash);
        assert_ne!(s1.cache_control_hash, s2.cache_control_hash);
    }

    #[test]
    fn cache_break_detects_system_prompt_change() {
        let mut detector = CacheBreakDetector::new();
        let baseline = create_prompt_snapshot(
            "system-v1",
            "tools",
            "claude-3",
            false,
            &default_mark(),
        );
        detector.record_prompt_state("src", baseline);
        let changed = create_prompt_snapshot(
            "system-v2",
            "tools",
            "claude-3",
            false,
            &default_mark(),
        );
        detector.record_prompt_state("src", changed);
        // `record_prompt_state` does not set prev_cache_read_tokens; seed it so
        // the token-drop gate in `check_cache_break` proceeds.
        detector
            .state_by_source
            .get_mut("src")
            .unwrap()
            .prev_cache_read_tokens = Some(10000);

        let report = detector
            .check_cache_break("src", 5000, 1000)
            .expect("expected a cache break report");
        assert!(report.system_prompt_changed);
        assert!(!report.tool_schemas_changed);
        assert_eq!(report.reason, "system prompt changed");
    }

    #[test]
    fn cache_break_detects_tool_schemas_change() {
        let mut detector = CacheBreakDetector::new();
        let baseline = create_prompt_snapshot(
            "system",
            "tools-v1",
            "claude-3",
            false,
            &default_mark(),
        );
        detector.record_prompt_state("src", baseline);
        let changed = create_prompt_snapshot(
            "system",
            "tools-v2",
            "claude-3",
            false,
            &default_mark(),
        );
        detector.record_prompt_state("src", changed);
        detector
            .state_by_source
            .get_mut("src")
            .unwrap()
            .prev_cache_read_tokens = Some(10000);

        let report = detector
            .check_cache_break("src", 5000, 1000)
            .expect("expected a cache break report");
        assert!(report.tool_schemas_changed);
        assert!(!report.system_prompt_changed);
        assert_eq!(report.reason, "tool schemas changed");
    }

    #[test]
    fn cache_break_no_false_positive_when_unchanged() {
        let mut detector = CacheBreakDetector::new();
        let snapshot = create_prompt_snapshot(
            "system",
            "tools",
            "claude-3",
            false,
            &default_mark(),
        );
        detector.record_prompt_state("src", snapshot.clone());
        // Record identical content again: epochs must not report a change.
        detector.record_prompt_state("src", snapshot);
        detector
            .state_by_source
            .get_mut("src")
            .unwrap()
            .prev_cache_read_tokens = Some(10000);

        let report = detector
            .check_cache_break("src", 5000, 1000)
            .expect("expected a cache break report");
        assert!(!report.system_prompt_changed);
        assert!(!report.tool_schemas_changed);
        assert!(!report.cache_control_changed);
        assert_eq!(report.reason, "possible TTL expiry");
    }
}
