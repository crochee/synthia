//! Integration tests for `synthia_context::truncate`.
//!
//! The same scenarios also live as inline `#[cfg(test)]` modules inside
//! `truncate.rs`; this file exercises the **public** API from the
//! integration test boundary, so the `pub fn` signatures and `pub mod`
//! exposure are part of the contract under test.

use std::path::PathBuf;

use synthia_context::truncate::{
    TruncateConfig,
    TruncatedResult,
    truncate_messages,
    truncate_output,
};
use synthia_provider::{Content, ContentPart, Message, Role, TextContent};

fn small_cfg() -> TruncateConfig {
    TruncateConfig {
        max_bytes: 256,
        head_lines: 5,
        tail_lines: 5,
        temp_dir: std::env::temp_dir().join("synthia-truncate-it"),
        ..Default::default()
    }
}

#[test]
fn small_input_round_trips_without_truncation() {
    let cfg = small_cfg();
    let input = "line-1\nline-2\nline-3\n";
    let r = truncate_output(input, &cfg);
    assert!(!r.truncated);
    assert_eq!(r.output, input);
    assert_eq!(r.output_bytes, input.len());
    assert_eq!(r.original_bytes, input.len());
    assert!(r.output_path.is_none());
}

#[test]
fn large_input_is_truncated_and_spilled() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = TruncateConfig {
        max_bytes: 64,
        head_lines: 2,
        tail_lines: 2,
        temp_dir: tmp.path().to_path_buf(),
        ..Default::default()
    };
    let input: String = (1..=20)
        .map(|i| format!("line-{:02}-payload\n", i))
        .collect();
    let r = truncate_output(&input, &cfg);
    assert!(r.truncated);
    assert!(r.output.contains("line-01"));
    assert!(r.output.contains("line-20"));
    assert!(r.output.contains("truncated"));
    let path = r.output_path.expect("spill path set");
    let written = std::fs::read_to_string(&path).unwrap();
    assert_eq!(written, input);
}

#[test]
fn empty_input_passes_through() {
    let cfg = small_cfg();
    let r = truncate_output("", &cfg);
    assert!(!r.truncated);
    assert_eq!(r.output, "");
    assert_eq!(r.output_bytes, 0);
    assert!(r.output_path.is_none());
}

#[test]
fn disk_failure_does_not_panic_and_omits_output_path() {
    // Point at a path that cannot be created.
    let bad =
        PathBuf::from("/this/path/should/not/exist/synthia-truncate-it-x");
    let cfg = TruncateConfig {
        max_bytes: 8,
        head_lines: 2,
        tail_lines: 2,
        temp_dir: bad,
        ..Default::default()
    };
    let big = "x".repeat(64);
    let r = truncate_output(&big, &cfg);
    assert!(r.truncated);
    assert!(r.output_path.is_none());
    assert!(r.output.contains("truncated"));
}

#[test]
fn truncate_messages_replaces_tool_role_text() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = TruncateConfig {
        max_bytes: 32,
        head_lines: 1,
        tail_lines: 1,
        temp_dir: tmp.path().to_path_buf(),
        ..Default::default()
    };
    let big = "y".repeat(200);
    let mut msgs = vec![
        Message::new(Role::System, Content::text("system stays")),
        Message::new(Role::User, Content::text("hi")),
        Message::new(Role::Tool, Content::text(big.clone())),
    ];
    let sys_before = msgs[0].content.extract_text().unwrap();
    let user_before = msgs[1].content.extract_text().unwrap();
    let results = truncate_messages(&mut msgs, &cfg, |m| m.role == Role::Tool);
    assert_eq!(results.len(), 1);
    assert!(results[0].truncated);
    assert_eq!(msgs[0].content.extract_text().unwrap(), sys_before);
    assert_eq!(msgs[1].content.extract_text().unwrap(), user_before);
    let tool_text = msgs[2].content.extract_text().unwrap();
    assert!(!tool_text.contains(&big));
    assert!(tool_text.contains("truncated"));
}

#[test]
fn truncated_result_legacy_alias_deserializes() {
    let json = r#"{
        "content": "head\n[truncated]\ntail",
        "original_length": 1234,
        "truncated_length": 50,
        "truncated": true,
        "output_path": null
    }"#;
    let r: TruncatedResult = serde_json::from_str(json).unwrap();
    assert_eq!(r.output, "head\n[truncated]\ntail");
    assert_eq!(r.original_bytes, 1234);
    assert_eq!(r.output_bytes, 50);
    assert!(r.truncated);
}

#[test]
fn truncate_messages_handles_multi_part_content() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = TruncateConfig {
        max_bytes: 16,
        head_lines: 1,
        tail_lines: 1,
        temp_dir: tmp.path().to_path_buf(),
        ..Default::default()
    };
    let big = "z".repeat(200);
    let msg = Message::new(
        Role::Tool,
        Content::Multi(vec![ContentPart::Text(TextContent {
            text: big.clone(),
            cache_control: None,
        })]),
    );
    let mut msgs = vec![msg];
    let results = truncate_messages(&mut msgs, &cfg, |m| m.role == Role::Tool);
    assert_eq!(results.len(), 1);
    assert!(results[0].truncated);
}
