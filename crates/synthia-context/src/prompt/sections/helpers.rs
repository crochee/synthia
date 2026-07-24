//! Prompt-section text helpers.
//!
//! Four small utilities shared by the concrete section
//! implementations (and a few one-shot tests):
//!
//! - [`prepend_bullets`] — turn a list of strings into a
//!   bulleted block with two-space indent ("  - item"). Used by
//!   `system` / `tools_usage` / `task_execution` for the
//!   itemized lists inside their rendered sections.
//! - [`join_lines`] — newline-join a list of strings (no
//!   trailing newline). Used by tests to assert exact section
//!   text.
//! - [`inject_workspace_file`] / [`inject_workspace_files`] —
//!   read a workspace-relative file (e.g. `IDENTITY.md`,
//!   `USER.md`, `MEMORY.md`) and inject it under a
//!   `### {filename}` heading, truncated at
//!   [`WORKSPACE_FILE_MAX_CHARS`] chars. Used by
//!   `identity::IdentitySection::build` to fold workspace
//!   context into the identity section.
//!
//! Kept separate from [`super::trait`] (the `PromptSection`
//! contract) so the trait definition isn't padded with
//! formatting helpers, and from the per-section modules so the
//! same helper isn't copy-pasted into each one.

use std::{fmt::Write, path::Path};

const WORKSPACE_FILE_MAX_CHARS: usize = 20_000;

pub fn prepend_bullets(items: &[&str]) -> String {
    items
        .iter()
        .map(|s| format!("  - {s}"))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn join_lines(items: &[&str]) -> String {
    items.join("\n")
}

pub fn inject_workspace_file(
    prompt: &mut String,
    workspace_dir: &Path,
    filename: &str,
) {
    let path = workspace_dir.join(filename);
    if let Ok(content) = std::fs::read_to_string(&path) {
        let trimmed = content.trim();
        if trimmed.is_empty() {
            return;
        }
        let _ = writeln!(prompt, "### {filename}\n");
        let truncated = if trimmed.chars().count() > WORKSPACE_FILE_MAX_CHARS {
            trimmed.chars().take(WORKSPACE_FILE_MAX_CHARS).collect()
        } else {
            trimmed.to_string()
        };
        prompt.push_str(&truncated);
        if truncated.len() < trimmed.len() {
            let _ = writeln!(
                prompt,
                "\n\n[... truncated at {WORKSPACE_FILE_MAX_CHARS} chars - use `read` for full file]\n"
            );
        } else {
            prompt.push_str("\n\n");
        }
    }
}

pub fn inject_workspace_files(
    prompt: &mut String,
    workspace_dir: &Path,
    files: &[&str],
) {
    for file in files {
        inject_workspace_file(prompt, workspace_dir, file);
    }
}
