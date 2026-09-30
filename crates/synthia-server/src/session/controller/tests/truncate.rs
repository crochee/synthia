//! `truncate` truncates a string to at most
//! `max_chars` Unicode scalar values, appending
//! a Unicode ellipsis (`…`) when truncation
//! actually happened. Char-count (not
//! byte-count) is the contract so the function
//! behaves correctly for multi-byte text
//! (中文 / 日本語).
//!
//! No test pins this today; a refactor that
//! switched to byte-indexing would silently
//! corrupt non-ASCII log previews.

use super::super::run_log::truncate;

#[test]
fn truncate_shorter_than_max_is_returned_verbatim() {
    assert_eq!(truncate("hi", 5), "hi");
    assert_eq!(truncate("hello", 5), "hello");
}

#[test]
fn truncate_equal_to_max_is_returned_verbatim() {
    // Boundary: exactly max_chars → no
    // truncation. The `iter.next()` check
    // after the loop is `None` (no extra
    // char), so no `…` is appended.
    assert_eq!(truncate("hello", 5), "hello");
}

#[test]
fn truncate_one_over_max_is_truncated_with_ellipsis() {
    assert_eq!(truncate("hello!", 5), "hello…");
}

#[test]
fn truncate_empty_string_is_returned_verbatim() {
    assert_eq!(truncate("", 0), "");
    assert_eq!(truncate("", 5), "");
}

#[test]
fn truncate_max_zero_with_nonempty_returns_ellipsis_only() {
    // The `for _ in 0..0` loop is a no-op.
    // `iter.next()` returns Some('x'), so
    // the ellipsis branch fires with an
    // empty `out`. Pin this so a refactor
    // doesn't swallow the ellipsis on
    // zero-width truncation.
    assert_eq!(truncate("x", 0), "…");
}

#[test]
fn truncate_counts_chars_not_bytes_for_multibyte_text() {
    // "中文" is 2 chars but 6 bytes (UTF-8).
    // With max=2 we MUST return "中文"
    // (no truncation) rather than
    // byte-truncated garbage.
    assert_eq!(truncate("中文", 2), "中文");
    assert_eq!(truncate("中文!", 2), "中文…");
    assert_eq!(truncate("日本語テスト", 3), "日本語…");
}

#[test]
fn truncate_does_not_append_ellipsis_on_exact_match() {
    // Distinguish "exact match" from "one
    // over": the former MUST NOT have the
    // ellipsis appended.
    let s = "abcde";
    assert_eq!(truncate(s, 5), "abcde");
    assert!(
        !truncate(s, 5).ends_with('…'),
        "exact match must not get an ellipsis"
    );
}
