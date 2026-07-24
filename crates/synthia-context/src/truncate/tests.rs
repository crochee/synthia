//! Unit tests for [`super::truncate_output`] and
//! [`super::truncate_messages`] (the two public entry
//! points) plus the spec-driven tests for the
//! `tool_result_cleared_at` placeholder branch and the
//! `replace_first_text_anywhere` helper (Shape A vs
//! Shape B vs ImageContent vs empty-ToolResult).

use std::path::PathBuf;

use synthia_provider::{
    Content,
    ContentPart,
    ImageContent,
    Message,
    Role,
    TextContent,
    ToolResult,
};

use super::{
    TruncateConfig,
    TruncatedResult,
    cleared_placeholder,
    text::replace_first_text_anywhere,
    truncate_messages,
    truncate_output,
};

fn text_msg(role: Role, text: &str) -> Message {
    Message::new(role, Content::text(text))
}

#[test]
fn empty_input_is_passthrough() {
    let cfg = TruncateConfig::default();
    let r = truncate_output("", &cfg);
    assert!(!r.truncated);
    assert_eq!(r.output, "");
    assert_eq!(r.output_bytes, 0);
    assert_eq!(r.original_bytes, 0);
    assert!(r.output_path.is_none());
}

#[test]
fn small_input_is_passthrough() {
    let cfg = TruncateConfig {
        max_bytes: 100,
        ..TruncateConfig::default()
    };
    let r = truncate_output("hello\nworld", &cfg);
    assert!(!r.truncated);
    assert_eq!(r.output, "hello\nworld");
    assert_eq!(r.output_bytes, "hello\nworld".len());
    assert!(r.output_path.is_none());
}

#[test]
fn large_input_is_truncated() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = TruncateConfig {
        max_bytes: 200,
        head_lines: 2,
        tail_lines: 2,
        temp_dir: tmp.path().to_path_buf(),
        ..Default::default()
    };
    // 50 lines of 11 chars each = 550 bytes (well above max_bytes=200)
    let content: String =
        (1..=50).map(|i| format!("line-{:02}-xxxx\n", i)).collect();
    let r = truncate_output(&content, &cfg);
    assert!(r.truncated);
    assert_eq!(r.original_bytes, content.len());
    // head
    assert!(r.output.contains("line-01"));
    // tail
    assert!(r.output.contains("line-50"));
    // middle lines (3..=48) must be absent
    assert!(!r.output.contains("line-25-xxxx"));
    // marker
    assert!(r.output.contains("truncated"));
    // spill file exists and is byte-identical
    let path = r.output_path.expect("spill path should be set");
    let written = std::fs::read_to_string(&path).unwrap();
    assert_eq!(written, content);
}

#[test]
fn short_input_with_many_lines_still_truncates_when_bytes_overflow() {
    // Confirm that the bytes budget is the gating constraint, not the
    // line counts: input > max_bytes but with only 4 lines triggers
    // truncation and a marker.
    let tmp = tempfile::tempdir().unwrap();
    let cfg = TruncateConfig {
        max_bytes: 8,
        head_lines: 100,
        tail_lines: 100,
        temp_dir: tmp.path().to_path_buf(),
        ..Default::default()
    };
    let content = "aaaa\nbbbb\ncccc\ndddd\n";
    let r = truncate_output(content, &cfg);
    assert!(r.truncated);
    assert!(r.output.contains("truncated"));
}

#[test]
fn disk_failure_degrades_gracefully() {
    // Point temp_dir at an impossible path to force a write failure.
    let bad = PathBuf::from(
        "/this-path-does-not-exist-and-cannot-be-created/synthia-truncate",
    );
    let cfg = TruncateConfig {
        max_bytes: 8,
        head_lines: 2,
        tail_lines: 2,
        temp_dir: bad,
        ..Default::default()
    };
    let content = "x".repeat(64);
    let r = truncate_output(&content, &cfg);
    assert!(r.truncated);
    assert!(r.output_path.is_none());
    // head/tail still present, but no `full output at <path>` fragment.
    assert!(!r.output.contains("full output at"));
}

