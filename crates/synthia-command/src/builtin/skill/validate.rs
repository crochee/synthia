//! `validate_skill` — check a SKILL.md file against
//! [`SkillLoader::parse_frontmatter`] and the structural
//! invariants encoded in [`SkillLoader::validate_optional_fields`].
//!
//! Independent of the registry; operates on a path.

use std::path::Path;

use synthia_core::Error;
use synthia_skill::loader::SkillLoader;

use super::{
    construct::SkillCommand,
    format::format_validate_output,
    types::ValidateResult,
};
use crate::types::CommandResult;

impl SkillCommand {
    pub(super) fn validate_skill(
        &self,
        path: &str,
    ) -> Result<CommandResult, Error> {
        let skill_path = Path::new(path);
        if !skill_path.exists() {
            let result = ValidateResult {
                path: path.to_string(),
                valid: false,
                errors: vec![format!("File not found: {}", path)],
                warnings: vec![],
            };
            return Ok(CommandResult::new(format_validate_output(
                &result, false,
            )));
        }

        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        match SkillLoader::parse_frontmatter(skill_path) {
            Ok(metadata) => {
                warnings = SkillLoader::validate_optional_fields(&metadata);

                let dir_name = skill_path
                    .parent()
                    .and_then(|p| p.file_name())
                    .and_then(|n| n.to_str());

                if let Some(dir) = dir_name
                    && metadata.name != dir
                {
                    errors.push(format!(
                        "Skill name '{}' does not match directory name '{}'",
                        metadata.name, dir
                    ));
                }

                if metadata.triggers.is_empty() {
                    warnings.push(
                        "No triggers defined, skill will only match via BM25"
                            .to_string(),
                    );
                }
                if metadata.tags.is_empty() {
                    warnings.push(
                        "No tags defined, skill will not be categorizable"
                            .to_string(),
                    );
                }
                if metadata.version.is_none() {
                    warnings.push("No version specified".to_string());
                }
            }
            Err(e) => {
                errors.push(format!("Failed to parse SKILL.md: {}", e));
            }
        }

        let result = ValidateResult {
            path: path.to_string(),
            valid: errors.is_empty(),
            errors,
            warnings,
        };

        Ok(CommandResult::new(format_validate_output(&result, false)))
    }
}
