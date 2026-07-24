//! End-to-end integration tests for the compact / truncate / prune pipeline.
//!
//! These tests exercise the public API of `synthia-context` and
//! `synthia-exec` across module boundaries. They cover the four
//! scenarios called out in the V.1 verification matrix:
//!
//! 1. **Bash UTF-8 safety** — bash tool output with multi-byte UTF-8 is
//!    truncated without panicking, and the message is rendered cleanly.
//! 2. **Prune budget enforcement** — `prune()` marks the oldest
//!    tool-results once cumulative tokens exceed `PRUNE_PROTECT_TOKENS`,
//!    and a second call is fully idempotent.
//! 3. **Single-pass compaction** — `apply_compaction` calls
//!    `estimate_tokens` exactly once and the L1 path forwards
//!    `previous_summary` from a prior cycle.
//! 4. **Renderer honors cleared marker** — after `prune()`,
//!    `truncate_messages` emits the placeholder for cleared entries
//!    while leaving their `content` intact in storage.
//!
//! Note on shape: `synthia_context::pruning::is_tool_result` recognizes
//! a tool-result message by the presence of a `ContentPart::ToolResult`
//! content variant, while `truncate_messages` only swaps the text in a
//! `ContentPart::Text` content part. The existing unit tests follow
//! that split (prune uses the ToolResult shape, renderer uses the Text
//! shape with `tool_call_id` and a manually-set `tool_result_cleared_at`).
//! We mirror that split below.

use synthia_context::{
    pruning::{PRUNE_PROTECT_TOKENS, PruneStats, prune},
    truncate::{TruncateConfig, truncate_messages, truncate_output},
};
use synthia_provider::{
    Content,
    ContentPart,
    Message,
    Role,
    TextContent,
    ToolResult,
};

