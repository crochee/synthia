//! `validate` subcommand — check a `SKILL.md` for
//! frontmatter + body correctness.
//!
//! Two functions live here:
//!
//! - [`validate_skill`]: the public entry point. Takes
//!   a `&Path` to a `SKILL.md` (or any file) and a
//!   `--json` flag. Returns `Ok(())` even on validation
//!   failure — the failure is **printed**, not raised
//!   (so scripts can pipe the JSON output and check
//!   `valid` themselves).
//! - [`output_validate_result`]: the shared renderer
//!   used by [`validate_skill`]. Kept `pub(super)` so
//!   only this module can call it.

use std::path::Path;

use anyhow::{Result, bail};
use synthia_skill::{loader::SkillLoader, types::SkillMetadata};

/// Validate a `SKILL.md` file. Never returns `Err` for
/// a validation failure — it prints the result and
/// returns `Ok(())` so the CLI exit code reflects the
/// user choice (`--json` consumers should check the
/// `valid` field in the JSON output).
pub fn validate_skill(path: &Path, as_json: bool) -> Result<()> {
    if !path.exists() {
        if as_json {
            let result = serde_json::json!({
                "valid": false,
                "path": path.to_string_lossy(),
                "errors": ["File not found"],
                "warnings": []
            });
            println!("{}", serde_json::to_string_pretty(&result)?);
        } else {
            bail!("File not found: {}", path.display());
        }
        return Ok(());
    }

    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    // Validate frontmatter (required fields)
    let metadata = match SkillLoader::parse_frontmatter(path) {
        Ok(m) => m,
        Err(e) => {
            errors.push(format!("Frontmatter parse error: {}", e));
            output_validate_result(as_json, path, None, &errors, &warnings)?;
            return Ok(());
        }
    };

    // Validate body
    let body = SkillLoader::parse_body(path);
    if let Err(e) = &body {
        errors.push(format!("Body parse error: {}", e));
    }

    // Check optional field warnings
    warnings.extend(SkillLoader::validate_optional_fields(&metadata));

    // Additional validation: check body is not empty
    if body.as_ref().map(|b| b.trim().is_empty()).unwrap_or(true) {
        warnings.push("SKILL.md body is empty".to_string());
    }

    output_validate_result(as_json, path, Some(&metadata), &errors, &warnings)
}

/// Render the validation result. JSON mode emits a
/// single JSON object; human mode emits a short
/// success / failure block with optional warnings.
fn output_validate_result(
    as_json: bool,
    path: &Path,
    metadata: Option<&SkillMetadata>,
    errors: &[String],
    warnings: &[String],
) -> Result<()> {
    if as_json {
        let result = serde_json::json!({
            "valid": errors.is_empty(),
            "path": path.to_string_lossy(),
            "name": metadata.map(|m| &m.name),
            "description": metadata.map(|m| &m.description),
            "errors": errors,
            "warnings": warnings
        });
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else if errors.is_empty() {
        if let Some(m) = metadata {
            println!("Valid SKILL.md: {} ({})", m.name, m.description);
        }
        if !warnings.is_empty() {
            println!("Warnings:");
            for w in warnings {
                println!("  - {}", w);
            }
        }
    } else {
        println!("Validation FAILED:");
        for e in errors {
            println!("  ERROR: {}", e);
        }
        if !warnings.is_empty() {
            println!("Warnings:");
            for w in warnings {
                println!("  - {}", w);
            }
        }
    }

    Ok(())
}
