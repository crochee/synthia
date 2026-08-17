//! Canonical skill value type + XML envelope.
//!
//! Aligned with the opencode `skill/index.ts::Info` shape and
//! the Anthropic Agent Skills progressive-disclosure contract:
//! `name` + `description` (metadata, always loaded into the
//! system prompt) + `location` + `content` (full body, loaded
//! only when the agent invokes the skill).
//!
//! The two surfaces the runtime cares about:
//!
//! - `Skill` — the value type itself.
//! - `format_skill_content` — wrap an invoked skill in the
//!   `<skill_content>` envelope the LLM reads. The envelope
//!   follows opencode's contract verbatim (`<skill_content
//!   name=…>body + base directory</skill_content>`) so models
//!   trained on either reference implementation recognise the
//!   shape and the agent's prompt text reads as one consistent
//!   vocabulary.
//!
//! `description` is optional — the Anthropic Agent Skills open
//! standard makes only `name` mandatory. When `description` is
//! absent, the loader keeps the frontmatter-validated field
//! as `None` and the caller falls back to the first non-empty
//! line of the body, matching the OpenCode convention.

use std::path::Path;

use crate::loader::SkillLoader;

/// One resolved skill — the canonical value type the runtime
/// passes between discovery, prompt assembly, and tool
/// dispatch.
///
/// Mirrors `opencode/packages/opencode/src/skill/index.ts::Info`:
///
/// ```text
/// name:        identifier (matches `name:` in SKILL.md frontmatter)
/// description: optional one-liner surfaced in <available_skills>
/// location:    absolute path to the SKILL.md file on disk
/// content:     full markdown body (everything after the
///              frontmatter closer), or the full file when no
///              frontmatter is present
/// ```
#[derive(Clone, Debug)]
pub struct Skill {
    pub name: String,
    pub description: Option<String>,
    pub location: std::path::PathBuf,
    pub content: String,
}

impl Skill {
    /// Build a `Skill` from an absolute `SKILL.md` path, parsing
    /// frontmatter + body in one pass.
    ///
    /// Returns the loader's error verbatim on malformed
    /// frontmatter (missing `---` delimiters, empty `name`,
    /// `name != parent_dir_name`); an absent `description` is
    /// surfaced as `Skill.description = None`, not a failure.
    pub fn from_path(path: &Path) -> Result<Self, String> {
        let meta = SkillLoader::parse_frontmatter(path)?;
        let body = SkillLoader::parse_body(path)?;
        Ok(Self {
            name: meta.name,
            description: meta.description,
            location: path.to_path_buf(),
            content: body,
        })
    }

    /// Resolve a `description` for the `<available_skills>`
    /// block: prefer the frontmatter field, fall back to the
    /// first non-empty line of the body. Matches the
    /// Anthropic / OpenCode convention where missing
    /// `description:` is silently derived from the body.
    pub fn effective_description(&self) -> String {
        if let Some(desc) = self.description.as_deref() {
            let trimmed = desc.trim();
            if !trimmed.is_empty() {
                return trimmed.to_string();
            }
        }
        // First non-empty line of the body — Anthropic Agent
        // Skills treats this as the implicit description.
        self.content
            .lines()
            .map(str::trim)
            .find(|l| !l.is_empty())
            .map(|l| l.trim_start_matches('#').trim().to_string())
            .unwrap_or_else(|| "(no description)".to_string())
    }
}