#[test]
fn truncate_messages_only_affects_predicate_match() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = TruncateConfig {
        max_bytes: 16,
        head_lines: 1,
        tail_lines: 1,
        temp_dir: tmp.path().to_path_buf(),
        ..Default::default()
    };
    let long = "x".repeat(64);
    let mut msgs = vec![
        text_msg(Role::System, &long), // not affected (predicate false)
        text_msg(Role::User, "hi"),    // unaffected by size
        text_msg(Role::Tool, &long),   // affected
    ];
    let original_sys = msgs[0].content.extract_text().unwrap();
    let original_user = msgs[1].content.extract_text().unwrap();
    let results = truncate_messages(&mut msgs, &cfg, |m| m.role == Role::Tool);
    assert_eq!(results.len(), 1);
    assert!(results[0].truncated);
    // System and User preserved byte-for-byte
    assert_eq!(msgs[0].content.extract_text().unwrap(), original_sys);
    assert_eq!(msgs[1].content.extract_text().unwrap(), original_user);
    // Tool message: the original "xxxx...x" is no longer present in full
    let tool_text = msgs[2].content.extract_text().unwrap();
    assert!(!tool_text.contains(&long));
    // The replacement carries the marker.
    assert!(tool_text.contains("truncated"));
}

#[test]
fn truncated_result_legacy_field_alias_roundtrip() {
    // The new struct's serde names accept the legacy keys via alias.
    let legacy_json = r#"{
        "content": "head\n[truncated]\ntail",
        "original_length": 1000,
        "truncated_length": 200,
        "truncated": true,
        "output_path": null
    }"#;
    let parsed: TruncatedResult = serde_json::from_str(legacy_json).unwrap();
    assert_eq!(parsed.output, "head\n[truncated]\ntail");
    assert_eq!(parsed.original_bytes, 1000);
    assert_eq!(parsed.output_bytes, 200);
    assert!(parsed.truncated);

    // Re-serialise uses the new keys.
    let new_json = serde_json::to_string(&parsed).unwrap();
    assert!(new_json.contains("\"output\""));
    assert!(new_json.contains("\"original_bytes\""));
    assert!(new_json.contains("\"output_bytes\""));
    assert!(!new_json.contains("\"content\""));
}

#[test]
fn truncate_messages_preserves_order_and_role() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = TruncateConfig {
        max_bytes: 4,
        head_lines: 1,
        tail_lines: 1,
        temp_dir: tmp.path().to_path_buf(),
        ..Default::default()
    };
    let long = "y".repeat(32);
    let mut msgs = vec![
        text_msg(Role::System, &long),
        text_msg(Role::User, "u"),
        text_msg(Role::Assistant, &long),
        text_msg(Role::Tool, &long),
    ];
    let roles_before: Vec<Role> = msgs.iter().map(|m| m.role).collect();
    let _ = truncate_messages(&mut msgs, &cfg, |m| {
        matches!(m.role, Role::System | Role::Tool)
    });
    let roles_after: Vec<Role> = msgs.iter().map(|m| m.role).collect();
    assert_eq!(roles_before, roles_after);
    assert_eq!(msgs.len(), 4);
}

#[test]
fn truncate_messages_works_with_tool_result_content_part() {
    // Multi-part content with a Text variant should also be trimmable.
    let tmp = tempfile::tempdir().unwrap();
    let cfg = TruncateConfig {
        max_bytes: 8,
        head_lines: 1,
        tail_lines: 1,
        temp_dir: tmp.path().to_path_buf(),
        ..Default::default()
    };
    let long = "z".repeat(64);
    let msg = Message::new(
        Role::Tool,
        Content::Multi(vec![ContentPart::Text(TextContent {
            text: long.clone(),
            cache_control: None,
        })]),
    );
    let mut msgs = vec![msg];
    let results = truncate_messages(&mut msgs, &cfg, |m| m.role == Role::Tool);
    assert_eq!(results.len(), 1);
    assert!(results[0].truncated);
}

// =========================================================================
// tool_result_cleared_at rendering — P1 spec: prune-idempotent-marker
// =========================================================================

