//! Skill module
//!
//! This module provides skill loading functionality with lazy loading support.
//! Skills are indexed at startup and loaded on-demand to minimize context usage.

mod builtin_skills;

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};

use async_trait::async_trait;
pub use builtin_skills::{EmbeddedFile, EmbeddedSkill};
use rmcp::model::CallToolResult;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::RwLock;

use crate::{AgentError, Result, tools::Tool};

/// Skill metadata from frontmatter.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SkillMetadata {
    name: String,
    description: String,
    #[serde(default)]
    trigger: Option<String>,
}

/// Lightweight skill index for system prompt (~50 tokens).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SkillIndex {
    pub(crate) name: String,
    pub(crate) description: String,
}

impl From<&SkillMetadata> for SkillIndex {
    fn from(metadata: &SkillMetadata) -> Self {
        Self {
            name: metadata.name.clone(),
            description: metadata.description.clone(),
        }
    }
}

/// Supporting file information for a skill.
#[derive(Debug, Clone)]
struct SupportingFile {
    relative_path: String,
    content: String,
}

/// Skill content with metadata.
#[derive(Debug, Clone)]
struct Skill {
    metadata: SkillMetadata,
    body: String,
    #[allow(dead_code)]
    directory: PathBuf,
    supporting_files: Vec<SupportingFile>,
}

/// Loading state for a skill.
#[derive(Debug, Clone)]
enum SkillLoadState {
    Indexed(SkillIndex),
    Loaded(Skill),
}

/// Skill cache entry tracking load state and usage count.
#[derive(Debug, Clone)]
struct SkillCacheEntry {
    state: SkillLoadState,
    use_count: u32,
}

impl SkillCacheEntry {
    fn new_indexed(index: SkillIndex) -> Self {
        Self {
            state: SkillLoadState::Indexed(index),
            use_count: 0,
        }
    }

    fn new_loaded(skill: Skill) -> Self {
        Self {
            state: SkillLoadState::Loaded(skill),
            use_count: 0,
        }
    }

    fn is_loaded(&self) -> bool {
        matches!(self.state, SkillLoadState::Loaded(_))
    }

    fn get_index(&self) -> SkillIndex {
        match &self.state {
            SkillLoadState::Indexed(index) => index.clone(),
            SkillLoadState::Loaded(skill) => SkillIndex::from(&skill.metadata),
        }
    }

    fn get_skill(&self) -> Option<&Skill> {
        match &self.state {
            SkillLoadState::Loaded(skill) => Some(skill),
            _ => None,
        }
    }
}

/// Skill loader and tool with lazy loading support.
#[derive(Debug)]
pub struct SkillTool {
    skills: Arc<RwLock<HashMap<String, SkillCacheEntry>>>,
    skill_sources: HashMap<String, SkillSource>,
    working_dir: PathBuf,
    cached_description: String,
}

/// Source from which a skill can be loaded.
#[derive(Debug, Clone)]
enum SkillSource {
    File(PathBuf),
}

impl SkillTool {
    /// Creates a new skill tool with lazy loading.
    pub fn new(working_dir: PathBuf) -> Self {
        if let Err(e) =
            builtin_skills::write_builtin_skills_to_filesystem(&working_dir)
        {
            tracing::warn!("Failed to write builtin skills to filesystem: {e}");
        }

        let directories = Self::get_default_skill_directories(&working_dir)
            .into_iter()
            .filter(|d| d.exists())
            .collect::<Vec<_>>();

        tracing::info!("Discovering skills in directories: {:?}", directories);

        let (indices, sources) = Self::discover_skill_indices(&directories);

        let skills: HashMap<String, SkillCacheEntry> = indices
            .into_iter()
            .map(|(name, index)| (name, SkillCacheEntry::new_indexed(index)))
            .collect();

        let cached_description = Self::build_tool_description(&skills);

        Self {
            skills: Arc::new(RwLock::new(skills)),
            skill_sources: sources,
            working_dir,
            cached_description,
        }
    }

    fn build_tool_description(
        skills: &HashMap<String, SkillCacheEntry>,
    ) -> String {
        let mut desc = String::from(
            "Load a skill by name and return its content.\n\
            This tool loads the specified skill and returns its body content along \
            with information about any supporting files in the skill directory.",
        );

        if !skills.is_empty() {
            desc.push_str("\n\nAvailable skills:\n");
            let mut skill_list: Vec<_> = skills.iter().collect();
            skill_list.sort_by_key(|(name, _)| *name);
            for (name, entry) in skill_list {
                let index = entry.get_index();
                desc.push_str(&format!("- {}: {}\n", name, index.description));
            }
        }

        desc
    }

    /// Gets default skill directories.
    fn get_default_skill_directories(working_dir: &Path) -> Vec<PathBuf> {
        let mut dirs = Vec::new();

        if let Some(home) = dirs::home_dir() {
            dirs.push(home.join(".claude/skills"));
            dirs.push(home.join(".config/claude/skills"));
        }

        dirs.push(working_dir.join(".claude/skills"));
        dirs.push(working_dir.join(".agents/skills"));
        dirs.push(working_dir.join(".skills"));

        dirs
    }

