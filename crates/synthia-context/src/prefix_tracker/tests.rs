use super::*;

#[test]
fn test_hash_computation() {
    let hash1 = PrefixTracker::compute_prefix_hash("prompt1", "skills1");
    let hash2 = PrefixTracker::compute_prefix_hash("prompt1", "skills1");
    let hash3 = PrefixTracker::compute_prefix_hash("prompt2", "skills1");

    assert_eq!(hash1, hash2);
    assert_ne!(hash1, hash3);
}

#[test]
fn test_hash_deterministic() {
    let hash = PrefixTracker::compute_prefix_hash("test", "snapshot");
    assert!(!hash.is_empty());
    assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    assert_eq!(hash.len(), 64); // SHA-256 produces 32 bytes = 64 hex chars
}

#[test]
fn test_stability_tracking() {
    let mut tracker = PrefixTracker::new();
    let hash = PrefixTracker::compute_prefix_hash("prompt", "skills");

    // 第一次记录 - 变化
    assert!(!tracker.record_prefix(&hash));
    assert_eq!(tracker.stability_ratio(), 0.0);

    // 第二次记录 - 稳定
    assert!(tracker.record_prefix(&hash));
    assert_eq!(tracker.stability_ratio(), 0.5);

    // 第三次记录 - 稳定
    assert!(tracker.record_prefix(&hash));
    assert!((tracker.stability_ratio() - 2.0 / 3.0).abs() < f64::EPSILON);
}

#[test]
fn test_stability_multiple_hashes() {
    let mut tracker = PrefixTracker::new();
    let hash1 = PrefixTracker::compute_prefix_hash("prompt1", "skills");
    let hash2 = PrefixTracker::compute_prefix_hash("prompt2", "skills");

    // Two different hashes
    assert!(!tracker.record_prefix(&hash1));
    assert!(!tracker.record_prefix(&hash2));
    assert_eq!(tracker.stability_ratio(), 0.0);

    // Repeat hash1
    assert!(tracker.record_prefix(&hash1));
    assert!((tracker.stability_ratio() - 1.0 / 3.0).abs() < f64::EPSILON);
}

#[test]
fn test_default() {
    let tracker = PrefixTracker::default();
    assert_eq!(tracker.stability_ratio(), 1.0);
}

#[test]
fn test_empty_input_hash() {
    let hash = PrefixTracker::compute_prefix_hash("", "");
    assert!(!hash.is_empty());
    assert_eq!(hash.len(), 64);
}

// ===== New rolling-window tests =====

#[test]
fn test_record_pre_stores_hash() {
    let mut tracker = PrefixTracker::new();
    let hash = tracker.record_pre(b"system prompt v1", &[], &[], 1);
    assert!(!hash.is_empty());
    assert_eq!(hash.len(), 64);
    assert_eq!(tracker.window_len(), 1);
}

#[test]
fn test_record_pre_deterministic() {
    let mut t1 = PrefixTracker::new();
    let mut t2 = PrefixTracker::new();
    let h1 = t1.record_pre(b"same bytes", &[], &[], 1);
    let h2 = t2.record_pre(b"same bytes", &[], &[], 1);
    assert_eq!(h1, h2);
}

#[test]
fn test_record_pre_different_bytes_different_hash() {
    let mut tracker = PrefixTracker::new();
    let h1 = tracker.record_pre(b"version 1", &[], &[], 1);
    let h2 = tracker.record_pre(b"version 2", &[], &[], 2);
    assert_ne!(h1, h2);
    assert_eq!(tracker.window_len(), 2);
}

#[test]
fn test_record_post_returns_true_when_stable() {
    let mut tracker = PrefixTracker::new();
    tracker.record_pre(b"stable system", &[], &[], 1);
    assert!(tracker.record_post(b"stable system", &[], &[], 1));
}

#[test]
fn test_record_post_returns_false_when_changed() {
    let mut tracker = PrefixTracker::new();
    tracker.record_pre(b"v1", &[], &[], 1);
    assert!(!tracker.record_post(b"v2", &[], &[], 1));
}

#[test]
fn test_record_post_vacuous_when_no_pre() {
    let mut tracker = PrefixTracker::new();
    // No record_pre called — vacuously stable
    assert!(tracker.record_post(b"anything", &[], &[], 1));
}

