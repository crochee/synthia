//! Async bound_output implementation — truncation + managed file spill.
//!
//! Operates on [`crate::types::ToolOutput`] which uses
//! `synthia_provider::types::ContentPart` for content parts.

use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use tokio::{fs, io::AsyncWriteExt, time};

use crate::types::{ToolOutput, TruncatedBy};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OutputBound {
    pub per_call_max_bytes: usize,
    pub per_call_max_lines: usize,
    pub managed_dir: PathBuf,
    pub overflow_strategy: OverflowStrategy,
    pub retention: Duration,
    pub cleanup_interval: Duration,
    pub sanitization: SanitizationPolicy,
}

impl Default for OutputBound {
    fn default() -> Self {
        Self {
            per_call_max_bytes: 50 * 1024,
            per_call_max_lines: 2000,
            managed_dir: PathBuf::from("/tmp/synthia-managed"),
            overflow_strategy: OverflowStrategy::TruncateHeadTail,
            retention: Duration::from_secs(7 * 24 * 3600),
            cleanup_interval: Duration::from_secs(3600),
            sanitization: SanitizationPolicy::StripControlChars,
        }
    }
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Default,
    serde::Serialize,
    serde::Deserialize,
)]
pub enum OverflowStrategy {
    #[default]
    TruncateHeadTail,
    TruncateHead,
    AlwaysSpill,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Default,
    serde::Serialize,
    serde::Deserialize,
)]
pub enum SanitizationPolicy {
    #[default]
    StripControlChars,
    WrapUntrusted,
}

/// Apply output bounds to a tool output.
///
/// Truncates content if it exceeds per-call limits, and spills
/// overflow to a managed file via `tokio::fs`.
pub async fn bound_output(
    output: &mut ToolOutput,
    config: &OutputBound,
    session_id: &str,
    tool_name: &str,
) -> Result<(), std::io::Error> {
    // Apply sanitization first
    apply_sanitization(output, &config.sanitization);

    // Process each content part
    for part in &mut output.content {
        if let Some(text) = part.text_mut() {
            let total_bytes = text.len();
            let total_lines = text.lines().count();

            if total_bytes > config.per_call_max_bytes
                || total_lines > config.per_call_max_lines
            {
                let original_bytes = total_bytes;
                match config.overflow_strategy {
                    OverflowStrategy::TruncateHeadTail => {
                        let (truncated, managed_path) = truncate_head_tail(
                            text,
                            config.per_call_max_bytes,
                            config.per_call_max_lines,
                            &config.managed_dir,
                            session_id,
                            tool_name,
                        )
                        .await?;
                        *text = truncated;
                        if let Some(path) = managed_path {
                            output.truncated_by =
                                Some(TruncatedBy::SpilledTo {
                                    path: path.to_string_lossy().to_string(),
                                });
                            output.metadata.insert(
                                "managed_path".to_string(),
                                serde_json::Value::String(
                                    path.to_string_lossy().to_string(),
                                ),
                            );
                        }
                    }
                    OverflowStrategy::TruncateHead => {
                        let original_lines = text.lines().count();
                        truncate_in_place(text, config.per_call_max_bytes);
                        output.truncated_by = Some(TruncatedBy::Lines {
                            shown: text.lines().count(),
                            total: original_lines,
                        });
                    }
                    OverflowStrategy::AlwaysSpill => {
                        let managed_path = spill_to_file(
                            text,
                            &config.managed_dir,
                            session_id,
                            tool_name,
                        )
                        .await?;
                        *text = format!(
                            "[Output spilled to managed file: {}]\n\
                             Use the managed file path to read full output.",
                            managed_path.display()
                        );
                        output.truncated_by = Some(TruncatedBy::SpilledTo {
                            path: managed_path.to_string_lossy().to_string(),
                        });
                        output.metadata.insert(
                            "managed_path".to_string(),
                            serde_json::Value::String(
                                managed_path.to_string_lossy().to_string(),
                            ),
                        );
                    }
                }
                output.metadata.insert(
                    "original_bytes".to_string(),
                    serde_json::json!(original_bytes),
                );
                output.metadata.insert(
                    "output_bytes".to_string(),
                    serde_json::json!(text.len()),
                );
            }
        }
    }

    Ok(())
}