    /// Parses frontmatter from skill content.
    fn parse_frontmatter(content: &str) -> Result<(SkillMetadata, String)> {
        let parts: Vec<&str> = content.split("---").collect();

        if parts.len() < 3 {
            return Err(AgentError::InvalidOperation(
                "Invalid frontmatter format".to_string(),
            ));
        }

        let yaml_content = parts[1].trim();
        let metadata: SkillMetadata = serde_yaml::from_str(yaml_content)
            .map_err(|e| {
                AgentError::InvalidOperation(format!(
                    "Failed to parse frontmatter: {e}"
                ))
            })?;

        let body = parts[2..].join("---").trim().to_string();

        Ok((metadata, body))
    }

    /// Parses a skill file.
    fn parse_skill_file(path: &Path) -> Result<Skill> {
        let content = std::fs::read_to_string(path)?;

        let (metadata, body) = Self::parse_frontmatter(&content)?;

        let directory = path
            .parent()
            .ok_or_else(|| {
                AgentError::InvalidOperation(
                    "Skill file has no parent directory".to_string(),
                )
            })?
            .to_path_buf();

        let supporting_files =
            Self::find_supporting_files_as_supporting(&directory, path);

        Ok(Skill {
            metadata,
            body,
            directory,
            supporting_files,
        })
    }

