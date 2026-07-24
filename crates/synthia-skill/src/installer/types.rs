//! Data carriers for the installer family.
//!
//! Three things live here, all kept together because
//! they're the "what the installer operates on":
//!
//! - The two [`MAX_FILE_SIZE`]/[`MAX_FILES`] size
//!   caps the installer enforces. Pulled out of
//!   `installer.rs` proper so the limits can be
//!   referenced from [`super::path_utils`] /
//!   [`super::package`] without dragging the whole
//!   `SkillInstaller` struct into scope.
//! - [`PackageManifest`]: the JSON sidecar file
//!   shipped inside `.skill` packages. Maps each
//!   relative file path to the SHA-256 hex the
//!   installer verifies post-extraction. The
//!   `load_from_dir` constructor is the only
//!   constructor — manifests are read from disk,
//!   not built in memory.
//! - [`InstalledSkill`]: a single installed skill
//!   as returned by
//!   [`super::installer::SkillInstaller::list_installed`].
//!   All fields are public so the CLI's
//!   `skill list-installed` consumer can render
//!   them directly.

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};

use serde::{Deserialize, Serialize};

/// Max bytes per file inside a `.skill` archive.
/// Files larger than this are rejected at
/// extraction time and the partial install is
/// rolled back.
pub(crate) const MAX_FILE_SIZE: u64 = 10 * 1024 * 1024; // 10 MB per file

/// Max file count per `.skill` archive. Same
/// rollback-on-exceed semantics as [`MAX_FILE_SIZE`].
pub(crate) const MAX_FILES: usize = 500;

/// Sidecar file shipped inside `.skill` packages.
/// Maps each relative file path to the SHA-256 hex
/// the installer verifies post-extraction.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PackageManifest {
    /// `(relative_path, sha256_hex)` pairs.
    pub sha256: HashMap<String, String>,
    /// Package version. Surfaced in
    /// [`InstalledSkill::version`] when the
    /// `SKILL.md` frontmatter is missing one.
    pub version: String,
    /// Optional human-readable author string. Not
    /// currently surfaced in the CLI; kept on the
    /// struct for future audit-log integration.
    #[serde(default)]
    pub author: Option<String>,
}

impl PackageManifest {
    /// Load `manifest.json` from the given skill
    /// directory. The deserialisation error is
    /// converted into an `io::ErrorKind::InvalidData`
    /// so the installer's `?` propagation gives the
    /// right context.
    pub fn load_from_dir(dir: &Path) -> Result<Self, std::io::Error> {
        let manifest_path = dir.join("manifest.json");
        let content = fs::read_to_string(manifest_path)?;
        serde_json::from_str(&content).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, e)
        })
    }
}

/// One row in the
/// [`super::installer::SkillInstaller::list_installed`]
/// output.
#[derive(Clone, Debug)]
pub struct InstalledSkill {
    /// Skill name from `SKILL.md` frontmatter.
    pub name: String,
    /// Version from `SKILL.md` frontmatter
    /// (optional — older packages don't have one).
    pub version: Option<String>,
    /// Description from `SKILL.md` frontmatter.
    pub description: String,
    /// Absolute path to the skill directory on
    /// disk.
    pub path: PathBuf,
    /// Directory mtime at listing time, or "now"
    /// if the mtime couldn't be read.
    pub installed_at: SystemTime,
    /// Currently always `None` — the installer
    /// doesn't store the source archive's SHA-256
    /// anywhere yet. The field is kept so the CLI
    /// can show "last installed from <hash>" once
    /// that bookkeeping is added.
    pub archive_hash: Option<String>,
}