fn tool_result_msg(id: &str, body: &str) -> Message {
    Message {
        role: Role::Tool,
        content: Content::Multi(vec![ContentPart::Text(TextContent {
            text: body.to_string(),
            cache_control: None,
        })]),
        tool_call_id: Some(id.to_string()),
        ..Default::default()
    }
}

#[test]
fn cleared_placeholder_format_matches_spec() {
    // Spec: "[Old tool result content cleared at {ISO8601_timestamp}]"
    let ts = chrono::DateTime::parse_from_rfc3339("2026-06-12T10:30:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);
    let placeholder = cleared_placeholder(ts);
    assert!(placeholder.starts_with("[Old tool result content cleared at "));
    assert!(placeholder.ends_with("]"));
    assert!(placeholder.contains("2026-06-12T10:30:00"));
}

#[test]
fn truncate_messages_replaces_cleared_with_placeholder() {
    // A message with tool_result_cleared_at = Some(_): the renderer
    // must replace its text with the placeholder, regardless of
    // predicate / size budget. The original content stays available
    // in storage; only the LLM-visible text is swapped.
    let mut msgs = vec![
        tool_result_msg("t1", "the original tool output"),
        tool_result_msg("t2", "another original output"),
    ];
    msgs[0].tool_result_cleared_at = Some(
        chrono::DateTime::parse_from_rfc3339("2026-06-12T10:30:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc),
    );

    let cfg = TruncateConfig {
        // Big enough that no ordinary truncation would run; only the
        // cleared-at branch should fire.
        max_bytes: 1024 * 1024,
        head_lines: 1,
        tail_lines: 1,
        temp_dir: std::env::temp_dir().join("synthia-truncate-test"),
        ..Default::default()
    };
    // Predicate matches all messages, but the cleared message must
    // still be replaced by the placeholder (the branch runs first).
    let results = truncate_messages(&mut msgs, &cfg, |_| true);
    // The cleared message produced no TruncatedResult (the
    // placeholder replaces via set_msg_text, not via
    // truncate_output).
    assert_eq!(results.len(), 0);

    let first_text = msgs[0].content.extract_text().unwrap();
    assert!(
        first_text.contains("Old tool result content cleared at"),
        "cleared message should render placeholder, got: {first_text}"
    );
    assert!(
        !first_text.contains("the original tool output"),
        "cleared message must not leak original content, got: {first_text}"
    );

    // The non-cleared message is untouched (predicate did not
    // exceed the size budget).
    assert_eq!(
        msgs[1].content.extract_text().unwrap(),
        "another original output"
    );
}

#[test]
fn truncate_messages_does_not_re_render_cleared_messages() {
    // Running truncate_messages twice on a cleared message must be a
    // no-op for the cleared entry on the second call: the
    // placeholder does not contain the marker fragment twice.
    let mut msgs = vec![tool_result_msg("t1", "original body")];
    msgs[0].tool_result_cleared_at = Some(
        chrono::DateTime::parse_from_rfc3339("2026-06-12T10:30:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc),
    );
    let cfg = TruncateConfig {
        max_bytes: 1024,
        head_lines: 1,
        tail_lines: 1,
        temp_dir: std::env::temp_dir().join("synthia-truncate-test"),
        ..Default::default()
    };
    let _ = truncate_messages(&mut msgs, &cfg, |_| true);
    let first = msgs[0].content.extract_text().unwrap();
    let _ = truncate_messages(&mut msgs, &cfg, |_| true);
    let second = msgs[0].content.extract_text().unwrap();
    assert_eq!(first, second);
}

#[test]
fn truncate_messages_cleared_overrides_size_based_truncation() {
    // Cleared messages render as the placeholder even when the
    // original content would have been big enough to trigger
    // head/tail truncation.
    let mut msgs = vec![tool_result_msg("t1", &"a".repeat(8_192))];
    msgs[0].tool_result_cleared_at = Some(
        chrono::DateTime::parse_from_rfc3339("2026-06-12T10:30:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc),
    );
    let cfg = TruncateConfig {
        max_bytes: 8,
        head_lines: 1,
        tail_lines: 1,
        temp_dir: std::env::temp_dir().join("synthia-truncate-test"),
        ..Default::default()
    };
    let results = truncate_messages(&mut msgs, &cfg, |_| true);
    // No TruncatedResult for the cleared message — the placeholder
    // path short-circuits before truncate_output runs.
    assert_eq!(results.len(), 0);
    let body = msgs[0].content.extract_text().unwrap();
    assert!(body.contains("Old tool result content cleared at"));
    assert!(!body.contains(&"a".repeat(100)));
}

// =========================================================================
// replace_first_text_anywhere — spec: prune-renderer-shape-unification
//
// The cleared-placeholder branch in `truncate_messages` delegates to
// this helper. It must support the two on-the-wire tool-result shapes:
//   * Shape A: Content::Single(ContentPart::ToolResult(_))  — the
//     Anthropic / OpenAI convention that `prune()` actually marks
//   * Shape B: Content::Multi([ContentPart::Text(_)]) + tool_call_id
//     — the legacy convention
// and must safely no-op on non-text variants (e.g. ImageContent).
// =========================================================================

fn shape_a_tool_result_msg(id: &str, body: &str) -> Message {
    Message {
        role: Role::User,
        content: Content::Single(ContentPart::ToolResult(ToolResult {
            tool_use_id: id.to_string(),
            content: vec![ContentPart::Text(TextContent {
                text: body.to_string(),
                cache_control: None,
            })],
            structured_content: None,
            is_error: None,
        })),
        ..Default::default()
    }
}

fn shape_b_tool_msg(id: &str, body: &str) -> Message {
    Message {
        role: Role::Tool,
        content: Content::Multi(vec![ContentPart::Text(TextContent {
            text: body.to_string(),
            cache_control: None,
        })]),
        tool_call_id: Some(id.to_string()),
        ..Default::default()
    }
}

#[test]
fn replace_first_text_shape_a_replaces_inner_text() {
    let mut msg = shape_a_tool_result_msg("t-1", "ORIGINAL");
    // Move content out so we can pass it as a `&mut Content` to the
    // module-private helper (the helper takes Content, not Message).
    let mut content = std::mem::replace(
        &mut msg.content,
        synthia_provider::Content::text(""),
    );
    let replaced = replace_first_text_anywhere(&mut content, "REPLACED");
    assert!(replaced, "Shape A must report a replacement");
    // Drill into the ToolResult to confirm the inner text was swapped.
    let tr = match &content {
        Content::Single(ContentPart::ToolResult(tr)) => tr,
        _ => panic!("expected ContentPart::ToolResult after replacement"),
    };
    let text = tr
        .content
        .iter()
        .find_map(|p| p.text())
        .expect("ToolResult must still contain a text part");
    assert_eq!(text, "REPLACED");
    // On-the-wire fields preserved.
    assert_eq!(tr.tool_use_id, "t-1");
    assert_eq!(msg.role, Role::User);
}

#[test]
fn replace_first_text_shape_b_replaces_top_level_text() {
    let mut msg = shape_b_tool_msg("t-1", "ORIGINAL");
    let mut content = std::mem::replace(
        &mut msg.content,
        synthia_provider::Content::text(""),
    );
    let replaced = replace_first_text_anywhere(&mut content, "REPLACED");
    assert!(replaced, "Shape B must report a replacement");
    assert_eq!(content.extract_text().unwrap(), "REPLACED");
}

#[test]
fn replace_first_text_multi_replaces_first_text_in_array_order() {
    // Multi-part with [Text, ToolResult]: the first Text is
    // replaced; the ToolResult is untouched.
    let mut content = Content::Multi(vec![
        ContentPart::Text(TextContent {
            text: "FIRST".to_string(),
            cache_control: None,
        }),
        ContentPart::ToolResult(ToolResult {
            tool_use_id: "t-1".to_string(),
            content: vec![ContentPart::Text(TextContent {
                text: "SECOND".to_string(),
                cache_control: None,
            })],
            structured_content: None,
            is_error: None,
        }),
    ]);
    let replaced = replace_first_text_anywhere(&mut content, "REPLACED");
    assert!(replaced);
    if let Content::Multi(parts) = &content {
        if let ContentPart::Text(t) = &parts[0] {
            assert_eq!(t.text, "REPLACED");
        } else {
            panic!("expected Text at index 0");
        }
        if let ContentPart::ToolResult(tr) = &parts[1] {
            assert_eq!(
                tr.content[0].text().unwrap(),
                "SECOND",
                "ToolResult text must not be replaced when a Text part comes first"
            );
        } else {
            panic!("expected ToolResult at index 1");
        }
    } else {
        panic!("expected Content::Multi after replacement");
    }
}

#[test]
fn replace_first_text_no_text_field_returns_false_without_panic() {
    let mut content = Content::Single(ContentPart::Image(ImageContent {
        data: "BASE64DATA".to_string(),
        mime_type: "image/png".to_string(),
        detail: None,
    }));
    let replaced = replace_first_text_anywhere(&mut content, "REPLACED");
    assert!(!replaced, "Image content must report no replacement");
    // Content unchanged.
    if let Content::Single(ContentPart::Image(img)) = &content {
        assert_eq!(img.data, "BASE64DATA");
        assert_eq!(img.mime_type, "image/png");
    } else {
        panic!("expected ContentPart::Image after no-op");
    }
}

#[test]
fn replace_first_text_empty_tool_result_content_returns_false() {
    // A ToolResult with an empty inner content array: helper must
    // return false (no panic, no replacement).
    let mut content = Content::Single(ContentPart::ToolResult(ToolResult {
        tool_use_id: "t-empty".to_string(),
        content: vec![],
        structured_content: None,
        is_error: None,
    }));
    let replaced = replace_first_text_anywhere(&mut content, "REPLACED");
    assert!(
        !replaced,
        "ToolResult with empty inner content must report no replacement"
    );
}

// =========================================================================
// Tool output offloading — spec: tool-output-offloading
// =========================================================================

use std::time::{Duration, SystemTime};

use super::cleanup_tool_output_store;

#[test]
fn output_below_byte_and_line_thresholds_is_not_offloaded() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = TruncateConfig {
        max_bytes: 100,
        max_lines: 10,
        head_lines: 2,
        tail_lines: 2,
        temp_dir: tmp.path().to_path_buf(),
        session_id: Some("sess-1".to_string()),
        tool_call_id: Some("call-1".to_string()),
    };
    // 5 lines, 25 bytes — below both thresholds.
    let content = "a\nb\nc\nd\ne";
    let r = truncate_output(content, &cfg);
    assert!(!r.truncated);
    assert_eq!(r.output, content);
    assert!(r.output_path.is_none());
}

#[test]
fn output_exceeding_byte_threshold_is_offloaded() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = TruncateConfig {
        max_bytes: 50,
        max_lines: 10_000,
        head_lines: 2,
        tail_lines: 2,
        temp_dir: tmp.path().to_path_buf(),
        session_id: Some("sess-bytes".to_string()),
        tool_call_id: Some("call-bytes".to_string()),
    };
    let content = "x".repeat(128);
    let r = truncate_output(&content, &cfg);
    assert!(r.truncated);
    let path = r.output_path.expect("spill path should be set");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), content);
}