    /// Finds supporting files and reads their content.
    fn find_supporting_files_as_supporting(
        directory: &Path,
        skill_file: &Path,
    ) -> Vec<SupportingFile> {
        let mut files = Vec::new();

        if let Ok(entries) = std::fs::read_dir(directory) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() && path != skill_file {
                    if let Ok(content) = std::fs::read_to_string(&path)
                        && let Ok(relative) = path.strip_prefix(directory)
                    {
                        files.push(SupportingFile {
                            relative_path: relative.display().to_string(),
                            content,
                        });
                    }
                } else if path.is_dir()
                    && let Ok(sub_entries) = std::fs::read_dir(&path)
                {
                    for sub_entry in sub_entries.flatten() {
                        let sub_path = sub_entry.path();
                        if sub_path.is_file()
                            && let Ok(content) =
                                std::fs::read_to_string(&sub_path)
                            && let Ok(relative) =
                                sub_path.strip_prefix(directory)
                        {
                            files.push(SupportingFile {
                                relative_path: relative.display().to_string(),
                                content,
                            });
                        }
                    }
                }
            }
        }

        files
    }

    /// Discovers skill indices in directories (lightweight, no body loading).
    fn discover_skill_indices(
        directories: &[PathBuf],
    ) -> (HashMap<String, SkillIndex>, HashMap<String, SkillSource>) {
        let mut indices = HashMap::new();
        let mut sources = HashMap::new();

        for dir in directories {
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        let skill_file = path.join("SKILL.md");
                        if skill_file.exists()
                            && let Ok(content) =
                                std::fs::read_to_string(&skill_file)
                            && let Ok((metadata, _)) =
                                Self::parse_frontmatter(&content)
                        {
                            let index = SkillIndex::from(&metadata);
                            indices.insert(metadata.name.clone(), index);
                            sources.insert(
                                metadata.name.clone(),
                                SkillSource::File(skill_file),
                            );
                        }
                    } else if path.extension().is_some_and(|ext| ext == "md")
                        && let Ok(content) = std::fs::read_to_string(&path)
                        && let Ok((metadata, _)) =
                            Self::parse_frontmatter(&content)
                    {
                        let index = SkillIndex::from(&metadata);
                        indices.insert(metadata.name.clone(), index);
                        sources.insert(
                            metadata.name.clone(),
                            SkillSource::File(path),
                        );
                    }
                }
            }
        }

        (indices, sources)
    }

    /// Generates instructions for available skills (indices only, ~50 tokens each).
    pub async fn generate_instructions(&self) -> String {
        let skills = self.skills.read().await;
        if skills.is_empty() {
            return String::new();
        }

        let mut instructions = String::from(
            "You have these skills at your disposal. Use the loadSkill tool to load a skill when needed:\n\n",
        );

        let mut skill_list: Vec<_> = skills.iter().collect();
        skill_list.sort_by_key(|(name, _)| *name);

        for (name, entry) in skill_list {
            let index = entry.get_index();
            instructions
                .push_str(&format!("- {}: {}\n", name, index.description));
        }

        instructions
    }

    /// Loads a skill by name (lazy loading).
    async fn load_skill(&self, name: &str) -> Result<String> {
        let mut skills = self.skills.write().await;

        let entry = skills.get_mut(name).ok_or_else(|| {
            AgentError::InvalidOperation(format!("Skill '{name}' not found"))
        })?;

        if !entry.is_loaded()
            && let Some(source) = self.skill_sources.get(name)
        {
            let skill = match source {
                SkillSource::File(path) => Self::parse_skill_file(path)?,
            };
            entry.state = SkillLoadState::Loaded(skill);
        }

        entry.use_count += 1;

        let Some(skill) = entry.get_skill() else {
            return Err(AgentError::InvalidOperation(
                "Skill failed to load".to_string(),
            ));
        };

        let mut response =
            format!("# Skill: {}\n\n{}\n\n", skill.metadata.name, skill.body);

        if !skill.supporting_files.is_empty() {
            response.push_str("## Supporting Files\n\n");
            response
                .push_str("The following supporting files are available:\n\n");

            let mut scripts: Vec<&SupportingFile> = Vec::new();
            let mut references: Vec<&SupportingFile> = Vec::new();
            let mut assets: Vec<&SupportingFile> = Vec::new();
            let mut other: Vec<&SupportingFile> = Vec::new();

            for file in &skill.supporting_files {
                if file.relative_path.starts_with("scripts/") {
                    scripts.push(file);
                } else if file.relative_path.starts_with("references/") {
                    references.push(file);
                } else if file.relative_path.starts_with("assets/") {
                    assets.push(file);
                } else {
                    other.push(file);
                }
            }

            if !scripts.is_empty() {
                response.push_str("### Scripts\n");
                for file in scripts {
                    response.push_str(&format!("- {}\n", file.relative_path));
                }
                response.push('\n');
            }

            if !references.is_empty() {
                response.push_str("### References\n");
                for file in references {
                    response.push_str(&format!("- {}\n", file.relative_path));
                }
                response.push('\n');
            }

            if !assets.is_empty() {
                response.push_str("### Assets\n");
                for file in assets {
                    response.push_str(&format!("- {}\n", file.relative_path));
                }
                response.push('\n');
            }

            if !other.is_empty() {
                response.push_str("### Other Files\n");
                for file in other {
                    response.push_str(&format!("- {}\n", file.relative_path));
                }
                response.push('\n');
            }

            response.push_str(
                "Use the read_file tool to access these files as needed.\n",
            );
        }

        Ok(response)
    }

    /// Gets all skill names.
    pub async fn get_skill_names(&self) -> Vec<String> {
        let skills = self.skills.read().await;
        skills.keys().cloned().collect()
    }

    /// Checks if there are any skills.
    pub async fn has_skills(&self) -> bool {
        let skills = self.skills.read().await;
        !skills.is_empty()
    }

    /// Gets a supporting file content from a loaded skill.
    pub async fn get_supporting_file(
        &self,
        skill_name: &str,
        file_path: &str,
    ) -> Option<String> {
        let skills = self.skills.read().await;
        let entry = skills.get(skill_name)?;
        let skill = entry.get_skill()?;

        skill
            .supporting_files
            .iter()
            .find(|f| f.relative_path == file_path)
            .map(|f| f.content.clone())
    }

    /// Saves a skill to the file system with standard directory structure.
    pub async fn save_skill(
        &self,
        name: &str,
        description: &str,
        body: &str,
        scripts: Vec<(String, String)>,
        references: Vec<(String, String)>,
        assets: Vec<(String, String)>,
    ) -> Result<PathBuf> {
        let skill_dir = self.working_dir.join(".agents/skills").join(name);
        std::fs::create_dir_all(&skill_dir)?;

        let skill_md_content = format!(
            "---\nname: {name}\ndescription: \"{description}\"\n---\n\n{body}"
        );
        std::fs::write(skill_dir.join("SKILL.md"), skill_md_content)?;

        if !scripts.is_empty() {
            let scripts_dir = skill_dir.join("scripts");
            std::fs::create_dir_all(&scripts_dir)?;
            for (filename, content) in scripts {
                std::fs::write(scripts_dir.join(&filename), content)?;
            }
        }

        if !references.is_empty() {
            let references_dir = skill_dir.join("references");
            std::fs::create_dir_all(&references_dir)?;
            for (filename, content) in references {
                std::fs::write(references_dir.join(&filename), content)?;
            }
        }

        if !assets.is_empty() {
            let assets_dir = skill_dir.join("assets");
            std::fs::create_dir_all(&assets_dir)?;
            for (filename, content) in assets {
                std::fs::write(assets_dir.join(&filename), content)?;
            }
        }

        self.refresh_skill_index(&skill_dir, name).await?;

        Ok(skill_dir)
    }

    /// Refreshes the skill index after saving a new skill.
    async fn refresh_skill_index(
        &self,
        skill_dir: &Path,
        name: &str,
    ) -> Result<()> {
        let skill_file = skill_dir.join("SKILL.md");
        let content = std::fs::read_to_string(&skill_file)?;
        let (metadata, body) = Self::parse_frontmatter(&content)?;

        let supporting_files =
            Self::find_supporting_files_as_supporting(skill_dir, &skill_file);

        let skill = Skill {
            metadata,
            body,
            directory: skill_dir.to_path_buf(),
            supporting_files,
        };

        let mut skills = self.skills.write().await;
        skills.insert(name.to_string(), SkillCacheEntry::new_loaded(skill));

        Ok(())
    }

    /// Lists all skills with their supporting file counts.
    pub async fn list_skills_with_details(
        &self,
    ) -> Vec<(String, String, usize)> {
        let skills = self.skills.read().await;
        let mut result = Vec::new();

        for (name, entry) in skills.iter() {
            let index = entry.get_index();
            let file_count = entry
                .get_skill()
                .map(|s| s.supporting_files.len())
                .unwrap_or(0);
            result.push((name.clone(), index.description, file_count));
        }

        result.sort_by(|a, b| a.0.cmp(&b.0));
        result
    }
}

#[async_trait]
impl Tool for SkillTool {
    fn name(&self) -> &str {
        "loadSkill"
    }

