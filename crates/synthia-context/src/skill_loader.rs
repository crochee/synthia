use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use crate::types::ContextError;

/// Summary-level skill info loaded at Level 0.
#[derive(Debug, Clone)]
pub struct SkillSummary {
    pub name: String,
    pub description: String,
}

/// Full skill content loaded at Level 1 on demand.
#[derive(Debug, Clone)]
pub struct SkillContent {
    pub name: String,
    pub description: String,
    pub content: String,
}

/// Progressive disclosure skill loader.
///
/// Level 0: Load Skill name + one-line summary (always loaded at session start)
/// Level 1: Load full Skill content on demand (when skill is invoked)
pub struct SkillLoader {
    /// Level 0 cache: always-loaded summaries
    summaries: Vec<SkillSummary>,
    /// Level 1 cache: lazily-loaded full content
    loaded_content: HashMap<String, SkillContent>,
    /// Root directory for skill files
    skills_dir: PathBuf,
}

impl SkillLoader {
    pub fn new(skills_dir: PathBuf) -> Self {
        Self {
            summaries: Vec::new(),
            loaded_content: HashMap::new(),
            skills_dir,
        }
    }

    /// Discover available skills by scanning the skills directory.
    /// Each skill is expected to be a markdown file: `<name>.md`
    pub fn discover_skills(&self) -> Result<Vec<SkillSummary>, ContextError> {
        if !self.skills_dir.exists() {
            return Ok(vec![]);
        }

        let mut skills = Vec::new();
        for entry in std::fs::read_dir(&self.skills_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("md") {
                // Level 0: Extract name and first-line summary only
                if let Some(summary) = Self::load_skill_summary(&path)? {
                    skills.push(summary);
                }
            }
        }

        skills.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(skills)
    }

    /// Load skill summaries and cache them (Level 0).
    pub fn load_summaries(&mut self) -> Result<&[SkillSummary], ContextError> {
        self.summaries = self.discover_skills()?;
        Ok(&self.summaries)
    }

    /// Get the cached Level 0 summaries.
    pub fn summaries(&self) -> &[SkillSummary] {
        &self.summaries
    }

    /// Load full skill content on demand (Level 1).
    /// Uses cache if already loaded.
    pub fn load_skill(
        &mut self,
        skill_name: &str,
    ) -> Result<&SkillContent, ContextError> {
        // Check cache first
        if self.loaded_content.contains_key(skill_name) {
            return Ok(self.loaded_content.get(skill_name).unwrap());
        }

        // Find the skill file
        let skill_path = self.find_skill_file(skill_name)?;
        let content = std::fs::read_to_string(&skill_path)?;

        // Parse the content: first line is description, rest is body
        let (description, body) = Self::parse_skill_content(&content);

        let skill = SkillContent {
            name: skill_name.to_string(),
            description: description.unwrap_or_default(),
            content: body,
        };

        self.loaded_content.insert(skill_name.to_string(), skill);
        Ok(self.loaded_content.get(skill_name).unwrap())
    }

    /// Inject Level 0 summaries into a system prompt fragment.
    /// Returns a formatted string listing all available skills.
    pub fn format_skill_summaries(&self) -> String {
        if self.summaries.is_empty() {
            return String::from(
                "# Available Skills\n\nNo skills are currently available.",
            );
        }

        let mut output = String::from("# Available Skills\n\n");
        for summary in &self.summaries {
            output.push_str(&format!(
                "- **{}**: {}\n",
                summary.name, summary.description
            ));
        }
        output
    }

    /// Format loaded skill content for injection into context.
    pub fn format_skill_content(
        &self,
        skill_name: &str,
    ) -> Result<String, ContextError> {
        let skill = self.loaded_content.get(skill_name).ok_or_else(|| {
            ContextError::Checkpoint(format!(
                "Skill '{}' not loaded",
                skill_name
            ))
        })?;

        Ok(format!(
            "# Skill: {}\n{}\n{}",
            skill.name, skill.description, skill.content
        ))
    }

    // --- Private helpers ---

    fn load_skill_summary(
        path: &Path,
    ) -> Result<Option<SkillSummary>, ContextError> {
        let content = std::fs::read_to_string(path)?;
        let file_stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();

        // Extract the first non-empty line as description
        let description = content
            .lines()
            .find(|l| !l.trim().is_empty() && !l.trim().starts_with('#'))
            .unwrap_or("")
            .trim()
            .to_string();

        if description.is_empty() {
            return Ok(None);
        }

        Ok(Some(SkillSummary {
            name: file_stem,
            description,
        }))
    }