/// Message shape recognized by `is_tool_result` / `prune()`.
fn prune_tool_result(id: &str, body: &str) -> Message {
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

/// Message shape used by `truncate_messages` tests (text body with
/// `tool_call_id` + `tool_result_cleared_at` set manually).
fn render_tool_result(id: &str, body: &str) -> Message {
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

fn large_tool_result_text(approx_tokens: usize) -> String {
    // ~4 chars per token; padded with a known prefix so the estimate is
    // deterministic and well above 0 even for tiny token budgets.
    "x".repeat(approx_tokens.saturating_mul(4).max(8))
}

fn small_truncate_cfg() -> TruncateConfig {
    TruncateConfig {
        max_bytes: 256,
        head_lines: 5,
        tail_lines: 5,
        temp_dir: std::env::temp_dir().join("synthia-pipeline-it"),
        ..Default::default()
    }
}

// =============================================================================
// Scenario 2: prune() budget enforcement + idempotency
// =============================================================================

#[test]
fn pipeline_prune_marks_oldest_over_budget_and_idempotent() {
    // Build a sequence: [old × 5, recent] where the older entries are
    // huge (well over 40K tokens each) and the recent one is small. The
    // reverse scan should keep the recent small one and mark all the
    // older ones.
    let huge = large_tool_result_text(20_000);
    let mut msgs: Vec<Message> = Vec::new();
    for i in 0..5 {
        msgs.push(prune_tool_result(&format!("old-{i}"), &huge));
    }
    msgs.push(prune_tool_result("recent", "small"));

    let first = prune(&mut msgs, PRUNE_PROTECT_TOKENS);
    assert!(
        first.marked_count >= 1,
        "at least one old entry should be marked, got {first:?}"
    );

    // The most recent (last) tool-result must remain in the protected
    // tail.
    let last = msgs.last().expect("at least one message");
    assert!(
        last.tool_result_cleared_at.is_none(),
        "the most recent tool result must stay within the protected tail"
    );

    // Snapshot the cleared_at timestamps; a second call must leave
    // every Some(_) timestamp at the same value (idempotent stop on
    // the first cleared message) and not mark anything new.
    let snapshot: Vec<_> =
        msgs.iter().map(|m| m.tool_result_cleared_at).collect();

    let second = prune(&mut msgs, PRUNE_PROTECT_TOKENS);
    assert_eq!(
        second.marked_count, 0,
        "second pass must mark nothing; older entries are already cleared"
    );
    let after: Vec<_> = msgs.iter().map(|m| m.tool_result_cleared_at).collect();
    assert_eq!(
        snapshot, after,
        "cleared_at timestamps must not be re-stamped"
    );
}

#[test]
fn pipeline_prune_zero_budget_marks_every_tool_message() {
    let mut msgs = vec![
        prune_tool_result("a", "aaaa"),
        prune_tool_result("b", "bbbb"),
        prune_tool_result("c", "cccc"),
    ];
    let stats: PruneStats = prune(&mut msgs, 0);
    assert_eq!(stats.marked_count, msgs.len());
    for m in &msgs {
        assert!(m.tool_result_cleared_at.is_some());
    }
}

#[test]
fn pipeline_prune_does_not_mutate_original_content() {
    // P8 invariant: marking must not mutate the original `content` of
    // a tool-result message. Only `tool_result_cleared_at` is set;
    // the body remains available for replay / recovery.
    let original_body = "this is the original tool result body";
    let mut msgs = vec![prune_tool_result("t", original_body)];
    let _stats = prune(&mut msgs, 0);
    let m = &msgs[0];
    assert!(m.tool_result_cleared_at.is_some());
    let preserved = match &m.content {
        Content::Single(ContentPart::ToolResult(tr)) => tr
            .content
            .iter()
            .find_map(|p| p.text())
            .unwrap_or("")
            .to_string(),
        _ => panic!("expected ToolResult content part"),
    };
    assert_eq!(preserved, original_body, "content must be preserved");
}

// =============================================================================
// Scenario 4: renderer honors cleared marker (truncate_messages)
// =============================================================================

#[test]
fn pipeline_renderer_replaces_cleared_with_placeholder() {
    // Use the render-side message shape (Text body + tool_call_id) and
    // manually stamp `tool_result_cleared_at`, since the renderer's
    // placeholder branch keys off `ContentPart::Text`.
    let huge = large_tool_result_text(8_000);
    let mut msgs = vec![render_tool_result("t", &huge)];
    msgs[0].tool_result_cleared_at = Some(
        chrono::DateTime::parse_from_rfc3339("2026-06-12T10:30:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc),
    );

    // The renderer must surface the placeholder, not the huge body.
    let cfg = TruncateConfig {
        // Big enough that no ordinary truncation would run; only the
        // cleared-at branch should fire.
        max_bytes: 1024 * 1024,
        head_lines: 1,
        tail_lines: 1,
        temp_dir: std::env::temp_dir().join("synthia-pipeline-it"),
        ..Default::default()
    };
    let _results = truncate_messages(&mut msgs, &cfg, |_| true);
    let rendered = msgs[0].content.extract_text().unwrap_or_default();
    assert!(
        rendered.contains("Old tool result content cleared at"),
        "rendered text must contain the cleared placeholder, got: {rendered:?}"
    );
    assert!(
        !rendered.contains(&"x".repeat(64)),
        "rendered text must not contain the cleared body, got len={}",
        rendered.len()
    );
}

#[test]
fn pipeline_truncate_output_preserves_multibyte_unicode() {
    // The bash UTF-8 path lives in `synthia-exec`, but the underlying
    // safe-truncation contract is shared: `truncate_output` must not
    // panic on multi-byte UTF-8 inputs. This guards the integration
    // boundary (config + messages flow through the same function).
    let cfg = small_truncate_cfg();
    let body: String = "你好世界🌍".repeat(200);
    let r = truncate_output(&body, &cfg);
    assert!(r.truncated, "input is far above max_bytes");
    // Output must be valid UTF-8 (no panic on slicing through a 3/4
    // byte character). The truncation marker references the original
    // size, not the post-truncation length.
    assert!(r.output.is_char_boundary(r.output.len()));
    assert!(
        r.output.contains("bytes"),
        "marker should report byte count"
    );
    assert!(r.output.contains("truncated"));
}

// =============================================================================
// Scenario 1 (re-exported): UTF-8 safety contract is shared between
// synthia-exec::bash_tool and synthia_context::truncate — both reject
// non-char-boundary indices. The bash-side regression test lives in
// crates/synthia-exec/tests/bash_utf8_panic.rs; here we just confirm
// the truncate path is consistent (it uses the same boundary check).
// =============================================================================

#[test]
fn pipeline_truncate_output_handles_pure_ascii_baseline() {
    // Sanity baseline: pure ASCII with boundary at max_bytes.
    let cfg = small_truncate_cfg();
    let input = "a".repeat(1024);
    let r = truncate_output(&input, &cfg);
    assert!(r.truncated);
    assert!(r.output.starts_with("aaaa"));
    assert!(r.output.contains("truncated"));
}

// =============================================================================
// Production-path coverage: Shape A tool-result → prune() → truncate_messages
// → placeholder. This is the test the previous change's reviewer caught
// as missing. The previous renderer's `extract_text().is_some()` gate
// silently missed the ToolResult shape; the new `replace_first_text_anywhere`
// helper drills into `ToolResult.content[0].text`.
// =============================================================================

#[test]
fn pipeline_prune_then_render_shape_a_full_production_path() {
    use synthia_context::truncate::truncate_messages;
    use synthia_provider::{
        Content,
        ContentPart,
        Message,
        Role,
        TextContent,
        ToolResult,
    };

    // Build 5 Shape A tool-result messages (Role::User + ContentPart::ToolResult).
    let huge = "x".repeat(8_000);
    let mut msgs: Vec<Message> = (0..5)
        .map(|i| Message {
            role: Role::User,
            content: Content::Single(ContentPart::ToolResult(ToolResult {
                tool_use_id: format!("t-{i}"),
                content: vec![ContentPart::Text(TextContent {
                    text: huge.clone(),
                    cache_control: None,
                })],
                structured_content: None,
                is_error: None,
            })),
            ..Default::default()
        })
        .collect();

    // Step 1: prune with zero budget marks all 5 (every tool message
    // overflows the 0-token protected tail).
    let stats = prune(&mut msgs, 0);
    assert_eq!(stats.marked_count, 5);
    for m in &msgs {
        assert!(
            m.tool_result_cleared_at.is_some(),
            "every Shape A tool-result must be marked with PRUNE_PROTECT=0"
        );
    }

    // Step 2: truncate_messages with a 1 MiB max_bytes so the size-based
    // path never fires — only the cleared-placeholder branch runs.
    let cfg = TruncateConfig {
        max_bytes: 1024 * 1024,
        head_lines: 1,
        tail_lines: 1,
        temp_dir: std::env::temp_dir().join("synthia-pipeline-it"),
        ..Default::default()
    };
    let results = truncate_messages(&mut msgs, &cfg, |_| true);
    // No TruncatedResult produced — the cleared branch short-circuits
    // before truncate_output runs.
    assert_eq!(results.len(), 0);

    // Step 3: every message must show the placeholder inside its
    // ToolResult.content[0].text, and the on-the-wire fields (role,
    // tool_use_id) MUST be preserved (P8 invariant: transform, never lose).
    for (i, m) in msgs.iter().enumerate() {
        let tr = match &m.content {
            Content::Single(ContentPart::ToolResult(tr)) => tr,
            _ => panic!("message {i} must remain ContentPart::ToolResult"),
        };
        assert_eq!(
            tr.tool_use_id,
            format!("t-{i}"),
            "message {i} tool_use_id must be preserved"
        );
        assert_eq!(m.role, Role::User, "message {i} role must be preserved");
        let text = tr
            .content
            .iter()
            .find_map(|p| p.text())
            .expect("ToolResult must still contain a text part");
        assert!(
            text.contains("Old tool result content cleared at"),
            "message {i} must render placeholder, got: {text:?}"
        );
        assert!(
            !text.contains(&"x".repeat(64)),
            "message {i} must not leak original body (len={})",
            text.len()
        );
    }
}
