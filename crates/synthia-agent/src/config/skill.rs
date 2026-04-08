//! Skill configuration module
//!
//! This module provides configuration for skill loading including
//! skill directories and cache settings.

use std::path::PathBuf;

/// Standard skill directory paths in priority order
pub(crate) const DIRECTORY_PRIORITY: &[&str] = &[
    "~/.claude/skills",
    "~/.config/claude/skills",
    "workspace/.claude/skills",
    "workspace/.skills",
];

/// Configuration for skill loading behavior
#[derive(Debug, Clone)]
pub struct SkillConfig {
    /// Directories to search for skills
    pub directories: Vec<PathBuf>,
    /// Maximum number of skills to keep in cache
    pub cache_size: usize,
    /// Whether to load skills on demand (vs at startup)
    pub load_on_demand: bool,
}

impl Default for SkillConfig {
    fn default() -> Self {
        Self {
            directories: Self::default_directories(),
            cache_size: 32,
            load_on_demand: true,
        }
    }
}

impl SkillConfig {
    /// Get the default skill directories
    pub fn default_directories() -> Vec<PathBuf> {
        DIRECTORY_PRIORITY
            .iter()
            .filter_map(|p| {
                if p.starts_with("workspace") {
                    None
                } else {
                    Some(PathBuf::from(p.replace(
                        '~',
                        &dirs::home_dir()?.display().to_string(),
                    )))
                }
            })
            .collect()
    }

    /// Create a skill config with workspace-specific directories
    pub fn with_workspace(self, workspace: PathBuf) -> Self {
        let mut directories = self.directories;
        directories.push(workspace.join(".claude/skills"));
        directories.push(workspace.join(".skills"));
        Self {
            directories,
            ..self
        }
    }

    /// Get the configured directories
    pub fn get_directories(&self) -> &[PathBuf] {
        &self.directories
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_skill_config_default() {
        let config = SkillConfig::default();
        assert_eq!(config.cache_size, 32);
        assert!(config.load_on_demand);
    }

    #[test]
    fn test_directory_priority_contains_standard_paths() {
        assert!(DIRECTORY_PRIORITY.contains(&"~/.claude/skills"));
        assert!(DIRECTORY_PRIORITY.contains(&"~/.config/claude/skills"));
        assert!(DIRECTORY_PRIORITY.contains(&"workspace/.claude/skills"));
        assert!(DIRECTORY_PRIORITY.contains(&"workspace/.skills"));
    }

    #[test]
    fn test_with_workspace_adds_paths() {
        let config = SkillConfig::default();
        let workspace = PathBuf::from("/test/workspace");
        let config = config.with_workspace(workspace.clone());

        assert!(
            config
                .directories
                .contains(&workspace.join(".claude/skills"))
        );
        assert!(config.directories.contains(&workspace.join(".skills")));
    }
}
