use std::path::Path;

use crate::task::types::TaskContext;

/// Resolve file references by reading their contents from disk.
///
/// Each resolved file is returned as a formatted string:
/// ```text
/// === path/to/file.rs ===
/// <file contents>
/// ```
///
/// Non-existent or unreadable files are silently skipped with a warning.
pub async fn resolve_file_references(
    file_paths: &[String],
    workspace_root: &Path,
) -> String {
    let mut resolved = String::new();

    for path in file_paths {
        let full_path = if std::path::Path::new(path).is_absolute() {
            std::path::PathBuf::from(path)
        } else {
            workspace_root.join(path)
        };

        match tokio::fs::read_to_string(&full_path).await {
            Ok(content) => {
                resolved.push_str(&format!(
                    "=== {} ===\n{}\n\n",
                    full_path.display(),
                    content
                ));
            }
            Err(e) => {
                tracing::warn!(
                    path = %full_path.display(),
                    error = %e,
                    "Failed to resolve file reference for task context"
                );
            }
        }
    }

    resolved
}

/// Format all task context into a single prompt section for the sub-agent.
pub fn format_task_context(
    context: &TaskContext,
    resolved_files: &str,
) -> String {
    let mut sections = Vec::new();

    sections.push(format!("## Task Description\n{}\n", context.description));

    if !resolved_files.is_empty() {
        sections.push(format!("## Referenced Files\n{}\n", resolved_files));
    }

    if !context.code_snippets.is_empty() {
        let snippets = context
            .code_snippets
            .iter()
            .map(|s| format!("### {}\n```{}\n```\n", s.name, s.content))
            .collect::<Vec<_>>()
            .join("\n");
        sections.push(format!("## Code Snippets\n{}\n", snippets));
    }

    if !context.constraints.is_empty() {
        let constraints = context
            .constraints
            .iter()
            .map(|c| format!("- {}", c))
            .collect::<Vec<_>>()
            .join("\n");
        sections.push(format!("## Constraints\n{}\n", constraints));
    }

    sections.join("\n")
}