#[test]
fn output_exceeding_line_threshold_is_offloaded() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = TruncateConfig {
        max_bytes: 1024 * 1024,
        max_lines: 5,
        head_lines: 2,
        tail_lines: 2,
        temp_dir: tmp.path().to_path_buf(),
        session_id: Some("sess-lines".to_string()),
        tool_call_id: Some("call-lines".to_string()),
    };
    // 10 lines, each 2 bytes — small byte count but too many lines.
    let content: String = (1..=10).map(|i| format!("{i}\n")).collect();
    let r = truncate_output(&content, &cfg);
    assert!(r.truncated);
    let path = r.output_path.expect("spill path should be set");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), content);
}

#[test]
fn deterministic_path_uses_session_and_call_id() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = TruncateConfig {
        max_bytes: 8,
        max_lines: 10,
        head_lines: 1,
        tail_lines: 1,
        temp_dir: tmp.path().to_path_buf(),
        session_id: Some("my/session".to_string()),
        tool_call_id: Some("my:call|id".to_string()),
    };
    let r = truncate_output("0123456789", &cfg);
    assert!(r.truncated);
    let path = r.output_path.expect("spill path should be set");
    // Sanitized segments with deterministic layout.
    let expected = tmp.path().join("my_session").join("my:call|id.txt");
    assert_eq!(path, expected);
    assert!(path.exists());
}

