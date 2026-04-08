//! Builtin skills module
//!
//! This module provides builtin skills that are embedded in the binary.
//! Skills can be single-file (just SKILL.md) or multi-file with supporting
//! resources in scripts/, references/, and assets/ directories.
//!
//! On startup, builtin skills are written to the filesystem at `.agents/skills/`
//! and loaded from there, providing a unified loading path for all skills.

use std::path::Path;

use crate::Result;

/// An embedded file with its relative path and content.
#[derive(Debug, Clone, Copy)]
pub struct EmbeddedFile {
    pub path: &'static str,
    pub content: &'static str,
}

/// An embedded skill with its main SKILL.md and optional supporting files.
#[derive(Debug, Clone)]
pub struct EmbeddedSkill {
    pub name: &'static str,
    pub skill_md: &'static str,
    pub scripts: &'static [EmbeddedFile],
    pub references: &'static [EmbeddedFile],
    pub assets: &'static [EmbeddedFile],
}

impl EmbeddedSkill {
    pub const fn new(name: &'static str, skill_md: &'static str) -> Self {
        Self {
            name,
            skill_md,
            scripts: &[],
            references: &[],
            assets: &[],
        }
    }

    pub const fn with_scripts(
        mut self,
        scripts: &'static [EmbeddedFile],
    ) -> Self {
        self.scripts = scripts;
        self
    }

    pub const fn with_references(
        mut self,
        references: &'static [EmbeddedFile],
    ) -> Self {
        self.references = references;
        self
    }

    pub const fn with_assets(
        mut self,
        assets: &'static [EmbeddedFile],
    ) -> Self {
        self.assets = assets;
        self
    }

    pub fn all_files(&self) -> Vec<EmbeddedFile> {
        let mut files = Vec::new();
        files.extend_from_slice(self.scripts);
        files.extend_from_slice(self.references);
        files.extend_from_slice(self.assets);
        files
    }

    pub fn has_supporting_files(&self) -> bool {
        !self.scripts.is_empty()
            || !self.references.is_empty()
            || !self.assets.is_empty()
    }

    pub fn write_to_filesystem(&self, base_dir: &Path) -> Result<()> {
        let skill_dir = base_dir.join(self.name);
        std::fs::create_dir_all(&skill_dir)?;
        std::fs::write(skill_dir.join("SKILL.md"), self.skill_md)?;

        for file in self
            .scripts
            .iter()
            .chain(self.references.iter())
            .chain(self.assets.iter())
        {
            let path = skill_dir.join(file.path);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&path, file.content)?;
        }

        Ok(())
    }
}

const SKILL_CREATOR: &str = include_str!("skills/skill-creator.md");
const FIND_SKILLS: &str = include_str!("skills/find-skills.md");
const SELF_IMPROVEMENT: &str = include_str!("skills/self-improvement/SKILL.md");
const RALPH_SKILL: &str = include_str!("skills/ralph/SKILL.md");
const PRD_SKILL: &str = include_str!("skills/prd/SKILL.md");

const RALPH_SCRIPTS: &[EmbeddedFile] = &[EmbeddedFile {
    path: "scripts/ralph.sh",
    content: include_str!("skills/ralph/scripts/ralph.sh"),
}];

const RALPH_REFERENCES: &[EmbeddedFile] = &[
    EmbeddedFile {
        path: "references/prompt.md",
        content: include_str!("skills/ralph/references/prompt.md"),
    },
    EmbeddedFile {
        path: "references/CLAUDE.md",
        content: include_str!("skills/ralph/references/CLAUDE.md"),
    },
    EmbeddedFile {
        path: "references/AGENTS.md",
        content: include_str!("skills/ralph/references/AGENTS.md"),
    },
];

const RALPH_ASSETS: &[EmbeddedFile] = &[EmbeddedFile {
    path: "assets/prd.json.example",
    content: include_str!("skills/ralph/assets/prd.json.example"),
}];

const SELF_IMPROVEMENT_SCRIPTS: &[EmbeddedFile] = &[
    EmbeddedFile {
        path: "scripts/activator.sh",
        content: include_str!("skills/self-improvement/scripts/activator.sh"),
    },
    EmbeddedFile {
        path: "scripts/error-detector.sh",
        content: include_str!(
            "skills/self-improvement/scripts/error-detector.sh"
        ),
    },
    EmbeddedFile {
        path: "scripts/extract-skill.sh",
        content: include_str!(
            "skills/self-improvement/scripts/extract-skill.sh"
        ),
    },
];

const SELF_IMPROVEMENT_REFERENCES: &[EmbeddedFile] = &[
    EmbeddedFile {
        path: "references/openclaw-integration.md",
        content: include_str!(
            "skills/self-improvement/references/openclaw-integration.md"
        ),
    },
    EmbeddedFile {
        path: "references/promotion-guide.md",
        content: include_str!(
            "skills/self-improvement/references/promotion-guide.md"
        ),
    },
    EmbeddedFile {
        path: "references/hooks-setup.md",
        content: include_str!(
            "skills/self-improvement/references/hooks-setup.md"
        ),
    },
    EmbeddedFile {
        path: "references/detection-triggers.md",
        content: include_str!(
            "skills/self-improvement/references/detection-triggers.md"
        ),
    },
    EmbeddedFile {
        path: "references/logging-format.md",
        content: include_str!(
            "skills/self-improvement/references/logging-format.md"
        ),
    },
    EmbeddedFile {
        path: "references/examples.md",
        content: include_str!("skills/self-improvement/references/examples.md"),
    },
];

