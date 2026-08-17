//! `skill` tool — load a specialised skill by name and return
//! its instructions to the model.
//!
//! ## Shape (single-purpose)
//!
//! Mirrors the Anthropic Agent Skills + OpenCode / Grok Build
//! convention: a single `{name}` parameter, no `action`
//! discriminator, no auxiliary knobs. The skill name comes
//! from the `<available_skills>` block in the system prompt so
//! the LLM has a closed set of valid inputs before it calls
//! the tool. Adding a discriminator (the prototype `action:
//! "list" | "read"` shape) produced repeated "Missing required
//! argument `name`" failures in transcripts because the model
//! half-applied the enum and emitted `{"name": null}`.
//!
//! ## Response envelope
//!
//! Returns the opencode `<skill_content>` envelope:
//!
//! ```text
//! <skill_content name="...">
//! <body>
//!
//! Base directory for this skill: <path>
//! Relative paths in this skill (e.g., scripts/, reference/)
//! are relative to this base directory.
//! </skill_content>
//! ```
//!
//! The envelope gives the model a clear identity and boundary
//! for the loaded content (matches Anthropic Agent Skills
//! "instructions wrapped in structural delimiters"). The
//! base-directory hint lets the model resolve relative
//! references inside the body without an extra round-trip
//! to shell.
//!
//! See [`crate::skill::format_skill_content`] for the
//! single-source-of-truth formatter.

use std::path::Path;

use async_trait::async_trait;
use serde_json::json;
use synthia_tool::{Context, Tool, ToolEntry, ToolOutput, ToolRegistry};

use crate::skill::{Skill, format_skill_content};

/// Public name of the skill tool exposed to the LLM.
pub const SKILL_TOOL_NAME: &str = "skill";

/// `skill` — load a specialised workflow skill by name.
///
/// Description wording follows the Anthropic Agent Skills
/// reference: "Load a specialized skill when the task at
/// hand matches one of the skills listed in the system
/// prompt." This is the same first sentence the opencode and
/// Grok Build skill tools use; the model has seen it on
/// every reference implementation and treats it as the
/// canonical "use this tool only when prompted" hook.
#[derive(Debug, Default)]
pub struct SkillTool;

impl SkillTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for SkillTool {
    fn name(&self) -> &str {
        SKILL_TOOL_NAME
    }

    fn description(&self) -> &str {
        "Load a specialized skill when the task at hand matches \
         one of the skills listed in the system prompt. Use this \
         tool to inject the skill's instructions and resources into \
         the current conversation. The output may contain detailed \
         workflow guidance as well as references to scripts, files, \
         etc. in the same directory as the skill. The skill name \
         must match one of the skills listed in the system prompt's \
         <available_skills> block."
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "The name of the skill from <available_skills>."
                }
            },
            "required": ["name"]
        })
    }

    async fn call(
        &self,
        input: serde_json::Value,
        context: &Context,
    ) -> ToolOutput {
        let Some(name) = input.get("name").and_then(|v| v.as_str()) else {
            return ToolOutput::error(
                "Missing required argument `name` for the `skill` tool. \
                 Pick one from the <available_skills> block in the \
                 system prompt and try again.",
            );
        };
        if name.is_empty() {
            return ToolOutput::error(
                "Empty `name` for the `skill` tool; expected a \
                 non-empty skill directory name from \
                 <available_skills>.",
            );
        }
        // Path-traversal guard — refuse names that would
        // escape the skills tree.
        if name.contains("..") || name.contains('/') || name.contains('\\') {
            return ToolOutput::error(format!(
                "Invalid skill name '{name}': must be a directory \
                 name from the <available_skills> block (no '/' or \
                 '..')."
            ));
        }
        let workspace_root: &Path = &context.workspace_root;
        let skill_md = skill_md_path(workspace_root, name);
        match Skill::from_path(&skill_md) {
            Ok(skill) => ToolOutput::text(format_skill_content(&skill)),
            Err(err) => {
                let hint = match discover_available_names(workspace_root) {
                    Ok(names) if !names.is_empty() => {
                        format!(" Available skills: {}.", names.join(", "))
                    }
                    Ok(_) => String::new(),
                    Err(_) => String::new(),
                };
                ToolOutput::error(format!(
                    "Skill '{name}' could not be loaded: {err}.{hint}"
                ))
            }
        }
    }
}

/// Resolve `<workspace>/.agents/skills/<name>/SKILL.md`.
///
/// Single source of truth for the on-disk path so the tool
/// and the seed remain in lock-step. Lives next to the tool
/// (not in `seed`) because it is the tool's contract, not
/// the seed's.
fn skill_md_path(workspace_root: &Path, name: &str) -> std::path::PathBuf {
    workspace_root
        .join(".agents")
        .join("skills")
        .join(name)
        .join("SKILL.md")
}

