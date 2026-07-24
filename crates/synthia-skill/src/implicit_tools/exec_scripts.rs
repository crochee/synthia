// ── exec field injection ──────────────────────────────────────────────────

/// Parse `exec` field from SKILL.md frontmatter and inject as "Available Scripts" section.
pub fn inject_exec_scripts(body: &str, exec_paths: &[String]) -> String {
    if exec_paths.is_empty() {
        return body.to_string();
    }

    let scripts_section = exec_paths
        .iter()
        .map(|p| format!("- `{}`", p))
        .collect::<Vec<_>>()
        .join("\n");

    format!("{}\n\n### Available Scripts\n\n{}\n", body, scripts_section)
}