#[test]
fn summary_contains_path_marker() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = TruncateConfig {
        max_bytes: 8,
        max_lines: 10,
        head_lines: 1,
        tail_lines: 1,
        temp_dir: tmp.path().to_path_buf(),
        session_id: Some("sess-marker".to_string()),
        tool_call_id: Some("call-marker".to_string()),
    };
    let content = "line1\nline2\nline3\n";
    let r = truncate_output(content, &cfg);
    assert!(r.truncated);
    let path = r.output_path.expect("spill path should be set");
    let marker = format!("full output at {}", path.display());
    assert!(
        r.output.contains(&marker),
        "summary should contain path marker, got: {}",
        r.output
    );
}

#[test]
#[cfg(unix)]
fn file_permissions_are_0o600() {
    use std::os::unix::fs::PermissionsExt;

    let tmp = tempfile::tempdir().unwrap();
    let cfg = TruncateConfig {
        max_bytes: 8,
        max_lines: 10,
        head_lines: 1,
        tail_lines: 1,
        temp_dir: tmp.path().to_path_buf(),
        session_id: Some("sess-perm".to_string()),
        tool_call_id: Some("call-perm".to_string()),
    };
    let r = truncate_output("0123456789", &cfg);
    assert!(r.truncated);
    let path = r.output_path.expect("spill path should be set");
    let mode = std::fs::metadata(&path).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o600);
}