#[test]
fn test_rolling_window_evicts_oldest() {
    let mut tracker = PrefixTracker::with_window(3);
    tracker.record_pre(b"v1", &[], &[], 1);
    tracker.record_pre(b"v2", &[], &[], 2);
    tracker.record_pre(b"v3", &[], &[], 3);
    assert_eq!(tracker.window_len(), 3);
    // Adding 4th evicts v1
    tracker.record_pre(b"v4", &[], &[], 4);
    assert_eq!(tracker.window_len(), 3);
}

#[test]
fn test_windowed_stability_all_stable() {
    let mut tracker = PrefixTracker::new();
    for i in 0..5 {
        tracker.record_pre(b"same", &[], &[], i);
    }
    // All 4 adjacent pairs match → 1.0
    assert!((tracker.windowed_stability_ratio() - 1.0).abs() < f64::EPSILON);
}

#[test]
fn test_windowed_stability_half_stable() {
    let mut tracker = PrefixTracker::new();
    tracker.record_pre(b"v1", &[], &[], 1);
    tracker.record_pre(b"v1", &[], &[], 2); // stable (pair 1-2)
    tracker.record_pre(b"v2", &[], &[], 3); // unstable (pair 2-3)
    tracker.record_pre(b"v2", &[], &[], 4); // stable (pair 3-4)
    // 2 stable out of 3 pairs
    assert!((tracker.windowed_stability_ratio() - 2.0 / 3.0).abs() < 1e-9);
}

#[test]
fn test_windowed_stability_empty_returns_one() {
    let tracker = PrefixTracker::new();
    assert!((tracker.windowed_stability_ratio() - 1.0).abs() < f64::EPSILON);
}

#[test]
fn test_windowed_stability_single_entry_returns_one() {
    let mut tracker = PrefixTracker::new();
    tracker.record_pre(b"only one", &[], &[], 1);
    // No adjacent pairs to compare
    assert!((tracker.windowed_stability_ratio() - 1.0).abs() < f64::EPSILON);
}

#[test]
fn test_emit_stability_event() {
    let mut tracker = PrefixTracker::new();
    tracker.record_pre(b"v1", &[], &[], 1);
    tracker.record_pre(b"v1", &[], &[], 2);
    let event = tracker.emit_stability_event(2);
    assert_eq!(event.turn_id, 2);
    assert!((event.stability_ratio - 1.0).abs() < f64::EPSILON);
}

// ===== Full-prefix hash coverage tests (D4) =====
//
// The hash must cover system + tools + messages prefix. These tests
// verify that changes to any one of the three inputs produce a
// different hash, that stability is reported accurately when all
// three are stable, and that the concatenation order is deterministic.

#[test]
fn test_tools_change_detected_by_hash() {
    // system unchanged but tools schema changed → hash must change,
    // stability_ratio must drop below 1.0.
    let mut tracker = PrefixTracker::new();
    let system = b"system prompt";
    let messages = b"[]";
    let tools_v1 = b"[{\"name\":\"tool_a\"}]";
    let tools_v2 = b"[{\"name\":\"tool_b\"}]";

    let h1 = tracker.record_pre(system, tools_v1, messages, 1);
    let h2 = tracker.record_pre(system, tools_v2, messages, 2);
    assert_ne!(
        h1, h2,
        "hash MUST change when tools schema changes (system + messages unchanged)"
    );

    let ratio = tracker.windowed_stability_ratio();
    assert!(
        ratio < 1.0,
        "stability_ratio MUST drop when tools change, got {ratio}"
    );
}

#[test]
fn test_messages_prefix_change_detected_by_hash() {
    // system + tools unchanged but messages prefix changed → hash must
    // change.
    let mut tracker = PrefixTracker::new();
    let system = b"system prompt";
    let tools = b"[]";
    let msg_v1 = b"[{\"role\":\"user\",\"content\":\"hello\"}]";
    let msg_v2 = b"[{\"role\":\"user\",\"content\":\"world\"}]";

    let h1 = tracker.record_pre(system, tools, msg_v1, 1);
    let h2 = tracker.record_pre(system, tools, msg_v2, 2);
    assert_ne!(
        h1, h2,
        "hash MUST change when messages prefix changes (system + tools unchanged)"
    );

    let ratio = tracker.windowed_stability_ratio();
    assert!(
        ratio < 1.0,
        "stability_ratio MUST drop when messages prefix changes, got {ratio}"
    );
}

