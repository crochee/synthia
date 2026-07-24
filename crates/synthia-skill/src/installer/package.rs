//! Archive-level helpers for the installer family.
//!
//! Three free functions live here, all sharing the
//! "read something out of the `.skill` ZIP without
//! installing it" theme:
//!
//! - [`compute_sha256`]: the SHA-256 hex digest of
//!   arbitrary bytes. Used both for the top-level
//!   archive hash (the caller passes
//!   `expected_hash`) and for the per-file
//!   verification in [`verify_file_hashes`].
//! - [`find_skill_name_from_bytes`]: scans the
//!   archive for a `SKILL.md` entry, parses its
//!   frontmatter, and returns the skill name. Falls
//!   back to the directory name containing the
//!   `SKILL.md` if the frontmatter is missing or
//!   unparseable.
//! - [`verify_file_hashes`](verify_file_hashes): post-extraction
//!   manifest check. Walks every
//!   `(path, sha256)` entry in
//!   [`super::types::PackageManifest`], reads the
//!   corresponding file off disk, and fails with
//!   `Error::Validation` if the on-disk content
//!   doesn't match.
//!
//! Kept separate from [`super::installer`] (which
//! orchestrates the install pipeline) and
//! [`super::path_utils`] (which handles the
//! path-level security checks) so a reader can
//! reason about "what does the archive check" in
//! one place.

use std::{fs, io::Read, path::Path};

use sha2::{Digest, Sha256};
use synthia_core::Error;

use super::types::PackageManifest;

/// Compute SHA-256 hash of bytes and return hex
/// string.
pub(super) fn compute_sha256(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}

/// Find the skill name by locating SKILL.md in the
/// archive. Returns the name from frontmatter if
/// found, otherwise uses the top-level directory
/// name or archive filename.
pub(super) fn find_skill_name_from_bytes(data: &[u8]) -> Result<String, Error> {
    let cursor = std::io::Cursor::new(data);
    let mut archive = zip::ZipArchive::new(cursor)
        .map_err(|e| Error::Parse(format!("zip error: {}", e)))?;

    // Look for SKILL.md at any level
    for i in 0..archive.len() {
        // First, check the filename
        let file = archive
            .by_index(i)
            .map_err(|e| Error::Parse(format!("zip error: {}", e)))?;
        let name = file
            .enclosed_name()
            .ok_or_else(|| {
                Error::Validation("path traversal detected in package".into())
            })?
            .to_path_buf();
        drop(file);

        if name.ends_with("SKILL.md") {
            // Now read the content
            let mut file = archive
                .by_index(i)
                .map_err(|e| Error::Parse(format!("zip error: {}", e)))?;
            let mut content = String::new();
            file.read_to_string(&mut content)?;

            let parts: Vec<&str> = content.splitn(3, "---").collect();
            if parts.len() >= 3 {
                let frontmatter = parts[1].trim();
                if let Ok(metadata) = serde_yaml::from_str::<
                    crate::types::SkillMetadata,
                >(frontmatter)
                {
                    return Ok(metadata.name);
                }
            }

            // Fallback: use directory containing SKILL.md
            if let Some(dir_name) = name.parent().and_then(|p| p.file_name())
                && let Some(name) = dir_name.to_str()
            {
                return Ok(name.to_string());
            }
        }
    }

    Err(Error::Skill("no SKILL.md found in archive".into()))
}

/// Post-extraction manifest hash check. Walks
/// every `(relative_path, sha256)` pair in
/// `manifest`, reads the corresponding file off
/// disk under `skill_dir`, and fails with
/// `Error::Validation` if the on-disk content
/// doesn't match.
pub(super) fn verify_file_hashes(
    skill_dir: &Path,
    manifest: &PackageManifest,
) -> Result<(), Error> {
    for (relative_path, expected_hash) in &manifest.sha256 {
        let file_path = skill_dir.join(relative_path);
        if !file_path.exists() {
            return Err(Error::Skill(format!(
                "manifest references missing file: {}",
                relative_path
            )));
        }

        let content = fs::read(&file_path)?;
        let actual_hash = compute_sha256(&content);

        if actual_hash != *expected_hash {
            return Err(Error::Validation(format!(
                "file hash mismatch for '{}': expected {}, got {}",
                relative_path, expected_hash, actual_hash
            )));
        }
    }
    Ok(())
}