const SELF_IMPROVEMENT_ASSETS: &[EmbeddedFile] = &[
    EmbeddedFile {
        path: "assets/LEARNINGS.md",
        content: include_str!("skills/self-improvement/assets/LEARNINGS.md"),
    },
    EmbeddedFile {
        path: "assets/SKILL-TEMPLATE.md",
        content: include_str!(
            "skills/self-improvement/assets/SKILL-TEMPLATE.md"
        ),
    },
];

const BUILTIN_SKILLS: &[EmbeddedSkill] = &[
    EmbeddedSkill::new("skill-creator", SKILL_CREATOR),
    EmbeddedSkill::new("find-skills", FIND_SKILLS),
    EmbeddedSkill::new("self-improvement", SELF_IMPROVEMENT)
        .with_scripts(SELF_IMPROVEMENT_SCRIPTS)
        .with_references(SELF_IMPROVEMENT_REFERENCES)
        .with_assets(SELF_IMPROVEMENT_ASSETS),
    EmbeddedSkill::new("ralph", RALPH_SKILL)
        .with_scripts(RALPH_SCRIPTS)
        .with_references(RALPH_REFERENCES)
        .with_assets(RALPH_ASSETS),
    EmbeddedSkill::new("prd", PRD_SKILL),
];

/// Writes all builtin skills to the filesystem.
pub(super) fn write_builtin_skills_to_filesystem(
    working_dir: &Path,
) -> Result<()> {
    let skills_dir = working_dir.join(".agents/skills");
    std::fs::create_dir_all(&skills_dir)?;

    for skill in BUILTIN_SKILLS {
        skill.write_to_filesystem(&skills_dir)?;
    }

    Ok(())
}

/// Returns all builtin skills (for testing purposes).
#[cfg(test)]
pub(super) fn get_all_builtin_skills() -> &'static [EmbeddedSkill] {
    BUILTIN_SKILLS
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    #[test]
    fn test_builtin_skills_not_empty() {
        assert!(!BUILTIN_SKILLS.is_empty());
    }

    #[test]
    fn test_builtin_skills_have_valid_frontmatter() {
        for skill in BUILTIN_SKILLS {
            assert!(skill.skill_md.contains("---"));
            assert!(skill.skill_md.contains("name:"));
            assert!(skill.skill_md.contains("description:"));
        }
    }

    #[test]
    fn test_ralph_skill_has_supporting_files() {
        let ralph = BUILTIN_SKILLS.iter().find(|s| s.name == "ralph").unwrap();
        assert!(ralph.has_supporting_files());
        assert!(!ralph.scripts.is_empty());
        assert!(!ralph.references.is_empty());
        assert!(!ralph.assets.is_empty());
    }

    #[test]
    fn test_self_improvement_skill_has_supporting_files() {
        let si = BUILTIN_SKILLS
            .iter()
            .find(|s| s.name == "self-improvement")
            .unwrap();
        assert!(si.has_supporting_files());
        assert!(!si.scripts.is_empty());
        assert!(!si.references.is_empty());
        assert!(!si.assets.is_empty());
    }

    #[test]
    fn test_skill_creator_no_supporting_files() {
        let sc = BUILTIN_SKILLS
            .iter()
            .find(|s| s.name == "skill-creator")
            .unwrap();
        assert!(!sc.has_supporting_files());
    }

    #[test]
    fn test_embedded_file_paths() {
        let ralph = BUILTIN_SKILLS.iter().find(|s| s.name == "ralph").unwrap();
        let all_files = ralph.all_files();

        let paths: Vec<&str> = all_files.iter().map(|f| f.path).collect();
        assert!(paths.contains(&"scripts/ralph.sh"));
        assert!(paths.contains(&"references/prompt.md"));
        assert!(paths.contains(&"assets/prd.json.example"));
    }

    #[test]
    fn test_write_to_filesystem() {
        let temp_dir = TempDir::new().unwrap();
        let skills_dir = temp_dir.path().join(".agents/skills");

        let ralph = BUILTIN_SKILLS.iter().find(|s| s.name == "ralph").unwrap();
        ralph.write_to_filesystem(&skills_dir).unwrap();

        let ralph_dir = skills_dir.join("ralph");
        assert!(ralph_dir.join("SKILL.md").exists());
        assert!(ralph_dir.join("scripts/ralph.sh").exists());
        assert!(ralph_dir.join("references/prompt.md").exists());
        assert!(ralph_dir.join("assets/prd.json.example").exists());
    }

    #[test]
    fn test_write_builtin_skills_to_filesystem() {
        let temp_dir = TempDir::new().unwrap();
        write_builtin_skills_to_filesystem(temp_dir.path()).unwrap();

        let skills_dir = temp_dir.path().join(".agents/skills");
        assert!(skills_dir.exists());

        for skill in BUILTIN_SKILLS {
            let skill_dir = skills_dir.join(skill.name);
            assert!(skill_dir.join("SKILL.md").exists());
        }
    }
}
