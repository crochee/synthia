use std::{collections::HashMap, path::Path};

use sha2::{Digest, Sha256};
use synthia_core::Error;

use crate::types::{SkillLevels, SkillMetadata};

pub struct SkillLoader;

pub enum LoadResult {
    Success(crate::types::Skill),
    Skipped {
        path: std::path::PathBuf,
        reason: String,
    },
    Warned {
        skill: crate::types::Skill,
        warnings: Vec<String>,
    },
}

impl SkillLoader {
    pub fn parse_frontmatter(path: &Path) -> Result<SkillMetadata, Error> {
        let content = std::fs::read_to_string(path)?;
        let parts: Vec<&str> = content.splitn(3, "---").collect();
        if parts.len() < 3 {
            return Err(Error::InvalidItem(format!(
                "invalid SKILL.md format at {:?}",
                path
            )));
        }
        let frontmatter = parts[1].trim();
        let mut metadata: SkillMetadata = serde_yaml::from_str(frontmatter)?;
        if metadata.name.is_empty() {
            return Err(Error::InvalidItem(
                "missing required field: name".to_string(),
            ));
        }
        if metadata.description.is_empty() {
            return Err(Error::InvalidItem(
                "missing required field: description".to_string(),
            ));
        }
        let dir_name = path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .ok_or_else(|| {
                Error::InvalidItem(format!(
                    "invalid SKILL.md format at {:?}",
                    path
                ))
            })?;
        if metadata.name != dir_name {
            return Err(Error::InvalidItem(format!(
                "skill name mismatch: expected {}, found {}",
                dir_name, metadata.name
            )));
        }
        if metadata.levels.level0.is_none()
            && metadata.levels.level1.is_none()
            && metadata.levels.level2.is_none()
        {
            metadata.levels = SkillLevels::new();
        }
        Ok(metadata)
    }

    pub fn parse_body(path: &Path) -> Result<String, Error> {
        let content = std::fs::read_to_string(path)?;
        Self::extract_body_from_content(&content)
    }

    pub fn extract_body_from_content(content: &str) -> Result<String, Error> {
        let parts: Vec<&str> = content.splitn(3, "---").collect();
        if parts.len() < 3 {
            return Err(Error::InvalidItem(
                "invalid SKILL.md format at <content>".into(),
            ));
        }
        Ok(parts[2].trim().to_string())
    }

    pub fn extract_snippets_from_body(body: &str) -> Vec<(String, String)> {
        let mut snippets = Vec::new();
        let mut current_heading = String::new();
        let mut current_content = String::new();

        for line in body.lines() {
            if line.starts_with("## ") || line.starts_with("# ") {
                if !current_heading.is_empty() && !current_content.is_empty() {
                    snippets.push((
                        current_heading.clone(),
                        current_content.trim().to_string(),
                    ));
                }
                current_heading =
                    line.trim_start_matches('#').trim().to_string();
                current_content.clear();
            } else {
                current_content.push_str(line);
                current_content.push('\n');
            }
        }

        if !current_heading.is_empty() && !current_content.is_empty() {
            snippets
                .push((current_heading, current_content.trim().to_string()));
        }

        snippets
    }

    pub fn validate_optional_fields(metadata: &SkillMetadata) -> Vec<String> {
        let mut warnings = Vec::new();
        if metadata.triggers.is_empty() {
            warnings.push(
                "No triggers defined, skill will only match via BM25"
                    .to_string(),
            );
        }
        if metadata.tags.is_empty() {
            warnings.push(
                "No tags defined, skill will not be categorizable".to_string(),
            );
        }
        if !metadata.depends_on.is_empty() {
            warnings.push(format!(
                "Skill depends on: {}",
                metadata.depends_on.join(", ")
            ));
        }
        if !metadata.conflicts_with.is_empty() {
            warnings.push(format!(
                "Skill conflicts with: {}",
                metadata.conflicts_with.join(", ")
            ));
        }
        warnings
    }

    pub fn compute_file_hash(path: &Path) -> Result<String, Error> {
        let content = std::fs::read(path)?;
        let mut hasher = Sha256::new();
        hasher.update(&content);
        Ok(hex::encode(hasher.finalize()))
    }

    pub fn compute_content_hash(content: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(content);
        hex::encode(hasher.finalize())
    }

    pub fn load_skill_directory(path: &Path) -> Result<SkillHashMap, Error> {
        let mut file_hashes = HashMap::new();
        let skill_md = path.join("SKILL.md");

        if skill_md.exists() {
            let hash = Self::compute_file_hash(&skill_md)?;
            file_hashes.insert("SKILL.md".to_string(), hash);
        }

        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.flatten() {
                let entry_path = entry.path();
                if entry_path.is_file()
                    && entry_path.file_name().and_then(|n| n.to_str())
                        != Some("SKILL.md")
                    && let Some(name) =
                        entry_path.file_name().and_then(|n| n.to_str())
                    && !name.starts_with('.')
                {
                    let hash = Self::compute_file_hash(&entry_path)?;
                    file_hashes.insert(name.to_string(), hash);
                }
            }
        }

        Ok(SkillHashMap { files: file_hashes })
    }

    pub fn has_changed(
        path: &Path,
        previous: &SkillHashMap,
    ) -> Result<bool, Error> {
        let current = Self::load_skill_directory(path)?;
        Ok(current.files != previous.files)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SkillHashMap {
    files: HashMap<String, String>,
}

impl SkillHashMap {
    pub fn new() -> Self {
        Self {
            files: HashMap::new(),
        }
    }

    pub fn get(&self, file: &str) -> Option<&String> {
        self.files.get(file)
    }

    pub fn files(&self) -> &HashMap<String, String> {
        &self.files
    }
}
