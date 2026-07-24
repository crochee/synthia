//! Private line-shaping helpers used by
//! [`super::truncate_output`]:
//!
//! - `split_head_tail` — pick the first `head_lines` and
//!   last `tail_lines` lines from a pre-split slice (the
//!   middle is discarded; the discarded content is the
//!   whole point of truncation).
//! - `cap_lines` — byte-budget the resulting head / tail
//!   slices so the final `output` size is bounded even
//!   when the user passes a very large
//!   `cfg.max_bytes` to [`super::truncate_output`].

/// Return `(head, tail)` slices of the first `head_lines`
/// and last `tail_lines` entries of `lines`. If the total
/// fits in `head_lines + tail_lines`, return the whole
/// `lines` as `head` and an empty `tail` (no truncation
/// needed at the line level — the size-based
/// `truncate_output` wrapper will then decide whether
/// the joined bytes still exceed `cfg.max_bytes`).
pub(super) fn split_head_tail<'a>(
    lines: &'a [&'a str],
    head_lines: usize,
    tail_lines: usize,
) -> (Vec<&'a str>, Vec<&'a str>) {
    if lines.len() <= head_lines + tail_lines {
        return (lines.to_vec(), Vec::new());
    }
    let head = lines[..head_lines].to_vec();
    let tail_start = lines.len().saturating_sub(tail_lines);
    let tail = lines[tail_start..].to_vec();
    (head, tail)
}

/// Cap `lines` so the joined byte length does not exceed `byte_budget`.
/// If the budget cannot accommodate even a single line, the function takes
/// the prefix of the first line that fits.
pub(super) fn cap_lines<'a>(
    lines: &[&'a str],
    byte_budget: usize,
) -> Vec<&'a str> {
    let mut out: Vec<&'a str> = Vec::with_capacity(lines.len());
    let mut used = 0usize;
    for line in lines {
        if used + line.len() > byte_budget {
            // Try to slice within the first overflowing line.
            if out.is_empty() && byte_budget > 0 {
                let take = byte_budget.min(line.len());
                // SAFETY/justification: we are building an internal preview
                // string; slicing at a non-char boundary could happen on
                // multi-byte UTF-8, so we floor to the previous char boundary.
                let mut end = take;
                while end > 0 && !line.is_char_boundary(end) {
                    end -= 1;
                }
                out.push(&line[..end]);
            }
            break;
        }
        out.push(line);
        used += line.len();
    }
    out
}