/// Best-effort enumeration of available skill names for the
/// "Skill 'foo' not found" error hint. Walks the project
/// skills root + `$HOME/{.claude,.agents}/skills` so the
/// hint covers both project and user installs.
///
/// We deliberately do NOT reuse [`crate::discovery::discover_skills`]
/// here — that one walks every `SKILL.md` and parses each
/// one's frontmatter, which is overkill for an error message.
/// A shallow `read_dir` of `<name>/SKILL.md` is enough.
fn discover_available_names(
    workspace_root: &Path,
) -> Result<Vec<String>, String> {
    let mut names: Vec<String> = Vec::new();
    let mut seen: std::collections::HashSet<String> =
        std::collections::HashSet::new();
    for root in skill_roots(workspace_root) {
        let Ok(entries) = std::fs::read_dir(&root) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let Some(name) = path
                .file_name()
                .and_then(|n| n.to_str())
                .map(str::to_string)
            else {
                continue;
            };
            if seen.insert(name.clone()) && path.join("SKILL.md").is_file() {
                names.push(name);
            }
        }
    }
    names.sort();
    Ok(names)
}

fn skill_roots(workspace_root: &Path) -> Vec<std::path::PathBuf> {
    let mut roots: Vec<std::path::PathBuf> = Vec::new();
    roots.push(workspace_root.join(crate::discovery::PROJECT_SKILLS_DIR));
    if let Some(home) = std::env::var_os("HOME").map(std::path::PathBuf::from) {
        roots.push(home.join(".claude").join("skills"));
        roots.push(home.join(".agents").join("skills"));
    }
    roots
}