#[test]
fn test_stable_prefix_reports_high_stability() {
    // All three inputs unchanged → stability_ratio MUST be ~1.0, not
    // falsely high due to undetected changes (the old behavior).
    let mut tracker = PrefixTracker::new();
    let system = b"system prompt";
    let tools = b"[{\"name\":\"tool_a\"}]";
    let messages = b"[{\"role\":\"user\",\"content\":\"hello\"}]";

    for i in 0..5 {
        let h = tracker.record_pre(system, tools, messages, i);
        if i > 0 {
            assert!(!h.is_empty(), "hash should be produced on every call");
        }
    }

    let ratio = tracker.windowed_stability_ratio();
    assert!(
        (ratio - 1.0).abs() < f64::EPSILON,
        "stability_ratio MUST be ~1.0 when all three inputs are stable, got {ratio}"
    );
}

#[test]
fn test_hash_concatenation_order_deterministic() {
    // The hash input MUST be system || tools || messages in that
    // fixed order. Verify: same three slices in the same order → same
    // hash; swapping system and tools → different hash (proving order
    // matters, not just content).
    let system = b"AAA";
    let tools = b"BBB";
    let messages = b"CCC";

    let h1 = PrefixTracker::compute_hash_bytes(system, tools, messages);
    let h2 = PrefixTracker::compute_hash_bytes(system, tools, messages);
    assert_eq!(
        h1, h2,
        "same three slices in the same order MUST produce the same hash"
    );

    // Swapping system and tools changes the hash — proves the
    // concatenation order is system-first, not tools-first.
    let h_swapped = PrefixTracker::compute_hash_bytes(tools, system, messages);
    assert_ne!(
        h1, h_swapped,
        "swapping system and tools MUST change the hash (order is fixed)"
    );

    // Changing any single input changes the hash.
    let h_tools_changed =
        PrefixTracker::compute_hash_bytes(system, b"XXX", messages);
    assert_ne!(h1, h_tools_changed, "changing tools MUST change the hash");
    let h_msgs_changed =
        PrefixTracker::compute_hash_bytes(system, tools, b"XXX");
    assert_ne!(h1, h_msgs_changed, "changing messages MUST change the hash");
}

#[test]
fn test_canonical_tools_schema_bytes_sorted_by_name() {
    // Tools in different input orders MUST produce the same canonical
    // bytes — the serialization sorts by name.
    use serde_json::json;
    use synthia_provider::ToolDefinition;

    let tools_a = vec![
        ToolDefinition::new("alpha", "desc", json!({})),
        ToolDefinition::new("beta", "desc", json!({})),
    ];
    let tools_b = vec![
        ToolDefinition::new("beta", "desc", json!({})),
        ToolDefinition::new("alpha", "desc", json!({})),
    ];

    let bytes_a = PrefixTracker::canonical_tools_schema_bytes(&tools_a);
    let bytes_b = PrefixTracker::canonical_tools_schema_bytes(&tools_b);
    assert_eq!(
        bytes_a, bytes_b,
        "canonical tools bytes MUST be independent of input order"
    );
}

#[test]
fn test_canonical_messages_prefix_bytes_stops_at_cleared() {
    // Messages before the first `tool_result_cleared_at` are included;
    // the cleared message and everything after MUST be excluded.
    use chrono::Utc;
    use synthia_provider::{Content, Message, Role};

    let mut msgs = vec![
        Message {
            role: Role::User,
            content: Content::text("hello"),
            tool_call_id: None,
            name: None,
            tool_result_cleared_at: None,
        },
        Message {
            role: Role::Assistant,
            content: Content::text("hi"),
            tool_call_id: None,
            name: None,
            tool_result_cleared_at: Some(Utc::now()),
        },
        Message {
            role: Role::User,
            content: Content::text("after cleared"),
            tool_call_id: None,
            name: None,
            tool_result_cleared_at: None,
        },
    ];

    let prefix_bytes = PrefixTracker::canonical_messages_prefix_bytes(&msgs);
    let prefix_str = String::from_utf8(prefix_bytes).unwrap();
    assert!(
        prefix_str.contains("hello"),
        "messages before cleared_at MUST be included"
    );
    assert!(
        !prefix_str.contains("after cleared"),
        "messages after the first cleared_at MUST be excluded"
    );

    // When no message is cleared, all messages are included.
    msgs[1].tool_result_cleared_at = None;
    let all_bytes = PrefixTracker::canonical_messages_prefix_bytes(&msgs);
    let all_str = String::from_utf8(all_bytes).unwrap();
    assert!(
        all_str.contains("after cleared"),
        "all messages included when none are cleared"
    );
}