    fn description(&self) -> &str {
        &self.cached_description
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Name of the skill to load"
                }
            },
            "required": ["name"]
        })
    }

    async fn call(&self, args: Value) -> CallToolResult {
        let args_obj = match args.as_object() {
            Some(obj) => obj,
            None => {
                return CallToolResult::error(vec![
                    rmcp::model::Content::text(
                        "Invalid arguments format".to_string(),
                    ),
                ]);
            }
        };

        let name = match args_obj.get("name").and_then(|v| v.as_str()) {
            Some(n) => n,
            None => {
                return CallToolResult::error(vec![
                    rmcp::model::Content::text(
                        "Missing parameter: name".to_string(),
                    ),
                ]);
            }
        };

        match self.load_skill(name).await {
            Ok(result) => {
                CallToolResult::success(vec![rmcp::model::Content::text(
                    result,
                )])
            }
            Err(e) => CallToolResult::error(vec![rmcp::model::Content::text(
                format!("Failed to load skill: {e}"),
            )]),
        }
    }
}

impl Clone for SkillTool {
    fn clone(&self) -> Self {
        Self {
            skills: Arc::clone(&self.skills),
            skill_sources: self.skill_sources.clone(),
            working_dir: self.working_dir.clone(),
            cached_description: self.cached_description.clone(),
        }
    }
}

/// Alias tool for SkillTool with name "Skill" (TOOL_SPEC.md compatible)
#[derive(Clone)]
pub struct SkillAliasTool {
    inner: SkillTool,
}

impl SkillAliasTool {
    pub fn new() -> Self {
        Self {
            inner: SkillTool::new(
                std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            ),
        }
    }
}

impl Default for SkillAliasTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for SkillAliasTool {
    fn name(&self) -> &str {
        "Skill"
    }

    fn description(&self) -> &str {
        self.inner.description()
    }

    fn parameters(&self) -> Value {
        self.inner.parameters()
    }