/// Register the `skill` tool into `registry`.
///
/// Returns `true` when the entry was inserted; `false` when a
/// Core tool already occupies the `skill` name (the registry's
/// immutability guard refuses to overwrite built-in entries
/// with the same name).
pub fn register_skill_tool(registry: &ToolRegistry) -> bool {
    let tool = std::sync::Arc::new(SkillTool::new());
    registry.register_entry(ToolEntry::new(tool))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use synthia_core::registry::Registry;
    use synthia_tool::{Context, Tool, ToolOutput};

    use super::{SkillTool, register_skill_tool};
    use crate::seed::seed_default_skills;

    fn make_context(root: PathBuf) -> Context {
        Context::new("s1".to_string(), root)
    }

    fn first_text(out: &ToolOutput) -> String {
        out.content
            .iter()
            .find_map(|c| c.text().map(str::to_string))
            .unwrap_or_default()
    }

    // ---- Tool metadata -------------------------------------------

    /// Anthropic Agent Skills: the tool name is `skill`. Pins
    /// the public constant so a future rename is loud.
    #[test]
    fn tool_name_is_skill() {
        assert_eq!(SkillTool::new().name(), "skill");
        assert_eq!(super::SKILL_TOOL_NAME, "skill");
    }

    /// Anthropic / opencode / Grok Build all gate the schema
    /// to a single required `name` parameter — no `action`
    /// discriminator. The previous prototype added one and
    /// produced repeated "Missing required argument `name`"
    /// failures in transcripts, so the pin is loud.
    #[test]
    fn parameters_pin_name_as_only_required_field() {
        let params = SkillTool::new().parameters();
        assert_eq!(params["type"], "object");
        let required: Vec<&str> = params["required"]
            .as_array()
            .expect("required")
            .iter()
            .filter_map(|v| v.as_str())
            .collect();
        assert_eq!(required, vec!["name"]);
        assert_eq!(
            params["properties"]["name"]["type"], "string",
            "name must be a string"
        );
    }

    /// Description wording matches the Anthropic Agent Skills
    /// reference ("Load a specialized skill…") and points the
    /// model at the `<available_skills>` block. Pinned so a
    /// future rewrite that drops the reference breaks loudly.
    #[test]
    fn description_mentions_industry_anchor() {
        let tool = SkillTool::new();
        let desc = tool.description();
        assert!(
            desc.to_lowercase().contains("load a specialized skill"),
            "description must use the Anthropic / OpenCode \
             wording; got: {desc}"
        );
        assert!(
            desc.contains("available_skills"),
            "description must point at the <available_skills> \
             block; got: {desc}"
        );
    }

    // ---- Behavior -------------------------------------------------

    /// Happy path: loading `code-review` returns the body
    /// wrapped in the opencode `<skill_content>` envelope
    /// with the base-directory hint.
    #[tokio::test]
    async fn read_returns_envelope_with_body_and_base_dir() {
        let dir = tempfile::tempdir().unwrap();
        seed_default_skills(dir.path()).unwrap();

        let tool = SkillTool::new();
        let out = tool
            .call(
                serde_json::json!({"name": "code-review"}),
                &make_context(dir.path().to_path_buf()),
            )
            .await;
        let text = first_text(&out);
        assert!(
            text.starts_with("<skill_content name=\"code-review\">"),
            "response must use the opencode envelope; got:\n{text}"
        );
        assert!(
            text.ends_with("</skill_content>"),
            "envelope must close with </skill_content>; got:\n{text}"
        );
        assert!(
            text.contains("Code review") && text.contains("Steps"),
            "envelope must carry the skill body; got:\n{text}"
        );
        assert!(
            text.contains("Base directory for this skill:"),
            "envelope must include the base-directory hint; got:\n{text}"
        );
    }

    #[tokio::test]
    async fn missing_name_argument_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let tool = SkillTool::new();
        let out = tool
            .call(
                serde_json::json!({}),
                &make_context(dir.path().to_path_buf()),
            )
            .await;
        assert!(out.is_error.unwrap_or(false));
        let text = first_text(&out);
        assert!(
            text.contains("name"),
            "error must mention name; got: {text}"
        );
        // The error message MUST point the model at the
        // recovery path (the <available_skills> block).
        assert!(
            text.contains("available_skills"),
            "error must direct the model to <available_skills>; \
             got: {text}"
        );
    }

    /// Replicates the exact failure mode from the original
    /// chat transcript: the LLM emits
    /// `{"name": null, "action": "read"}` which after dropping
    /// the `action` discriminator collapses to a missing-name
    /// failure. Pin the behaviour so a future schema
    /// regression is caught by the test rather than the user.
    #[tokio::test]
    async fn null_name_argument_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let tool = SkillTool::new();
        let out = tool
            .call(
                serde_json::json!({"name": null}),
                &make_context(dir.path().to_path_buf()),
            )
            .await;
        assert!(out.is_error.unwrap_or(false));
        let text = first_text(&out);
        assert!(
            text.contains("name"),
            "null name must surface as a missing-name error; got: {text}"
        );
    }

    #[tokio::test]
    async fn empty_name_argument_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let tool = SkillTool::new();
        let out = tool
            .call(
                serde_json::json!({"name": ""}),
                &make_context(dir.path().to_path_buf()),
            )
            .await;
        assert!(out.is_error.unwrap_or(false));
    }

    /// Missing-skill error lists the available skill names so
    /// the LLM can self-correct on the next call. Anthropic /
    /// OpenCode / Grok Build all do this.
    #[tokio::test]
    async fn unknown_skill_lists_available_names() {
        let dir = tempfile::tempdir().unwrap();
        seed_default_skills(dir.path()).unwrap();
        let tool = SkillTool::new();
        let out = tool
            .call(
                serde_json::json!({"name": "ghost"}),
                &make_context(dir.path().to_path_buf()),
            )
            .await;
        assert!(out.is_error.unwrap_or(false));
        let text = first_text(&out);
        assert!(
            text.contains("ghost"),
            "must name the missing skill; got: {text}"
        );
        for seeded in ["code-review", "bug-investigation", "test-planning"] {
            assert!(
                text.contains(seeded),
                "error must list seeded skill '{seeded}' so the LLM \
                 can recover; got: {text}"
            );
        }
    }

    /// Skills with missing `description` (Anthropic Agent
    /// Skills makes it optional) MUST load successfully —
    /// the loader falls back to the first non-empty body line.
    #[tokio::test]
    async fn skill_without_description_still_loads() {
        let dir = tempfile::tempdir().unwrap();
        let skill_dir = dir.path().join(".agents").join("skills").join("nodoc");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: nodoc\n---\n\n# No description skill\n\nBody.\n",
        )
        .unwrap();

        let tool = SkillTool::new();
        let out = tool
            .call(
                serde_json::json!({"name": "nodoc"}),
                &make_context(dir.path().to_path_buf()),
            )
            .await;
        assert!(
            out.is_error.is_none() || !out.is_error.unwrap_or(false),
            "skill without description must still load; got error: {out:?}"
        );
        let text = first_text(&out);
        assert!(
            text.contains("No description skill"),
            "envelope must carry the body fallback; got:\n{text}"
        );
    }

    #[tokio::test]
    async fn blocks_path_traversal() {
        let dir = tempfile::tempdir().unwrap();
        let tool = SkillTool::new();
        let out = tool
            .call(
                serde_json::json!({"name": "../etc/passwd"}),
                &make_context(dir.path().to_path_buf()),
            )
            .await;
        assert!(out.is_error.unwrap_or(false));
    }

    #[tokio::test]
    async fn blocks_forward_slash_in_name() {
        let dir = tempfile::tempdir().unwrap();
        let tool = SkillTool::new();
        let out = tool
            .call(
                serde_json::json!({"name": "foo/bar"}),
                &make_context(dir.path().to_path_buf()),
            )
            .await;
        assert!(out.is_error.unwrap_or(false));
    }

    // ---- Registry integration -------------------------------------

    #[tokio::test]
    async fn register_skill_tool_inserts_skill_into_registry() {
        let registry = synthia_tool::ToolRegistry::new();
        assert!(registry.get("skill").await.unwrap().is_none());
        assert!(register_skill_tool(&registry));
        assert!(registry.get("skill").await.unwrap().is_some());
    }
}
