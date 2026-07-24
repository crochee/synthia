//! Single-string truncation entry point —
//! [`truncate_output`] — plus the private `build_marker`
//! helper.
//!
//! The two public helpers ([`truncate_output`] +
//! [`super::truncate_messages::truncate_messages`]) form
//! the public API. This module owns the simple
//! `content: &str -> TruncatedResult` variant; the
//! per-message variant lives in
//! [`super::truncate_messages`].

use std::path::PathBuf;

use tracing::warn;

use super::{
    lines::{cap_lines, split_head_tail},
    spill::spill_to_disk,
    types::{TruncateConfig, TruncatedResult},
};

/// Truncate `content` to fit `cfg.max_bytes`, preserving the first
/// `head_lines` and the last `tail_lines` lines. The full content is
/// written to `cfg.temp_dir` and referenced by a marker line in `output`.
///
/// Disk write failures degrade gracefully: the in-memory truncated output
/// is still returned, with `output_path = None` and a warning logged.
pub fn truncate_output(content: &str, cfg: &TruncateConfig) -> TruncatedResult {
    if content.is_empty() {
        return TruncatedResult::passthrough(content);
    }

    let lines: Vec<&str> = content.split_inclusive('\n').collect();
    if content.len() <= cfg.max_bytes && lines.len() <= cfg.max_lines {
        return TruncatedResult::passthrough(content);
    }
    let (head_raw, tail_raw) =
        split_head_tail(&lines, cfg.head_lines, cfg.tail_lines);
    // If the head and tail are themselves larger than the budget, byte-trim
    // each half. The marker sits in the middle and pushes the total above
    // max_bytes, but the visible head/tail stay bounded.
    let per_half = cfg.max_bytes / 2;
    let head = cap_lines(&head_raw, per_half);
    let tail = cap_lines(&tail_raw, per_half);

    let (output_path, write_ok) = match spill_to_disk(
        content,
        &cfg.temp_dir,
        cfg.session_id.as_deref(),
        cfg.tool_call_id.as_deref(),
    ) {
        Ok(path) => (Some(path), true),
        Err(err) => {
            warn!(
                target: "synthia.context.truncate",
                error = %err,
                temp_dir = %cfg.temp_dir.display(),
                "truncate_output: failed to spill full content to disk; returning head/tail only",
            );
            (None, false)
        }
    };

    let marker = build_marker(
        content.len(),
        lines.len(),
        output_path.as_ref(),
        write_ok,
    );

    let mut output = String::with_capacity(
        head.iter().map(|s| s.len()).sum::<usize>()
            + marker.len()
            + tail.iter().map(|s| s.len()).sum::<usize>(),
    );
    for line in head {
        output.push_str(line);
    }
    output.push_str(&marker);
    for line in tail {
        output.push_str(line);
    }

    let output_bytes = output.len();
    TruncatedResult {
        output,
        original_bytes: content.len(),
        output_bytes,
        truncated: true,
        output_path,
    }
}

fn build_marker(
    original_bytes: usize,
    total_lines: usize,
    output_path: Option<&PathBuf>,
    write_ok: bool,
) -> String {
    if write_ok {
        let path = output_path
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "<unknown>".to_string());
        format!(
            "\n[... {original_bytes} bytes / {total_lines} lines truncated; full output at {path} ...]\n"
        )
    } else {
        format!(
            "\n[... {original_bytes} bytes / {total_lines} lines truncated; disk write failed ...]\n"
        )
    }
}
