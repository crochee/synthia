//! Markdown frontmatter skill loader.
//!
//! Loads skill definitions from `.md` files with YAML frontmatter:
//!
//! ```markdown
//! ---
//! name: my-skill
//! description: My custom skill
//! tools:
//!   - read_file
//!   - bash
//! ---
//!
//! # Instructions
//!
//! Your skill instructions go here.
//! ```

use std::path::Path;

use async_trait::async_trait;

use super::{
    skill_registry::{Skill, SkillProvenance},
    tool_name::ToolName,
};

/// Error type for skill loading.
#[derive(Debug, thiserror::Error)]
pub enum SkillLoadError {
    #[error("IO error reading skill file: {0}")]
    Io(#[from] std::io::Error),
    #[error("Missing frontmatter delimiter '---' in: {0}")]
    MissingFrontmatter(String),
    #[error("Invalid YAML frontmatter in {path}: {reason}")]
    InvalidYaml { path: String, reason: String },
    #[error("Missing required field '{field}' in frontmatter of: {path}")]
    MissingField { path: String, field: String },
}

/// Parsed frontmatter for a skill definition.
#[derive(Debug, Clone, serde::Deserialize)]
struct SkillFrontmatter {
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    tools: Vec<String>,
}

/// A skill loaded from a Markdown file with YAML frontmatter.
#[derive(Debug)]
pub struct FileSkill {
    name: String,
    description: String,
    instructions: String,
    tools: Vec<ToolName>,
    path: String,
}

impl FileSkill {
    /// Load a skill from a Markdown file.
    ///
    /// The file must start with `---` delimiters containing YAML frontmatter
    /// with at least a `name` field. The body after the frontmatter becomes
    /// the skill instructions.
    pub fn load(path: &Path) -> Result<Self, SkillLoadError> {
        let content = std::fs::read_to_string(path)?;
        Self::parse(&content, &path.to_string_lossy())
    }

    /// Parse a skill from a string with the given path (for error messages).
    fn parse(content: &str, path: &str) -> Result<Self, SkillLoadError> {
        let (frontmatter_str, body) =
            split_frontmatter(content).ok_or_else(|| {
                SkillLoadError::MissingFrontmatter(path.to_string())
            })?;

        let fm: SkillFrontmatter = serde_yaml::from_str(frontmatter_str)
            .map_err(|e| SkillLoadError::InvalidYaml {
                path: path.to_string(),
                reason: e.to_string(),
            })?;

        if fm.name.is_empty() {
            return Err(SkillLoadError::MissingField {
                path: path.to_string(),
                field: "name".to_string(),
            });
        }

        let tools: Vec<ToolName> =
            fm.tools.iter().map(ToolName::plain).collect();

        Ok(Self {
            name: fm.name,
            description: fm.description,
            instructions: body.trim().to_string(),
            tools,
            path: path.to_string(),
        })
    }

    /// The file path this skill was loaded from.
    pub fn path(&self) -> &str {
        &self.path
    }
}

#[async_trait]
impl Skill for FileSkill {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn instructions(&self) -> &str {
        &self.instructions
    }

    fn tools(&self) -> Vec<ToolName> {
        self.tools.clone()
    }

    fn provenance(&self) -> &SkillProvenance {
        // We return a static reference via a leaked Box.
        // This is acceptable because SkillProvenance is only held
        // for the lifetime of the skill, which is typically 'static.
        static PROVENANCE: std::sync::OnceLock<SkillProvenance> =
            std::sync::OnceLock::new();
        PROVENANCE.get_or_init(|| SkillProvenance::File {
            path: String::new(),
        })
    }

