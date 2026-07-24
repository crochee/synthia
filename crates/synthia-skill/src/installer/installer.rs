//! [`SkillInstaller`] — the install / uninstall /
//! list pipeline.
//!
//! The struct holds the destination `skills_dir`
//! and a clone of the [`SkillRegistry`] used for
//! the post-extract `register_from_path` call.
//!
//! ## `install` flow
//!
//! 1. Read the archive bytes and (optionally)
//!    verify the top-level SHA-256.
//! 2. Scan for a `SKILL.md` entry to determine the
//!    skill name (delegated to
//!    [`super::package::find_skill_name_from_bytes`]).
//! 3. Reject if `<skills_dir>/<name>/` already
//!    exists (returns `Error::AlreadyExists`).
//! 4. Open the ZIP, enforce the
//!    [`super::types::MAX_FILES`] cap, then walk
//!    every entry: strip the top-level prefix,
//!    reject absolute / traversal paths
//!    (delegated to
//!    [`super::path_utils::has_path_traversal`]),
//!    and write the file to disk. Any per-file
//!    failure rolls back the partial install with
//!    `fs::remove_dir_all`.
//! 5. Re-validate the extracted `SKILL.md` via
//!    [`SkillLoader::parse_frontmatter`].
//! 6. If a `manifest.json` is present, verify the
//!    per-file SHA-256 hashes (delegated to
//!    [`super::package::verify_file_hashes`]).
//! 7. Register the skill in the [`SkillRegistry`].
//!
//! ## `uninstall` flow
//!
//! 1. `registry.unregister(name)` — drop the
//!    in-memory state first so a failed disk
//!    cleanup doesn't leave a ghost entry.
//! 2. `fs::remove_dir_all(<skills_dir>/<name>/)`.
//!
//! ## `list_installed` flow
//!
//! Walk `<skills_dir>/`, skip entries that
//! don't have a `SKILL.md`, parse the
//! frontmatter, and emit a
//! [`super::types::InstalledSkill`] per
//! directory. Output is sorted by name.

use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::Arc,
};

use synthia_core::Error;
use tracing;

use super::{
    package::{find_skill_name_from_bytes, verify_file_hashes},
    path_utils::{has_path_traversal, strip_top_level_prefix},
    types::{InstalledSkill, MAX_FILE_SIZE, MAX_FILES, PackageManifest},
};
use crate::{loader::SkillLoader, registry::SkillRegistry};

/// Installer for `.skill` ZIP packages.
pub struct SkillInstaller {
    /// Directory under which installed skills live
    /// (one subdirectory per skill name).
    skills_dir: PathBuf,
    /// Registry the installer pushes
    /// `register_from_path` results into.
    registry: Arc<SkillRegistry>,
}

impl SkillInstaller {
    /// Build a new installer rooted at `skills_dir`
    /// and pushing into `registry`.
    pub fn new(skills_dir: PathBuf, registry: Arc<SkillRegistry>) -> Self {
        Self {
            skills_dir,
            registry,
        }
    }

