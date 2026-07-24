//! The output-shaping pipeline:
//!
//! - [`merge_within_limit`]: truncate each file to `max_chars_per_file`
//!   and stop appending once the cumulative total would exceed
//!   `max_chars_total`.
//! - [`truncate_with_marker`]: per-file helper, keeps the first
//!   `max_chars` chars of `s` plus a marker.
//! - [`format_merged`]: wrap the final list in `<agents_md>` tags.

use super::config::DiscoveredFile;

/// Truncate each file's content to `max_chars_per_file` (with a
/// marker) and stop appending once the cumulative total would exceed
/// `max_chars_total`. Because the input is farthest-to-closest, the
/// closest file is processed last and is therefore naturally favored
/// when the budget is tight.
pub(super) fn merge_within_limit(
    mut files: Vec<DiscoveredFile>,
    max_chars_per_file: usize,
    max_chars_total: usize,
) -> Vec<DiscoveredFile> {
    let mut kept: Vec<DiscoveredFile> = Vec::new();
    let mut used = 0usize;
    let mut total_truncated = false;

    for file in files.drain(..) {
        let DiscoveredFile { path, content } = file;
        let truncated = truncate_with_marker(&content, max_chars_per_file);
        let new_chars = truncated.chars().count();

        if used + new_chars > max_chars_total {
            total_truncated = true;
            break;
        }

        used += new_chars;
        kept.push(DiscoveredFile {
            path,
            content: truncated,
        });
    }

    if total_truncated {
        // Append a final marker to the last kept file. If nothing was
        // kept, return an empty Vec (caller handles empty case).
        if let Some(last) = kept.last_mut() {
            last.content.push_str(&format!(
                "\n\n[... total content exceeded {max_chars_total} chars; further AGENTS.md files omitted ...]\n"
            ));
        }
    }

    kept
}

/// If `s` is longer than `max_chars`, return the first `max_chars`
/// characters plus a marker. Otherwise return `s` unchanged.
pub(super) fn truncate_with_marker(s: &str, max_chars: usize) -> String {
    let count = s.chars().count();
    if count <= max_chars {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max_chars).collect();
    out.push_str(&format!(
        "\n\n[... truncated at {max_chars} chars - use read for full file ...]\n"
    ));
    out
}

/// Format the merged files into a single section string. The output is
/// wrapped in `<agents_md>` tags to mirror `EnvironmentSection`'s
/// `<env>` style.
pub(super) fn format_merged(
    files: &[DiscoveredFile],
    _max_chars_total: usize,
) -> String {
    let mut out = String::from("# Project Agent Instructions\n\n<agents_md>\n");
    let blocks: Vec<String> = files
        .iter()
        .map(|f| {
            format!(
                "## AGENTS.md: {}\n\n{}",
                f.path.display(),
                f.content.trim_end()
            )
        })
        .collect();
    out.push_str(&blocks.join("\n\n---\n\n"));
    out.push_str("\n</agents_md>");
    out
}
