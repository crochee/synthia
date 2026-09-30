//! Tiny string helpers shared by every section renderer
//! and the top-level assembler. Kept in one file so the
//! `\n\n` join logic stays in a single place — if a future
//! format change is needed (e.g. add trailing newline, swap
//! separator), there is exactly one call site to update.

/// Wrap `body` in a paired XML tag. The body is taken verbatim
/// — no trimming, no escaping. Caller is responsible for
/// ensuring the body does not contain `</…>` of the same tag
/// (it cannot: each tag name is a literal we control).
pub(crate) fn wrap(tag: &str, body: &str) -> String {
    format!("<{tag}>\n{body}\n</{tag}>")
}

/// `Some(s)` when `s.trim()` is non-empty, else `None`.
pub(crate) fn trimmed_non_empty(s: &str) -> Option<&str> {
    let t = s.trim();
    if t.is_empty() { None } else { Some(t) }
}

/// Append `block` to `out`, preceded by `\n\n` if not the
/// first block. The `first` flag is `mut` so the assembler
/// can thread it through a single pass without an extra
/// boolean per section.
pub(crate) fn push_block(out: &mut String, first: &mut bool, block: &str) {
    if !*first {
        out.push_str("\n\n");
    }
    out.push_str(block);
    *first = false;
}