    /// Install a `.skill` archive from
    /// `archive_path`. See the module-level
    /// rustdoc for the 7-step flow.
    pub fn install(
        &self,
        archive_path: &Path,
        expected_hash: Option<&str>,
    ) -> Result<String, Error> {
        // Read archive bytes
        let archive_bytes = fs::read(archive_path)?;

        // Verify hash if provided
        if let Some(hash) = expected_hash {
            let actual_hash = super::package::compute_sha256(&archive_bytes);
            if actual_hash != hash {
                return Err(Error::Validation(format!(
                    "SHA-256 mismatch: expected {}, got {}",
                    hash, actual_hash
                )));
            }
        }

        // Determine skill name by scanning the archive
        let skill_name = find_skill_name_from_bytes(&archive_bytes)?;
        let skill_dir = self.skills_dir.join(&skill_name);

        // Check if already installed
        if skill_dir.exists() {
            return Err(Error::AlreadyExists(skill_name.clone()));
        }

        // Validate archive file count and extract
        let cursor = std::io::Cursor::new(&archive_bytes);
        let mut archive = zip::ZipArchive::new(cursor)
            .map_err(|e| Error::Parse(format!("zip error: {}", e)))?;
        let file_count = archive.len();
        if file_count > MAX_FILES {
            return Err(Error::Validation(format!(
                "package too large: {} files (max {})",
                file_count, MAX_FILES
            )));
        }

        // Create skill directory
        fs::create_dir_all(&skill_dir)?;

        // Extract files
        let mut extracted = 0;
        for i in 0..file_count {
            let mut file = archive
                .by_index(i)
                .map_err(|e| Error::Parse(format!("zip error: {}", e)))?;
            let file_path = file.enclosed_name().ok_or_else(|| {
                Error::Validation("path traversal detected in package".into())
            })?;

            // Strip common top-level directory prefix (e.g., "my-skill/")
            let relative_path = strip_top_level_prefix(&file_path);

            // Security: reject absolute paths
            if relative_path.is_absolute() {
                return Err(Error::Validation(format!(
                    "absolute path not allowed: {}",
                    relative_path.display()
                )));
            }

            // Security: reject path traversal
            if has_path_traversal(&relative_path) {
                return Err(Error::Validation(
                    "path traversal detected".into(),
                ));
            }

            let target = skill_dir.join(&relative_path);

            if file.is_dir() {
                fs::create_dir_all(&target)?;
            } else {
                // Check file size
                let size = file.size();
                if size > MAX_FILE_SIZE {
                    // Clean up on failure
                    let _ = fs::remove_dir_all(&skill_dir);
                    return Err(Error::Validation(format!(
                        "file too large: {}",
                        relative_path.to_string_lossy()
                    )));
                }

                // Ensure parent directory exists
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent)?;
                }

                let mut content = Vec::new();
                file.read_to_end(&mut content)?;
                fs::write(&target, content)?;
            }

            extracted += 1;
        }

        // Validate SKILL.md exists and has valid frontmatter
        let skill_md = skill_dir.join("SKILL.md");
        if !skill_md.exists() {
            // Clean up on failure
            let _ = fs::remove_dir_all(&skill_dir);
            return Err(Error::Skill("no SKILL.md found in archive".into()));
        }

        let _metadata =
            SkillLoader::parse_frontmatter(&skill_md).map_err(|e| {
                Error::Skill(format!("invalid skill format: {}", e))
            })?;

        // Verify file hashes from manifest.json if present
        if let Ok(manifest) = PackageManifest::load_from_dir(&skill_dir) {
            verify_file_hashes(&skill_dir, &manifest)?;
        }

        // Register the skill
        if let Err(e) = self.registry.register_from_path(&skill_dir) {
            // Clean up on failure
            let _ = fs::remove_dir_all(&skill_dir);
            return Err(Error::Skill(format!("invalid skill format: {}", e)));
        }

        tracing::info!(
            skill = %skill_name,
            extracted = extracted,
            "Skill installed successfully"
        );

        Ok(skill_name)
    }

    /// Uninstall the skill with the given name.
    /// See the module-level rustdoc for the 2-step
    /// flow.
    pub fn uninstall(&self, skill_name: &str) -> Result<(), Error> {
        let skill_dir = self.skills_dir.join(skill_name);

        if !skill_dir.exists() {
            return Err(Error::NotFound(skill_name.to_string()));
        }

        // Remove from registry first
        self.registry.unregister(skill_name);

        // Remove skill directory
        fs::remove_dir_all(&skill_dir)?;

        tracing::info!(skill = %skill_name, "Skill uninstalled");

        Ok(())
    }

    /// Walk `<skills_dir>/` and return one
    /// [`InstalledSkill`] per directory that
    /// contains a parseable `SKILL.md`. Output is
    /// sorted by name.
    pub fn list_installed(&self) -> Result<Vec<InstalledSkill>, Error> {
        let mut installed = Vec::new();

        if !self.skills_dir.exists() {
            return Ok(installed);
        }

        let entries = fs::read_dir(&self.skills_dir)?;

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            let skill_md = path.join("SKILL.md");
            if !skill_md.exists() {
                continue;
            }

            if let Ok(metadata) = SkillLoader::parse_frontmatter(&skill_md) {
                let installed_at = entry
                    .metadata()
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .unwrap_or_else(std::time::SystemTime::now);

                installed.push(InstalledSkill {
                    name: metadata.name,
                    version: metadata.version,
                    description: metadata.description,
                    path,
                    installed_at,
                    archive_hash: None,
                });
            }
        }

        installed.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(installed)
    }
}
