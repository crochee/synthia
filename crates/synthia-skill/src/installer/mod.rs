//! Installer for `.skill` ZIP packages.
//!
//! The original 761-line `installer.rs` was split
//! into focused submodules by responsibility:
//!
//! - [`types`]: the data carriers
//!   ([`types::PackageManifest`],
//!   [`types::InstalledSkill`]) and the two
//!   size caps ([`types::MAX_FILE_SIZE`],
//!   [`types::MAX_FILES`]).
//! - [`package`]: the archive-content helpers
//!   ([`package::compute_sha256`],
//!   [`package::find_skill_name_from_bytes`],
//!   [`package::verify_file_hashes`]).
//! - [`path_utils`]: the path-level security
//!   helpers
//!   ([`path_utils::strip_top_level_prefix`],
//!   [`path_utils::has_path_traversal`]).
//! - [`installer`]: the [`installer::SkillInstaller`]
//!   struct + the install / uninstall / list
//!   pipeline.
//!
//! The 11 unit tests live in [`tests`].

#[allow(clippy::module_inception)]
mod installer;
mod package;
mod path_utils;
mod types;

#[allow(clippy::module_inception)]
#[cfg(test)]
mod tests;

pub use installer::SkillInstaller;