/// Start the managed file cleanup background task.
pub fn start_cleanup_task(
    managed_dir: PathBuf,
    retention: Duration,
    cleanup_interval: Duration,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = time::interval(cleanup_interval);
        loop {
            interval.tick().await;
            if let Err(e) = cleanup_managed_files(&managed_dir, retention).await
            {
                tracing::warn!("managed file cleanup failed: {}", e);
            }
        }
    })
}

/// Apply sanitization policy to output content.
fn apply_sanitization(output: &mut ToolOutput, policy: &SanitizationPolicy) {
    match policy {
        SanitizationPolicy::StripControlChars => {
            for part in &mut output.content {
                if let Some(text) = part.text_mut() {
                    *text = strip_control_chars(text);
                }
            }
        }
        SanitizationPolicy::WrapUntrusted => {
            for part in &mut output.content {
                if let Some(text) = part.text_mut() {
                    // Remove any existing isolation tags first
                    *text = text.replace("<user_denial_feedback>", "");
                    *text = text.replace("</user_denial_feedback>", "");
                    *text = format!(
                        "<user_denial_feedback>{}</user_denial_feedback>",
                        text
                    );
                }
            }
        }
    }
}

/// Strip ASCII control characters except \n, \r, \t.
fn strip_control_chars(s: &str) -> String {
    s.chars()
        .filter(|&c| !c.is_control() || c == '\n' || c == '\r' || c == '\t')
        .collect()
}

/// Truncate keeping head + tail, spill middle to managed file.
async fn truncate_head_tail(
    text: &str,
    max_bytes: usize,
    max_lines: usize,
    managed_dir: &Path,
    session_id: &str,
    tool_name: &str,
) -> Result<(String, Option<PathBuf>), std::io::Error> {
    let lines: Vec<&str> = text.lines().collect();
    let total_lines = lines.len();

    if total_lines <= max_lines && text.len() <= max_bytes {
        return Ok((text.to_string(), None));
    }

    // 60% head, 40% tail
    let head_lines = (max_lines as f64 * 0.6) as usize;
    let tail_lines = max_lines - head_lines;

    let head: String = lines[..head_lines.min(total_lines)].join("\n");
    let tail_start = total_lines.saturating_sub(tail_lines);
    let tail: String = if tail_start > head_lines {
        lines[tail_start..].join("\n")
    } else {
        String::new()
    };

    // Spill full content to managed file
    let managed_path =
        spill_to_file(text, managed_dir, session_id, tool_name).await?;

    let truncated = format!(
        "{}\n\n... [{} lines truncated, full output at: {}] ...\n\n{}",
        head,
        // Single-line oversized text (e.g. a `minified.json`
        // blob) can have `total_lines = 1` while
        // `head_lines + tail_lines` sums to many thousands,
        // which would underflow an unchecked `usize`
        // subtraction. `saturating_sub` reports `0` lines
        // truncated in that case — the user still gets a
        // head-only view above plus the managed file pointer,
        // which is correct: there really were no "middle" lines
        // to drop.
        total_lines
            .saturating_sub(head_lines)
            .saturating_sub(tail_lines),
        managed_path.display(),
        tail
    );

    Ok((truncated, Some(managed_path)))
}

/// Truncate in place (head only).
fn truncate_in_place(text: &mut String, max_bytes: usize) {
    if text.len() <= max_bytes {
        return;
    }
    // Find a char boundary near max_bytes
    let truncation_point = max_bytes;
    let boundary = if text.is_char_boundary(truncation_point) {
        truncation_point
    } else {
        // Walk back to nearest char boundary
        let mut p = truncation_point;
        while p > 0 && !text.is_char_boundary(p) {
            p -= 1;
        }
        p
    };
    text.truncate(boundary);
    text.push_str("\n... [truncated]");
}

/// Spill full text content to a managed file.
async fn spill_to_file(
    text: &str,
    managed_dir: &Path,
    session_id: &str,
    tool_name: &str,
) -> Result<PathBuf, std::io::Error> {
    fs::create_dir_all(managed_dir).await?;

    let timestamp = chrono::Utc::now().format("%Y%m%d%H%M%S");
    let filename = format!("{}-{}-{}.txt", session_id, tool_name, timestamp);
    let path = managed_dir.join(&filename);

    let mut file = fs::File::create(&path).await?;
    file.write_all(text.as_bytes()).await?;
    file.flush().await?;

    Ok(path)
}