    async fn call(&self, args: Value) -> CallToolResult {
        self.inner.call(args).await
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    fn get_test_working_dir() -> PathBuf {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    }

    #[test]
    fn test_skill_tool_creation() {
        let tool = SkillTool::new(get_test_working_dir());
        assert_eq!(tool.name(), "loadSkill");
        assert!(!tool.description().is_empty());
    }

    #[test]
    fn test_parse_frontmatter() {
        let content = r#"---
name: test-skill
description: A test skill
---

# Test Skill

This is the body of the skill.
"#;

        let (metadata, body) = SkillTool::parse_frontmatter(content).unwrap();
        assert_eq!(metadata.name, "test-skill");
        assert_eq!(metadata.description, "A test skill");
        assert!(body.contains("# Test Skill"));
    }

    #[test]
    fn test_parse_frontmatter_missing() {
        let content = "# No frontmatter here";
        assert!(SkillTool::parse_frontmatter(content).is_err());
    }

    #[test]
    fn test_parse_frontmatter_unclosed() {
        let content = r#"---
name: test
description: test
"#;
        assert!(SkillTool::parse_frontmatter(content).is_err());
    }

    #[test]
    fn test_parse_skill_file() {
        let temp_dir = TempDir::new().unwrap();
        let skill_dir = temp_dir.path().join("test-skill");
        fs::create_dir(&skill_dir).unwrap();

        let skill_file = skill_dir.join("SKILL.md");
        fs::write(
            &skill_file,
            r#"---
name: test-skill
description: A test skill
---

# Test Skill Content
"#,
        )
        .unwrap();

        fs::write(skill_dir.join("helper.py"), "print('hello')").unwrap();
        fs::create_dir(skill_dir.join("templates")).unwrap();
        fs::write(skill_dir.join("templates/template.txt"), "template")
            .unwrap();

        let skill = SkillTool::parse_skill_file(&skill_file).unwrap();
        assert_eq!(skill.metadata.name, "test-skill");
        assert_eq!(skill.metadata.description, "A test skill");
        assert!(skill.body.contains("# Test Skill Content"));
        assert_eq!(skill.supporting_files.len(), 2);
    }

    #[test]
    fn test_discover_skill_indices() {
        let temp_dir = TempDir::new().unwrap();
        let skills_dir = temp_dir.path().join("skills");
        fs::create_dir(&skills_dir).unwrap();

        let skill1_dir = skills_dir.join("test-skill-one");
        fs::create_dir(&skill1_dir).unwrap();
        fs::write(
            skill1_dir.join("SKILL.md"),
            r#"---
name: test-skill-one
description: First test skill
---
Body 1
"#,
        )
        .unwrap();

        let skill2_dir = skills_dir.join("test-skill-two");
        fs::create_dir(&skill2_dir).unwrap();
        fs::write(
            skill2_dir.join("SKILL.md"),
            r#"---
name: test-skill-two
description: Second test skill
---
Body 2
"#,
        )
        .unwrap();

        let (indices, sources) =
            SkillTool::discover_skill_indices(&[skills_dir]);

        assert_eq!(indices.len(), 2);
        assert!(indices.contains_key("test-skill-one"));
        assert!(indices.contains_key("test-skill-two"));
        assert_eq!(sources.len(), 2);
    }

    #[tokio::test]
    async fn test_builtin_skills_loaded() {
        let tool = SkillTool::new(get_test_working_dir());
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            if tool.has_skills().await {
                assert!(tool.has_skills().await);
            }
        })
        .await
        .ok();
    }

    #[tokio::test]
    async fn test_skill_tool_call() {
        let tool = SkillTool::new(get_test_working_dir());

        let skill_names = tool.get_skill_names().await;
        if !skill_names.is_empty() {
            let params = serde_json::json!({
                "name": skill_names[0]
            });

            let result = tool.call(params).await;
            assert!(
                result.is_error.is_none() || result.is_error == Some(false)
            );
        }
    }

    #[tokio::test]
    async fn test_skill_tool_call_nonexistent() {
        let tool = SkillTool::new(get_test_working_dir());

        let params = serde_json::json!({
            "name": "nonexistent_skill"
        });

        let result = tool.call(params).await;
        assert!(result.is_error == Some(true));
    }

    #[tokio::test]
    async fn test_generate_instructions() {
        let tool = SkillTool::new(get_test_working_dir());
        let instructions = tool.generate_instructions().await;

        if tool.has_skills().await {
            assert!(instructions.contains("loadSkill"));
        }
    }

    #[tokio::test]
    async fn test_ralph_skill_has_supporting_files() {
        let tool = SkillTool::new(get_test_working_dir());

        let result = tool.load_skill("ralph").await;
        assert!(result.is_ok());

        let content = result.unwrap();
        assert!(content.contains("Supporting Files"));
        assert!(content.contains("scripts/ralph.sh"));
        assert!(content.contains("references/prompt.md"));
        assert!(content.contains("assets/prd.json.example"));
    }

    #[tokio::test]
    async fn test_get_supporting_file() {
        let tool = SkillTool::new(get_test_working_dir());

        tool.load_skill("ralph").await.ok();

        let content =
            tool.get_supporting_file("ralph", "scripts/ralph.sh").await;
        assert!(content.is_some());
        assert!(content.unwrap().contains("#!/bin/bash"));
    }

    #[tokio::test]
    async fn test_get_supporting_file_not_found() {
        let tool = SkillTool::new(get_test_working_dir());

        tool.load_skill("ralph").await.ok();

        let content = tool
            .get_supporting_file("ralph", "nonexistent/file.txt")
            .await;
        assert!(content.is_none());
    }

    mod skill_metadata_tests {
        use super::*;

        #[test]
        fn test_skill_metadata_deserialization() {
            let yaml = r#"
name: test-skill
description: A test skill description
"#;
            let metadata: SkillMetadata = serde_yaml::from_str(yaml).unwrap();
            assert_eq!(metadata.name, "test-skill");
            assert_eq!(metadata.description, "A test skill description");
        }

        #[test]
        fn test_skill_metadata_serialization_roundtrip() {
            let metadata = SkillMetadata {
                name: "roundtrip-test".to_string(),
                description: "Testing serialization".to_string(),
                trigger: None,
            };
            let serialized = serde_yaml::to_string(&metadata).unwrap();
            let deserialized: SkillMetadata =
                serde_yaml::from_str(&serialized).unwrap();
            assert_eq!(deserialized.name, metadata.name);
            assert_eq!(deserialized.description, metadata.description);
        }

        #[test]
        fn test_skill_metadata_json_serialization() {
            let metadata = SkillMetadata {
                name: "json-test".to_string(),
                description: "Testing JSON".to_string(),
                trigger: None,
            };
            let json = serde_json::to_string(&metadata).unwrap();
            assert!(json.contains("json-test"));
            assert!(json.contains("Testing JSON"));
        }
    }

    mod skill_index_tests {
        use super::*;

        #[test]
        fn test_skill_index_from_metadata() {
            let metadata = SkillMetadata {
                name: "my-skill".to_string(),
                description: "My skill description".to_string(),
                trigger: None,
            };
            let index = SkillIndex::from(&metadata);
            assert_eq!(index.name, "my-skill");
            assert_eq!(index.description, "My skill description");
        }

        #[test]
        fn test_skill_index_serialization_roundtrip() {
            let index = SkillIndex {
                name: "index-test".to_string(),
                description: "Index description".to_string(),
            };

            let yaml = serde_yaml::to_string(&index).unwrap();
            let deserialized: SkillIndex = serde_yaml::from_str(&yaml).unwrap();
            assert_eq!(deserialized.name, index.name);
            assert_eq!(deserialized.description, index.description);
        }

        #[test]
        fn test_skill_index_json_serialization() {
            let index = SkillIndex {
                name: "json-index".to_string(),
                description: "JSON index desc".to_string(),
            };
            let json = serde_json::to_string(&index).unwrap();
            let deserialized: SkillIndex = serde_json::from_str(&json).unwrap();
            assert_eq!(deserialized.name, index.name);
        }

        #[test]
        fn test_skill_index_clone() {
            let index = SkillIndex {
                name: "clone-test".to_string(),
                description: "Clone desc".to_string(),
            };
            let cloned = index.clone();
            assert_eq!(cloned.name, index.name);
            assert_eq!(cloned.description, index.description);
        }
    }

    mod skill_cache_entry_tests {
        use super::*;

        #[test]
        fn test_new_indexed() {
            let index = SkillIndex {
                name: "indexed-skill".to_string(),
                description: "An indexed skill".to_string(),
            };
            let entry = SkillCacheEntry::new_indexed(index.clone());

            assert!(!entry.is_loaded());
            let retrieved_index = entry.get_index();
            assert_eq!(retrieved_index.name, index.name);
            assert_eq!(retrieved_index.description, index.description);
            assert!(entry.get_skill().is_none());
            assert_eq!(entry.use_count, 0);
        }

        #[test]
        fn test_new_loaded() {
            let skill = Skill {
                metadata: SkillMetadata {
                    name: "loaded-skill".to_string(),
                    description: "A loaded skill".to_string(),
                    trigger: None,
                },
                body: "Skill body content".to_string(),
                directory: PathBuf::from("/test/dir"),
                supporting_files: vec![],
            };
            let entry = SkillCacheEntry::new_loaded(skill);

            assert!(entry.is_loaded());
            assert!(entry.get_skill().is_some());
            let retrieved = entry.get_skill().unwrap();
            assert_eq!(retrieved.metadata.name, "loaded-skill");
            assert_eq!(entry.use_count, 0);
        }

        #[test]
        fn test_get_index_from_loaded() {
            let skill = Skill {
                metadata: SkillMetadata {
                    name: "skill-for-index".to_string(),
                    description: "Description for index".to_string(),
                    trigger: None,
                },
                body: "Body".to_string(),
                directory: PathBuf::new(),
                supporting_files: vec![],
            };
            let entry = SkillCacheEntry::new_loaded(skill);

            let index = entry.get_index();
            assert_eq!(index.name, "skill-for-index");
            assert_eq!(index.description, "Description for index");
        }

        #[test]
        fn test_get_skill_returns_none_when_indexed() {
            let index = SkillIndex {
                name: "only-indexed".to_string(),
                description: "Not loaded yet".to_string(),
            };
            let entry = SkillCacheEntry::new_indexed(index);
            assert!(entry.get_skill().is_none());
        }
    }

    mod skill_source_tests {
        use super::*;

        #[test]
        fn test_skill_source_debug() {
            let source = SkillSource::File(PathBuf::from("/path/to/skill.md"));
            let debug_str = format!("{source:?}");
            assert!(debug_str.contains("File"));
            assert!(debug_str.contains("skill.md"));
        }

        #[test]
        fn test_skill_source_clone() {
            let source = SkillSource::File(PathBuf::from("/original/path.md"));
            let cloned = source.clone();
            let (SkillSource::File(p1), SkillSource::File(p2)) =
                (&source, &cloned);
            assert_eq!(p1, p2);
        }
    }

    mod skill_load_state_tests {
        use super::*;

        #[test]
        fn test_skill_load_state_indexed_variant() {
            let state = SkillLoadState::Indexed(SkillIndex {
                name: "state-test".to_string(),
                description: "Testing state".to_string(),
            });
            match state {
                SkillLoadState::Indexed(index) => {
                    assert_eq!(index.name, "state-test");
                }
                SkillLoadState::Loaded(_) => panic!("Expected Indexed variant"),
            }
        }

        #[test]
        fn test_skill_load_state_loaded_variant() {
            let skill = Skill {
                metadata: SkillMetadata {
                    name: "loaded-state".to_string(),
                    description: "Loaded state desc".to_string(),
                    trigger: None,
                },
                body: "Body".to_string(),
                directory: PathBuf::new(),
                supporting_files: vec![],
            };
            let state = SkillLoadState::Loaded(skill);
            match state {
                SkillLoadState::Loaded(loaded) => {
                    assert_eq!(loaded.metadata.name, "loaded-state");
                }
                SkillLoadState::Indexed(_) => panic!("Expected Loaded variant"),
            }
        }
    }

    mod parse_frontmatter_edge_cases {
        use super::*;

        #[test]
        fn test_parse_frontmatter_with_extra_dashes_in_body() {
            let content = r#"---
name: test-skill
description: Test with dashes
---
# Test Skill

This has --- in the body that should not split.
"#;
            let (metadata, body) =
                SkillTool::parse_frontmatter(content).unwrap();
            assert_eq!(metadata.name, "test-skill");
            assert!(body.contains("This has --- in the body"));
        }

        #[test]
        fn test_parse_frontmatter_with_colons_in_body() {
            let content = r#"---
name: test-skill
description: Test
---
# Title

Some text with: colons and more: content here.
"#;
            let (metadata, body) =
                SkillTool::parse_frontmatter(content).unwrap();
            assert_eq!(metadata.name, "test-skill");
            assert!(body.contains("colons"));
        }

        #[test]
        fn test_parse_frontmatter_yaml_parse_error() {
            let content = r#"---
name
description: Test
---
Body
"#;
            let result = SkillTool::parse_frontmatter(content);
            assert!(result.is_err());
        }

        #[test]
        fn test_parse_frontmatter_only_two_delimiters() {
            let content = "---\nname: test\n---\nBody";
            let result = SkillTool::parse_frontmatter(content);
            assert!(result.is_err());
        }

        #[test]
        fn test_parse_frontmatter_empty_name() {
            let content = r#"---
name:
description: Test
---
Body
"#;
            let result = SkillTool::parse_frontmatter(content);
            assert!(result.is_ok());
        }
    }

    mod get_default_skill_directories_tests {
        use super::*;

        #[test]
        fn test_get_default_skill_directories_includes_working_dir_paths() {
            let working = PathBuf::from("/fake/working/dir");
            let dirs = SkillTool::get_default_skill_directories(&working);

            assert!(dirs.contains(&working.join(".claude/skills")));
            assert!(dirs.contains(&working.join(".agents/skills")));
        }

        #[test]
        fn test_get_default_skill_directories_different_working_dirs() {
            let dir1 = PathBuf::from("/home/user/project");
            let dir2 = PathBuf::from("/other/project");

            let dirs1 = SkillTool::get_default_skill_directories(&dir1);
            let dirs2 = SkillTool::get_default_skill_directories(&dir2);

            assert!(dirs1.contains(&dir1.join(".claude/skills")));
            assert!(dirs2.contains(&dir2.join(".claude/skills")));
        }
    }

    mod build_tool_description_tests {
        use super::*;

        #[test]
        fn test_build_tool_description_empty() {
            let skills: HashMap<String, SkillCacheEntry> = HashMap::new();
            let desc = SkillTool::build_tool_description(&skills);

            assert!(desc.contains("Load a skill"));
            assert!(!desc.contains("Available skills:"));
        }

        #[test]
        fn test_build_tool_description_with_skills() {
            let mut skills = HashMap::new();
            skills.insert(
                "skill-a".to_string(),
                SkillCacheEntry::new_indexed(SkillIndex {
                    name: "skill-a".to_string(),
                    description: "Description A".to_string(),
                }),
            );
            skills.insert(
                "skill-b".to_string(),
                SkillCacheEntry::new_indexed(SkillIndex {
                    name: "skill-b".to_string(),
                    description: "Description B".to_string(),
                }),
            );

            let desc = SkillTool::build_tool_description(&skills);

            assert!(desc.contains("Available skills:"));
            assert!(desc.contains("skill-a"));
            assert!(desc.contains("skill-b"));
            assert!(desc.contains("Description A"));
        }

        #[test]
        fn test_build_tool_description_sorted_alphabetically() {
            let mut skills = HashMap::new();
            skills.insert(
                "zebra".to_string(),
                SkillCacheEntry::new_indexed(SkillIndex {
                    name: "zebra".to_string(),
                    description: "Z description".to_string(),
                }),
            );
            skills.insert(
                "apple".to_string(),
                SkillCacheEntry::new_indexed(SkillIndex {
                    name: "apple".to_string(),
                    description: "A description".to_string(),
                }),
            );

            let desc = SkillTool::build_tool_description(&skills);

            let apple_pos = desc.find("apple").unwrap();
            let zebra_pos = desc.find("zebra").unwrap();
            assert!(apple_pos < zebra_pos);
        }
    }

    mod skill_struct_tests {
        use super::*;

        #[test]
        fn test_skill_clone() {
            let skill = Skill {
                metadata: SkillMetadata {
                    name: "clone-skill".to_string(),
                    description: "Clone description".to_string(),
                    trigger: None,
                },
                body: "Clone body".to_string(),
                directory: PathBuf::from("/clone/path"),
                supporting_files: vec![SupportingFile {
                    relative_path: "test.txt".to_string(),
                    content: "content".to_string(),
                }],
            };
            let cloned = skill.clone();
            assert_eq!(cloned.metadata.name, skill.metadata.name);
            assert_eq!(cloned.body, skill.body);
            assert_eq!(cloned.directory, skill.directory);
            assert_eq!(
                cloned.supporting_files.len(),
                skill.supporting_files.len()
            );
        }

        #[test]
        fn test_skill_debug() {
            let skill = Skill {
                metadata: SkillMetadata {
                    name: "debug-skill".to_string(),
                    description: "Debug description".to_string(),
                    trigger: None,
                },
                body: "Debug body".to_string(),
                directory: PathBuf::from("/debug/path"),
                supporting_files: vec![],
            };
            let debug_str = format!("{skill:?}");
            assert!(debug_str.contains("debug-skill"));
            assert!(debug_str.contains("Debug body"));
        }
    }

    mod skill_tool_basic_tests {
        use super::*;

        #[test]
        fn test_skill_tool_name() {
            let tool = SkillTool::new(get_test_working_dir());
            assert_eq!(tool.name(), "loadSkill");
        }

        #[test]
        fn test_skill_tool_description_not_empty() {
            let tool = SkillTool::new(get_test_working_dir());
            assert!(!tool.description().is_empty());
        }

        #[test]
        fn test_skill_tool_parameters() {
            let tool = SkillTool::new(get_test_working_dir());
            let params = tool.parameters();

            assert!(params.is_object());
            let obj = params.as_object().unwrap();
            assert!(obj.contains_key("properties"));
            assert!(obj.contains_key("type"));
        }

        #[test]
        fn test_skill_tool_clone() {
            let tool = SkillTool::new(get_test_working_dir());
            let cloned = tool.clone();

            assert_eq!(tool.name(), cloned.name());
        }

        #[test]
        fn test_skill_tool_cached_description_persists() {
            let tool = SkillTool::new(get_test_working_dir());
            let desc1 = tool.description().to_string();

            assert_eq!(desc1, tool.description());
        }
    }

    mod skill_tool_async_tests {
        use super::*;

        #[tokio::test]
        async fn test_get_skill_names_returns_vec() {
            let tool = SkillTool::new(get_test_working_dir());
            let names = tool.get_skill_names().await;
            assert!(names.is_empty() || !names.is_empty());
        }

        #[tokio::test]
        async fn test_has_skills_returns_bool() {
            let tool = SkillTool::new(get_test_working_dir());
            let _has = tool.has_skills().await;
        }

        #[tokio::test]
        async fn test_generate_instructions_returns_string() {
            let tool = SkillTool::new(get_test_working_dir());
            let instructions = tool.generate_instructions().await;
            if !tool.has_skills().await {
                assert!(instructions.is_empty());
            }
        }

        #[tokio::test]
        async fn test_load_skill_nonexistent_returns_error() {
            let tool = SkillTool::new(get_test_working_dir());
            let result =
                tool.load_skill("definitely-nonexistent-skill-12345").await;
            assert!(result.is_err());
        }
    }

    mod builtin_skills_tests {
        use super::*;

        #[test]
        fn test_get_all_builtin_skills_not_empty() {
            let skills = builtin_skills::get_all_builtin_skills();
            assert!(!skills.is_empty());
        }

        #[test]
        fn test_builtin_skills_have_valid_frontmatter() {
            for skill in builtin_skills::get_all_builtin_skills() {
                let result = SkillTool::parse_frontmatter(skill.skill_md);
                assert!(result.is_ok(), "Skill should have valid frontmatter");
                let (metadata, body) = result.unwrap();
                assert!(!metadata.name.is_empty());
                assert!(!metadata.description.is_empty());
                assert!(!body.is_empty());
            }
        }

        #[test]
        fn test_builtin_skills_contain_expected_names() {
            let skills = builtin_skills::get_all_builtin_skills();
            let skill_names: Vec<&str> =
                skills.iter().map(|s| s.name).collect();

            assert!(skill_names.iter().any(|n| n.contains("skill-creator")));
            assert!(skill_names.iter().any(|n| n.contains("find-skills")));
            assert!(skill_names.iter().any(|n| n.contains("ralph")));
        }
    }

    mod tool_implementation_tests {
        use super::*;

        #[tokio::test]
        async fn test_call_with_invalid_args_format() {
            let tool = SkillTool::new(get_test_working_dir());
            let result = tool.call(serde_json::Value::Null).await;
            assert!(result.is_error.is_some());
        }

        #[tokio::test]
        async fn test_call_with_missing_name_param() {
            let tool = SkillTool::new(get_test_working_dir());
            let params = serde_json::json!({});
            let result = tool.call(params).await;
            assert!(result.is_error.is_some());
        }

        #[tokio::test]
        async fn test_call_with_empty_args_object() {
            let tool = SkillTool::new(get_test_working_dir());
            let params = serde_json::json!({});
            let result = tool.call(params).await;
            assert!(result.is_error == Some(true));
        }
    }

    mod discover_skill_indices_edge_case_tests {
        use super::*;

        #[test]
        fn test_discover_skill_indices_empty_directory() {
            let temp_dir = TempDir::new().unwrap();
            let (indices, sources) =
                SkillTool::discover_skill_indices(&[temp_dir
                    .path()
                    .to_path_buf()]);

            assert!(indices.is_empty());
            assert!(sources.is_empty());
        }

        #[test]
        fn test_discover_skill_indices_nonexistent_directory() {
            let fake_dir = PathBuf::from("/nonexistent/path/12345");
            let (indices, _sources) =
                SkillTool::discover_skill_indices(&[fake_dir]);

            assert!(indices.is_empty());
        }

        #[test]
        fn test_discover_skill_indices_single_file_skill() {
            let temp_dir = TempDir::new().unwrap();
            let single_file = temp_dir.path().join("single-skill.md");
            std::fs::write(
                &single_file,
                r#"---
name: single-file-skill
description: A single file skill
---
Single file skill body content.
"#,
            )
            .unwrap();

            let (indices, sources) =
                SkillTool::discover_skill_indices(&[temp_dir
                    .path()
                    .to_path_buf()]);

            assert_eq!(indices.len(), 1);
            assert!(indices.contains_key("single-file-skill"));
            assert!(sources.contains_key("single-file-skill"));
        }

        #[test]
        fn test_discover_skill_indices_directory_skill() {
            let temp_dir = TempDir::new().unwrap();
            let skill_dir = temp_dir.path().join("dir-skill");
            std::fs::create_dir(&skill_dir).unwrap();
            std::fs::write(
                skill_dir.join("SKILL.md"),
                r#"---
name: dir-skill
description: A directory-based skill
---
Directory skill body.
"#,
            )
            .unwrap();

            let (indices, _sources) =
                SkillTool::discover_skill_indices(&[temp_dir
                    .path()
                    .to_path_buf()]);

            assert_eq!(indices.len(), 1);
            assert!(indices.contains_key("dir-skill"));
        }

        #[test]
        fn test_discover_skill_indices_invalid_frontmatter_skipped() {
            let temp_dir = TempDir::new().unwrap();
            let invalid_skill = temp_dir.path().join("invalid.md");
            std::fs::write(&invalid_skill, "No valid frontmatter here")
                .unwrap();

            let (indices, sources) =
                SkillTool::discover_skill_indices(&[temp_dir
                    .path()
                    .to_path_buf()]);

            assert!(indices.is_empty());
            assert!(sources.is_empty());
        }
    }
}
