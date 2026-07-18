//! Async bound_output implementation — truncation + managed file spill.

use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use tokio::{fs, io::AsyncWriteExt, time};

use crate::tool::{
    output_bound::{OutputBound, OverflowStrategy, SanitizationPolicy},
    types::{ContentPart, ToolOutput},
};

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
        if let ContentPart::Text { text } = part {
            let total_bytes = text.len();
            let total_lines = text.lines().count();

            if total_bytes > config.per_call_max_bytes
                || total_lines > config.per_call_max_lines
            {
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
                            output.metadata.truncated = true;
                            output.metadata.managed_paths.push(path);
                        }
                    }
                    OverflowStrategy::TruncateHead => {
                        truncate_in_place(text, config.per_call_max_bytes);
                        output.metadata.truncated = true;
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
                        output.metadata.truncated = true;
                        output.metadata.managed_paths.push(managed_path);
                    }
                }
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
                if let ContentPart::Text { text } = part {
                    *text = strip_control_chars(text);
                }
            }
        }
        SanitizationPolicy::WrapUntrusted => {
            for part in &mut output.content {
                if let ContentPart::Text { text } = part {
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
        SanitizationPolicy::RedactUrlsMatching => {
            // URL redaction would need a regex pattern — deferred
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
        total_lines - head_lines - tail_lines,
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
        if let Ok(modified) = metadata.modified() {
            if modified < cutoff {
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
    }

    Ok(())
}