    fn find_skill_file(
        &self,
        skill_name: &str,
    ) -> Result<PathBuf, ContextError> {
        if !self.skills_dir.exists() {
            return Err(ContextError::Checkpoint(format!(
                "Skills directory not found: {}",
                self.skills_dir.display()
            )));
        }

        // Try exact match with .md extension
        let exact_path = self.skills_dir.join(format!("{}.md", skill_name));
        if exact_path.exists() {
            return Ok(exact_path);
        }

        // Try case-insensitive search
        for entry in std::fs::read_dir(&self.skills_dir)? {
            let entry = entry?;
            let path = entry.path();
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str())
                && stem.eq_ignore_ascii_case(skill_name)
                && path.extension().and_then(|s| s.to_str()) == Some("md")
            {
                return Ok(path);
            }
        }

        Err(ContextError::Checkpoint(format!(
            "Skill file not found: {}",
            skill_name
        )))
    }

    fn parse_skill_content(content: &str) -> (Option<String>, String) {
        let all_lines: Vec<&str> = content.lines().collect();
        let mut description = None;
        let mut body_start = 0;
        for (i, line) in all_lines.iter().enumerate() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            description = Some(trimmed.to_string());
            // Body includes everything after the description line
            body_start = i + 1;
            break;
        }

        let body = all_lines[body_start..].join("\n");
        // If body is empty, include the description line as part of content too
        if body.is_empty() && description.is_some() {
            // Content field includes full file when only a single line exists
            return (description.clone(), content.to_string());
        }
        (description, body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_temp_skill_dir() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();

        // Create skill files
        std::fs::write(
            dir.path().join("code_review.md"),
            "# Code Review Skill\n\nPerforms automated code review on PRs.",
        )
        .unwrap();

        std::fs::write(
            dir.path().join("test_runner.md"),
            "# Test Runner\n\nRuns tests and reports results.",
        )
        .unwrap();

        dir
    }

    #[test]
    fn test_skill_loader_discover() {
        let dir = create_temp_skill_dir();
        let loader = SkillLoader::new(dir.path().to_path_buf());

        let skills = loader.discover_skills().unwrap();
        assert_eq!(skills.len(), 2);
        assert_eq!(skills[0].name, "code_review");
        assert_eq!(skills[1].name, "test_runner");
    }

    #[test]
    fn test_skill_loader_load_summaries() {
        let dir = create_temp_skill_dir();
        let mut loader = SkillLoader::new(dir.path().to_path_buf());

        let summaries = loader.load_summaries().unwrap();
        assert_eq!(summaries.len(), 2);
        assert_eq!(loader.summaries().len(), 2);
    }

    #[test]
    fn test_skill_loader_load_full_content() {
        let dir = create_temp_skill_dir();
        let mut loader = SkillLoader::new(dir.path().to_path_buf());

        let content = loader.load_skill("code_review").unwrap();
        assert_eq!(content.name, "code_review");
        // The description is extracted separately from the body
        assert!(
            content.description.contains("automated code review")
                || content.content.contains("automated code review")
        );
    }

    #[test]
    fn test_skill_loader_caches_content() {
        let dir = create_temp_skill_dir();
        let mut loader = SkillLoader::new(dir.path().to_path_buf());

        // Load twice
        let content1 =
            loader.load_skill("test_runner").unwrap().content.clone();
        let content2 =
            loader.load_skill("test_runner").unwrap().content.clone();
        assert_eq!(content1, content2);
    }

    #[test]
    fn test_skill_loader_format_summaries() {
        let dir = create_temp_skill_dir();
        let mut loader = SkillLoader::new(dir.path().to_path_buf());
        loader.load_summaries().unwrap();

        let formatted = loader.format_skill_summaries();
        assert!(formatted.contains("Available Skills"));
        assert!(formatted.contains("code_review"));
        assert!(formatted.contains("test_runner"));
    }

    #[test]
    fn test_skill_loader_format_content() {
        let dir = create_temp_skill_dir();
        let mut loader = SkillLoader::new(dir.path().to_path_buf());
        loader.load_skill("code_review").unwrap();

        let formatted = loader.format_skill_content("code_review").unwrap();
        assert!(formatted.contains("# Skill: code_review"));
    }

    #[test]
    fn test_skill_loader_missing_directory() {
        let loader = SkillLoader::new(PathBuf::from("/nonexistent/skills"));
        let skills = loader.discover_skills().unwrap();
        assert!(skills.is_empty());
    }

    #[test]
    fn test_skill_loader_missing_file() {
        let dir = create_temp_skill_dir();
        let mut loader = SkillLoader::new(dir.path().to_path_buf());

        let result = loader.load_skill("nonexistent_skill");
        assert!(result.is_err());
    }

    #[test]
    fn test_skill_loader_format_empty_summaries() {
        let loader = SkillLoader::new(PathBuf::from("/tmp"));
        let formatted = loader.format_skill_summaries();
        assert!(formatted.contains("No skills"));
    }
}
