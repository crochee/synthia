use chrono::Utc;

use super::constants::*;

/// Returns the file name for a given hot memory key.
/// Known keys map to specific files; unknown keys get their own .md file.
pub(super) fn key_to_filename(key: &str) -> String {
    match key {
        MEMORY_MD_KEY => "MEMORY.md".to_string(),
        USER_MD_KEY => "USER.md".to_string(),
        _ => format!("{}.md", key),
    }
}

/// Format an entry as markdown with YAML frontmatter.
pub(super) fn format_entry(key: &str, content: &str) -> String {
    let timestamp = Utc::now().to_rfc3339();
    format!(
        "---\nkey: {}\ntimestamp: {}\n---\n\n{}\n",
        key, timestamp, content
    )
}

/// Parse content from a markdown file, stripping frontmatter.
pub(super) fn parse_entry(file_content: &str) -> Option<String> {
    let content = file_content.trim_start();
    if !content.starts_with("---") {
        return Some(content.to_string());
    }

    // Find the closing ---
    let rest = &content[3..];
    if let Some(end) = rest.find("\n---") {
        let body = &rest[end + 4..];
        Some(body.trim().to_string())
    } else {
        None
    }
}