    async fn detect_invocation(&self, _user_input: &str) -> f64 {
        // File skills don't auto-detect; they must be explicitly activated.
        0.0
    }
}

/// Split a Markdown file into frontmatter and body.
///
/// Returns `Some((frontmatter, body))` if the content starts with `---`,
/// or `None` if no frontmatter is found.
fn split_frontmatter(content: &str) -> Option<(&str, &str)> {
    let content = content.trim_start();
    if !content.starts_with("---") {
        return None;
    }
    // Skip the opening ---
    let after_open = &content[3..];
    // Find the closing ---
    let close_offset = after_open.find("---")?;
    let frontmatter = &after_open[..close_offset];
    let body = &after_open[close_offset + 3..];
    Some((frontmatter.trim(), body.trim_start_matches('\n')))
}

/// Load all skill files from a directory.
///
/// Scans for `.md` files and attempts to load each as a `FileSkill`.
/// Returns successfully loaded skills and errors for files that failed.
pub fn load_skills_from_dir(
    dir: &Path,
) -> (Vec<FileSkill>, Vec<(String, SkillLoadError)>) {
    let mut skills = Vec::new();
    let mut errors = Vec::new();

    if !dir.exists() {
        return (skills, errors);
    }

    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return (skills, errors),
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "md") {
            match FileSkill::load(&path) {
                Ok(skill) => skills.push(skill),
                Err(e) => errors.push((path.to_string_lossy().to_string(), e)),
            }
        }
    }

    (skills, errors)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_frontmatter_basic() {
        let content = "---\nname: test\n---\nBody text";
        let (fm, body) = split_frontmatter(content).unwrap();
        assert_eq!(fm, "name: test");
        assert_eq!(body, "Body text");
    }

    #[test]
    fn split_frontmatter_with_description() {
        let content = "---\nname: my-skill\ndescription: A skill\n---\n# Instructions\nDo stuff.";
        let (fm, body) = split_frontmatter(content).unwrap();
        assert!(fm.contains("name: my-skill"));
        assert!(fm.contains("description: A skill"));
        assert!(body.contains("# Instructions"));
    }

    #[test]
    fn split_frontmatter_missing() {
        let content = "No frontmatter here";
        assert!(split_frontmatter(content).is_none());
    }

    #[test]
    fn parse_valid_skill() {
        let content = "---\nname: test-skill\ndescription: Test\ntools:\n  - read_file\n  - bash\n---\n# Instructions\nDo the thing.";
        let skill = FileSkill::parse(content, "test.md").unwrap();
        assert_eq!(skill.name(), "test-skill");
        assert_eq!(skill.description(), "Test");
        assert!(skill.instructions().contains("# Instructions"));
        assert_eq!(skill.tools().len(), 2);
    }

    #[test]
    fn parse_missing_name() {
        let content = "---\ndescription: No name\n---\nBody";
        let err = FileSkill::parse(content, "test.md").unwrap_err();
        match err {
            SkillLoadError::InvalidYaml { .. } => {
                // serde_yaml requires the `name` field, so it returns a
                // YAML parse error (missing field), which we surface as
                // InvalidYaml.
            }
            other => panic!("Expected InvalidYaml, got: {other}"),
        }
    }

    #[test]
    fn parse_empty_name() {
        let content = "---\nname: \"\"\n---\nBody";
        let err = FileSkill::parse(content, "test.md").unwrap_err();
        match err {
            SkillLoadError::MissingField { field, .. } => {
                assert_eq!(field, "name");
            }
            other => panic!("Expected MissingField, got: {other}"),
        }
    }

    #[test]
    fn parse_invalid_yaml() {
        let content = "---\nname: [\n---\nBody";
        let err = FileSkill::parse(content, "test.md").unwrap_err();
        match err {
            SkillLoadError::InvalidYaml { .. } => {}
            other => panic!("Expected InvalidYaml, got: {other}"),
        }
    }

    #[test]
    fn parse_no_frontmatter() {
        let content = "Just some text";
        let err = FileSkill::parse(content, "test.md").unwrap_err();
        match err {
            SkillLoadError::MissingFrontmatter(_) => {}
            other => panic!("Expected MissingFrontmatter, got: {other}"),
        }
    }

    #[test]
    fn parse_empty_tools() {
        let content = "---\nname: no-tools\n---\nSimple skill";
        let skill = FileSkill::parse(content, "test.md").unwrap();
        assert_eq!(skill.name(), "no-tools");
        assert!(skill.tools().is_empty());
    }

    #[test]
    fn load_skills_from_nonexistent_dir() {
        let (skills, errors) =
            load_skills_from_dir(Path::new("/nonexistent/skills"));
        assert!(skills.is_empty());
        assert!(errors.is_empty());
    }

    #[tokio::test]
    async fn file_skill_detect_invocation_returns_zero() {
        let content = "---\nname: test\n---\nBody";
        let skill = FileSkill::parse(content, "test.md").unwrap();
        assert_eq!(skill.detect_invocation("write code").await, 0.0);
    }
}