/// Wrap an invoked skill's body in the opencode / Anthropic
/// `<skill_content>` envelope.
///
/// Format:
///
/// ```text
/// <skill_content name="{name}">
/// {body}
///
/// Base directory for this skill: {base}
/// Relative paths in this skill (e.g., scripts/, reference/) are
/// relative to this base directory.
/// </skill_content>
/// ```
///
/// `base` is the parent directory of the SKILL.md file. The
/// hint lets the model resolve relative paths in the body
/// (e.g. `scripts/foo.py`) without round-tripping back to
/// shell.
///
/// This is the canonical formatter for any code path that
/// surfaces a skill body to the model: the `skill` tool,
/// any future slash-command expansion, and preloading paths
/// all route through this function so the presentation stays
/// identical.
pub fn format_skill_content(skill: &Skill) -> String {
    let base = skill
        .location
        .parent()
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    format!(
        "<skill_content name=\"{name}\">\n{body}\n\
         \n\
         Base directory for this skill: {base}\n\
         Relative paths in this skill (e.g., scripts/, reference/) are \
         relative to this base directory.\n\
         </skill_content>",
        name = skill.name,
        body = skill.content.trim(),
        base = base,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_skill(
        dir: &std::path::Path,
        name: &str,
        body: &str,
    ) -> std::path::PathBuf {
        let skill_dir = dir.join(name);
        std::fs::create_dir_all(&skill_dir).unwrap();
        let skill_md = skill_dir.join("SKILL.md");
        std::fs::write(&skill_md, body).unwrap();
        skill_md
    }

    /// `Skill::from_path` MUST accept a SKILL.md whose
    /// `description` is absent (Anthropic Agent Skills
    /// convention) and surface it as `description: None`.
    #[test]
    fn skill_from_path_accepts_missing_description() {
        let dir = tempfile::tempdir().unwrap();
        let md = write_skill(
            dir.path(),
            "transcribe",
            "---\nname: transcribe\n---\n\n# Transcribe\n\nBody.\n",
        );
        let skill = Skill::from_path(&md).unwrap();
        assert_eq!(skill.name, "transcribe");
        assert!(skill.description.is_none());
        assert!(skill.content.contains("# Transcribe"));
    }

    /// `Skill::effective_description` falls back to the first
    /// non-empty line of the body when `description` is missing
    /// (Anthropic / OpenCode convention).
    #[test]
    fn effective_description_falls_back_to_first_body_line() {
        let dir = tempfile::tempdir().unwrap();
        let md = write_skill(
            dir.path(),
            "transcribe",
            "---\nname: transcribe\n---\n\n# Transcribe audio\n\nBody.\n",
        );
        let skill = Skill::from_path(&md).unwrap();
        assert_eq!(skill.effective_description(), "Transcribe audio");
    }

    /// When the frontmatter description is present, it
    /// wins — body derivation is not consulted.
    #[test]
    fn effective_description_prefers_frontmatter() {
        let dir = tempfile::tempdir().unwrap();
        let md = write_skill(
            dir.path(),
            "x",
            "---\nname: x\ndescription: Frontmatter wins.\n---\n\n# Body line\n",
        );
        let skill = Skill::from_path(&md).unwrap();
        assert_eq!(skill.effective_description(), "Frontmatter wins.");
    }

    /// `format_skill_content` matches the opencode
    /// `<skill_content>` envelope byte-for-byte: balanced
    /// open/close tags, `name` attribute, body verbatim, then
    /// a base-directory hint.
    #[test]
    fn format_skill_content_matches_opencode_envelope() {
        let dir = tempfile::tempdir().unwrap();
        let md = write_skill(
            dir.path(),
            "transcribe",
            "---\nname: transcribe\ndescription: Transcribe audio.\n---\n\nBody line.\n",
        );
        let skill = Skill::from_path(&md).unwrap();
        let formatted = format_skill_content(&skill);
        assert!(
            formatted.starts_with("<skill_content name=\"transcribe\">"),
            "envelope must match opencode shape; got:\n{formatted}"
        );
        assert!(
            formatted.ends_with("</skill_content>"),
            "envelope must close with </skill_content>; got:\n{formatted}"
        );
        assert!(formatted.contains("Body line."));
        assert!(
            formatted.contains("Base directory for this skill:"),
            "envelope must include the base-directory hint; got:\n{formatted}"
        );
    }
}