/// Remove managed files older than retention period.
async fn cleanup_managed_files(
    managed_dir: &Path,
    retention: Duration,
) -> Result<(), std::io::Error> {
    let cutoff = std::time::SystemTime::now() - retention;

    let mut entries = fs::read_dir(managed_dir).await?;
    while let Some(entry) = entries.next_entry().await? {
        let metadata = entry.metadata().await?;
        if let Ok(modified) = metadata.modified()
            && modified < cutoff
        {
            let path = entry.path();
            if let Err(e) = fs::remove_file(&path).await {
                tracing::warn!(
                    "failed to remove managed file {:?}: {}",
                    path,
                    e
                );
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `strip_control_chars` is the MVP
    /// sanitization policy applied to tool output
    /// before it reaches the LLM. Pin the exact
    /// contract: drop control chars EXCEPT
    /// `\n`, `\r`, `\t` (which are legitimate
    /// line-formatting). This is a refactor
    /// hot-spot — switching to
    /// `c.is_control()` alone would also drop
    /// newlines, producing one giant line.
    #[test]
    fn strip_control_chars_preserves_newline_and_tab() {
        assert_eq!(strip_control_chars("a\nb\tc"), "a\nb\tc");
        assert_eq!(strip_control_chars("a\rb"), "a\rb");
        // \0 (NULL) is a control char — dropped.
        assert_eq!(strip_control_chars("a\0b"), "ab");
        // BEL (\x07) is a control char — dropped.
        assert_eq!(strip_control_chars("a\x07b"), "ab");
        // DEL (\x7f) is a control char — dropped.
        assert_eq!(strip_control_chars("a\x7fb"), "ab");
        // Mixed legitimate + control: keep legit.
        assert_eq!(
            strip_control_chars("line1\nline2\x00\tcol"),
            "line1\nline2\tcol"
        );
    }

    /// `strip_control_chars` on empty string is
    /// empty (no panic).
    #[test]
    fn strip_control_chars_empty_string_returns_empty() {
        assert_eq!(strip_control_chars(""), "");
    }

    /// `truncate_in_place` is the head-only
    /// truncation for `OverflowStrategy::TruncateHead`.
    /// Pin the 3 contracts:
    ///   - text shorter than max_bytes is left
    ///     verbatim (no truncation marker)
    ///   - text exactly max_bytes is left verbatim
    ///     (boundary case)
    ///   - text longer than max_bytes is cut AND
    ///     gets a `"\n... [truncated]"` marker
    #[test]
    fn truncate_in_place_shorter_than_max_is_verbatim() {
        let mut s = String::from("hi");
        truncate_in_place(&mut s, 100);
        assert_eq!(s, "hi");
        assert!(!s.contains("truncated"));
    }

    #[test]
    fn truncate_in_place_exact_max_bytes_is_verbatim() {
        let mut s = String::from("hello");
        truncate_in_place(&mut s, 5);
        assert_eq!(s, "hello");
    }

    #[test]
    fn truncate_in_place_one_over_max_bytes_is_truncated_with_marker() {
        let mut s = String::from("hello world");
        truncate_in_place(&mut s, 5);
        // The implementation truncates at 5 bytes
        // (char-boundary-safe), then appends a
        // marker. Pin the exact post-condition:
        // marker must be present and the leading
        // 5 bytes ("hello") must remain.
        assert!(
            s.starts_with("hello"),
            "first 5 bytes must be preserved; got {s:?}"
        );
        assert!(
            s.contains("truncated"),
            "truncation marker must be appended; got {s:?}"
        );
        assert!(
            s.ends_with("[truncated]"),
            "marker must end with [truncated]; got {s:?}"
        );
    }

    /// `truncate_in_place` is char-boundary-safe.
    /// If `max_bytes` falls inside a multi-byte
    /// UTF-8 sequence, the function MUST walk
    /// back to a char boundary (otherwise it
    /// would panic on `String::truncate`). Pin
    /// the contract with a multi-byte string.
    #[test]
    fn truncate_in_place_handles_multibyte_char_boundary_safely() {
        // "中文" is 6 bytes (3 each). max_bytes=4
        // falls mid-codepoint; the function must
        // walk back to byte 3 (the boundary
        // between the two codepoints).
        let mut s = String::from("中文!"); // 7 bytes
        truncate_in_place(&mut s, 4);
        // The implementation must NOT panic. The
        // exact prefix length is byte-boundary
        // dependent; the contract we pin is
        // "doesn't panic, has a marker, and
        // contains the marker".
        assert!(
            s.contains("[truncated]"),
            "marker must be present; got {s:?}"
        );
        assert!(!s.is_empty(), "must retain some prefix");
    }

    /// `truncate_in_place` with `max_bytes=0`
    /// leaves nothing of the original (just the
    /// marker). Pin the edge case.
    #[test]
    fn truncate_in_place_zero_max_bytes_yields_marker_only() {
        let mut s = String::from("hi");
        truncate_in_place(&mut s, 0);
        assert!(s.contains("[truncated]"));
        assert!(
            !s.contains("hi"),
            "all original bytes must be dropped when max=0"
        );
    }

    /// Regression for the `bound_output` panic on
    /// `attempt to subtract with overflow` at
    /// `bound_output.rs:254`. Triggered when a tool
    /// returns a single very-long line (e.g. a
    /// minified JSON blob, a `wc -c` dump, a stack
    /// trace pasted into one line): `total_lines`
    /// is small (often 1) but
    /// `head_lines + tail_lines` is computed from
    /// `max_lines`, so an unchecked
    /// `total_lines - head_lines - tail_lines`
    /// underflows the `usize`. `saturating_sub`
    /// reports `0` lines truncated in that case.
    #[tokio::test]
    async fn truncate_head_tail_does_not_panic_on_single_oversized_line() {
        // 1 line, 10 KB — exceeds per_call_max_bytes but
        // total_lines = 1 ≪ per_call_max_lines = 2000.
        let one_big_line = "x".repeat(10 * 1024);
        let tmp = std::env::temp_dir().join("synthia-bound-output-test");
        let (truncated, managed) = truncate_head_tail(
            &one_big_line,
            4 * 1024,
            2000,
            &tmp,
            "session-test",
            "shell",
        )
        .await
        .expect("truncate_head_tail must not fail on a single long line");
        // Pin the regression: the function returned
        // without panicking. The head slice should
        // contain the input prefix; the "[N lines
        // truncated]" marker must report 0 (not underflow
        // into usize::MAX or panic).
        assert!(truncated.contains("0 lines truncated"));
        assert!(managed.is_some(), "overflow must spill to a managed file");
    }
}

#[cfg(test)]
mod default_tests {
    use std::time::Duration;

    use super::*;

    /// `OutputBound::default()` MUST
    /// set the documented defaults
    /// for each of its 9 fields.
    /// This pins the documented
    /// spec at the type level so
    /// silent regressions surface
    /// immediately.
    #[test]
    fn default_yields_documented_field_values() {
        let b = OutputBound::default();
        assert_eq!(b.per_call_max_bytes, 50 * 1024);
        assert_eq!(b.per_call_max_lines, 2000);
        assert_eq!(b.managed_dir, PathBuf::from("/tmp/synthia-managed"));
        assert!(matches!(
            b.overflow_strategy,
            OverflowStrategy::TruncateHeadTail
        ));
        assert_eq!(b.retention, Duration::from_secs(7 * 24 * 3600));
        assert_eq!(b.cleanup_interval, Duration::from_secs(3600));
        assert!(matches!(
            b.sanitization,
            SanitizationPolicy::StripControlChars
        ));
    }

    /// `OutputBound` MUST round-trip
    /// through JSON (regression
    /// test for serde-derived
    /// Serialize + Deserialize).
    #[test]
    fn output_bound_round_trips_through_json() {
        let b = OutputBound::default();
        let json = serde_json::to_string(&b).expect("serialize");
        let parsed: OutputBound =
            serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.per_call_max_bytes, b.per_call_max_bytes);
        assert_eq!(parsed.per_call_max_lines, b.per_call_max_lines);
        assert_eq!(parsed.managed_dir, b.managed_dir);
        assert_eq!(parsed.retention, b.retention);
        assert_eq!(parsed.cleanup_interval, b.cleanup_interval);
    }
}
