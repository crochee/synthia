use std::path::Path;

use crate::types::SkillMetadata;

/// Parses `SKILL.md` files.
///
/// A skill file has the form:
///
/// ```text
/// ---
/// name: <name>           # required
/// description: <text>    # recommended (Anthropic convention)
/// [metadata: { ... }]    # optional, free-form
/// ---
///
/// <body markdown>
/// ```
///
/// Aligned with the Anthropic Agent Skills open standard:
/// `name` is required and must equal the parent directory name;
/// `description` is recommended but optional (missing or empty
/// descriptions fall back to the first non-empty line of the
/// body, matching the OpenCode convention).
pub struct SkillLoader;

impl SkillLoader {
    /// Read the frontmatter from `<path>` and return its parsed
    /// [`SkillMetadata`].
    ///
    /// `name` is required; `description` and `metadata` are
    /// optional. A missing `description` deserialises as `None`
    /// and does NOT block loading — the loader only enforces
    /// `name != ""` and `name == parent_dir_name`. Both checks
    /// are the Anthropic / OpenCode / Grok Build contract for
    /// skill identity.
    pub fn parse_frontmatter(path: &Path) -> Result<SkillMetadata, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("read {}: {e}", path.display()))?;
        let parts: Vec<&str> = content.splitn(3, "---").collect();
        if parts.len() < 3 {
            return Err(format!(
                "invalid SKILL.md format at {} (missing '---' delimiters)",
                path.display()
            ));
        }
        let frontmatter = parts[1].trim();
        let mut metadata: SkillMetadata = serde_yaml::from_str(frontmatter)
            .map_err(|e| format!("parse frontmatter: {e}"))?;
        if metadata.name.is_empty() {
            return Err("missing required field: name".to_string());
        }
        // Anthropic Agent Skills treats `description` as
        // recommended, not required. Normalise missing / blank
        // descriptions to `None` so callers don't need to
        // distinguish the two.
        if let Some(desc) = metadata.description.as_ref()
            && desc.trim().is_empty()
        {
            metadata.description = None;
        }
        let dir_name = path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .ok_or_else(|| {
                format!(
                    "invalid SKILL.md format at {} (parent dir has no name)",
                    path.display()
                )
            })?;
        if metadata.name != dir_name {
            return Err(format!(
                "skill name mismatch: expected {dir_name}, found {}",
                metadata.name
            ));
        }
        Ok(metadata)
    }

    /// Read the markdown body (everything after the frontmatter
    /// closer) from `<path>`.
    pub fn parse_body(path: &Path) -> Result<String, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("read {}: {e}", path.display()))?;
        Self::extract_body_from_content(&content)
    }

    /// Same as [`Self::parse_body`] but takes the file content
    /// directly — useful for tests.
    pub fn extract_body_from_content(content: &str) -> Result<String, String> {
        let parts: Vec<&str> = content.splitn(3, "---").collect();
        if parts.len() < 3 {
            return Err(
                "invalid SKILL.md format: missing '---' delimiters".to_string()
            );
        }
        Ok(parts[2].trim().to_string())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn write(
        dir: &std::path::Path,
        name: &str,
        body: &str,
    ) -> std::path::PathBuf {
        let d = dir.join(name);
        fs::create_dir(&d).unwrap();
        fs::write(d.join("SKILL.md"), body).unwrap();
        d
    }

    #[test]
    fn parses_a_well_formed_skill_md() {
        let dir = tempfile::tempdir().unwrap();
        let skill = write(
            dir.path(),
            "transcribe",
            "---\nname: transcribe\ndescription: Transcribe audio\n---\n\nBody text.\n",
        );
        let meta =
            SkillLoader::parse_frontmatter(&skill.join("SKILL.md")).unwrap();
        assert_eq!(meta.name, "transcribe");
        assert_eq!(meta.description.as_deref(), Some("Transcribe audio"));
        let body = SkillLoader::parse_body(&skill.join("SKILL.md")).unwrap();
        assert_eq!(body, "Body text.");
    }

    /// Anthropic Agent Skills makes `description` optional —
    /// the loader MUST accept a SKILL.md with only `name`
    /// set, surfacing `description: None` so the runtime can
    /// fall back to the first non-empty line of the body.
    #[test]
    fn parses_skill_md_with_missing_description() {
        let dir = tempfile::tempdir().unwrap();
        let skill = write(
            dir.path(),
            "transcribe",
            "---\nname: transcribe\n---\n\n# Transcribe\n\nBody text.\n",
        );
        let meta =
            SkillLoader::parse_frontmatter(&skill.join("SKILL.md")).unwrap();
        assert_eq!(meta.name, "transcribe");
        assert!(
            meta.description.is_none(),
            "missing description must surface as None; got {:?}",
            meta.description
        );
    }

    /// `description: ""` (explicit blank) MUST be normalised
    /// to `None` by the loader, matching the "missing" case.
    /// Anthropic / OpenCode treat the two equivalently.
    #[test]
    fn parses_skill_md_with_blank_description_as_none() {
        let dir = tempfile::tempdir().unwrap();
        let skill = write(
            dir.path(),
            "transcribe",
            "---\nname: transcribe\ndescription: \"\"\n---\n\nBody text.\n",
        );
        let meta =
            SkillLoader::parse_frontmatter(&skill.join("SKILL.md")).unwrap();
        assert!(
            meta.description.is_none(),
            "blank description must normalise to None; got {:?}",
            meta.description
        );
    }

    #[test]
    fn rejects_missing_frontmatter() {
        let dir = tempfile::tempdir().unwrap();
        let skill = write(dir.path(), "nofront", "no delimiters here\n");
        let err = SkillLoader::parse_frontmatter(&skill.join("SKILL.md"))
            .unwrap_err();
        assert!(err.contains("invalid SKILL.md format"));
    }

    #[test]
    fn rejects_name_dir_mismatch() {
        let dir = tempfile::tempdir().unwrap();
        let skill = write(
            dir.path(),
            "transcribe",
            "---\nname: other\ndescription: x\n---\n",
        );
        let err = SkillLoader::parse_frontmatter(&skill.join("SKILL.md"))
            .unwrap_err();
        assert!(err.contains("skill name mismatch"));
    }

    #[test]
    fn rejects_missing_name() {
        let dir = tempfile::tempdir().unwrap();
        let skill = write(dir.path(), "x", "---\nname: \"\"\n---\n");
        let err = SkillLoader::parse_frontmatter(&skill.join("SKILL.md"))
            .unwrap_err();
        assert!(err.contains("name"));
    }

    #[test]
    fn extracts_body_from_raw_content() {
        let body = SkillLoader::extract_body_from_content(
            "---\nname: a\ndescription: b\n---\n\nhello world\n",
        )
        .unwrap();
        assert_eq!(body, "hello world");
    }
}
