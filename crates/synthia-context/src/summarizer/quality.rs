//! Summary quality checking: file paths, user requests, key decisions.

use std::sync::LazyLock;

use synthia_provider::Message;

use crate::types::SummaryQuality;

static FILE_PATH_PATTERNS: LazyLock<Vec<(&'static str, &'static str)>> =
    LazyLock::new(|| {
        vec![
            ("src/", ".rs"),
            ("tests/", ".rs"),
            ("examples/", ".rs"),
            ("benches/", ".rs"),
            ("crates/", "/"),
            ("/home/", "/"),
            ("/usr/", "/"),
            ("/var/", "/"),
            ("/tmp/", "/"),
            (".", ".rs"),
            (".", ".toml"),
            (".", ".json"),
            (".", ".yaml"),
            (".", ".yml"),
            (".", ".md"),
        ]
    });

static REQUEST_KEYWORDS: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
    vec![
        "user request",
        "user asked",
        "user wants",
        "user need",
        "requested",
        "require",
        "implement",
        "add ",
        "modify",
        "update",
        "delete",
        "remove",
        "create",
        "fix",
        "change",
        "please",
        "want to",
        "need to",
        "would like",
    ]
});

static DECISION_KEYWORDS: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
    vec![
        "decided",
        "decision",
        "conclusion",
        "agreed",
        "chosen",
        "selected",
        "opted",
        "determined",
        "resolved",
        "will",
        "plan to",
        "going to",
        "approach",
        "strategy",
        "solution",
        "recommend",
        "suggest",
    ]
});

/// Check if summary contains file paths
fn has_file_paths(summary: &str) -> bool {
    FILE_PATH_PATTERNS.iter().any(|(prefix, suffix)| {
        summary.contains(prefix) && summary.contains(suffix)
    })
}

/// Check if summary contains user requests
fn has_user_requests(summary: &str) -> bool {
    let summary_lower = summary.to_lowercase();
    REQUEST_KEYWORDS
        .iter()
        .any(|keyword| summary_lower.contains(*keyword))
}

/// Check if summary contains key decisions
fn has_key_decisions(summary: &str) -> bool {
    let summary_lower = summary.to_lowercase();
    DECISION_KEYWORDS
        .iter()
        .any(|keyword| summary_lower.contains(*keyword))
}

/// Check summary quality
pub fn check_summary_quality(
    summary: &str,
    original: &[Message],
) -> SummaryQuality {
    let required_sections = ["## Summary", "## User Intent", "## Current Work"];

    let has_required_sections = required_sections
        .iter()
        .all(|section| summary.contains(section));

    // Check identifier integrity (basic check for common patterns)
    let identifier_integrity = check_identifier_integrity(summary);

    // Check user request reflection
    let user_request_reflected =
        summary.contains("User Intent") || summary.contains("user");

    // Check for file paths in summary
    let summary_has_file_paths = has_file_paths(summary);

    // Check for user requests preservation
    let summary_has_user_requests = has_user_requests(summary);

    // Check for key decisions
    let summary_has_key_decisions = has_key_decisions(summary);

    // If original messages contain file paths but summary doesn't, that's a quality issue
    let original_has_files = original.iter().any(|msg| {
        let text = extract_text_content(msg);
        has_file_paths(&text)
    });

    // If original has file paths but summary doesn't, mark as missing file paths
    let effective_has_file_paths = if original_has_files {
        summary_has_file_paths
    } else {
        true
    };

    SummaryQuality::new(
        has_required_sections,
        identifier_integrity,
        user_request_reflected,
        effective_has_file_paths,
        summary_has_user_requests,
        summary_has_key_decisions,
    )
}

/// Basic identifier integrity check
fn check_identifier_integrity(summary: &str) -> bool {
    // Check for common identifier patterns
    let has_file_refs = summary.contains(".") && summary.contains("/");
    let has_code_blocks = summary.contains("```");
    let has_structure = summary.contains("##");

    // At least one structural element should be present
    has_structure || has_file_refs || has_code_blocks
}

fn extract_text_content(msg: &Message) -> String {
    msg.content.extract_text().unwrap_or_default()
}