#[test]
fn cleanup_removes_stale_files() {
    let tmp = tempfile::tempdir().unwrap();
    let stale_dir = tmp.path().join("sess-old");
    std::fs::create_dir_all(&stale_dir).unwrap();
    let stale_file = stale_dir.join("call-1.txt");
    std::fs::write(&stale_file, "stale").unwrap();

    // Manually back-date the file to older than the retention period.
    let old_time = SystemTime::now() - Duration::from_secs(8 * 24 * 60 * 60);
    let old_times = filetime::FileTime::from_system_time(old_time);
    filetime::set_file_mtime(&stale_file, old_times).unwrap();

    // A recent file in a sibling directory must survive.
    let fresh_dir = tmp.path().join("sess-new");
    std::fs::create_dir_all(&fresh_dir).unwrap();
    let fresh_file = fresh_dir.join("call-2.txt");
    std::fs::write(&fresh_file, "fresh").unwrap();

    let deleted = cleanup_tool_output_store(
        tmp.path(),
        Duration::from_secs(7 * 24 * 60 * 60),
    )
    .unwrap();
    assert_eq!(deleted, 1);
    assert!(!stale_file.exists());
    assert!(fresh_file.exists());
}

#[test]
fn full_flow_through_truncate_output_with_ids() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = TruncateConfig {
        max_bytes: 50 * 1024,
        max_lines: 2000,
        head_lines: 100,
        tail_lines: 100,
        temp_dir: tmp.path().to_path_buf(),
        session_id: Some("sess-flow".to_string()),
        tool_call_id: Some("call-flow".to_string()),
    };
    // Build an output just above the byte threshold, with enough
    // lines that both head and tail are non-empty.
    let content: String = (1..=2000)
        .map(|i| format!("line-{i:08}-{}\n", "x".repeat(32)))
        .collect();
    let r = truncate_output(&content, &cfg);
    assert!(r.truncated);

    let expected_path = tmp.path().join("sess-flow").join("call-flow.txt");
    assert_eq!(r.output_path, Some(expected_path.clone()));
    assert!(expected_path.exists());
    assert_eq!(std::fs::read_to_string(&expected_path).unwrap(), content);

    // Summary keeps head and tail.
    assert!(r.output.starts_with("line-00000001"));
    assert!(r.output.contains("line-00002000"));
    assert!(r.output.contains("truncated"));
    assert!(r.output.contains("full output at"));
}
