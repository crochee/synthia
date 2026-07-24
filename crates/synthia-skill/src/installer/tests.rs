//! Unit tests for the installer family.
//!
//! All 11 tests for [`super::installer::SkillInstaller`]
//! (8) and the path / hash helpers in
//! [`super::path_utils`] / [`super::package`] (3) live
//! here.
//!
//! The local `create_test_zip` / `make_installer` /
//! `valid_skill_content` builders are centralised
//! because every `install` test needs at least
//! one of each. Without centralisation, the test
//! code would duplicate the 80-line ZIP-building
//! ceremony.

use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::Arc,
};

use tempfile::TempDir;

use super::{
    installer::SkillInstaller,
    package::compute_sha256,
    path_utils::{has_path_traversal, strip_top_level_prefix},
};
use crate::{registry::SkillRegistry, types::SkillPaths};

/// Build a `.skill` archive under
/// `temp_dir/test-skill.skill` containing the
/// `(filename, content)` pairs in `content`.
///
/// All entries are prefixed with `test-skill/`
/// (mimicking a real package built from a
/// `test-skill/` directory).
fn create_test_zip(content: &[(&str, &str)]) -> (PathBuf, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let zip_path = temp_dir.path().join("test-skill.skill");

    // Create a temporary directory structure for the ZIP
    let temp_root = TempDir::new().unwrap();
    let skill_root = temp_root.path().join("test-skill");
    fs::create_dir_all(&skill_root).unwrap();

    for (filename, file_content) in content {
        let file_path = if filename.contains('/') {
            let dir = skill_root.join(
                std::path::Path::new(filename)
                    .parent()
                    .expect("parent should exist"),
            );
            fs::create_dir_all(&dir).unwrap();
            skill_root.join(filename)
        } else {
            skill_root.join(filename)
        };
        fs::write(&file_path, file_content).unwrap();
    }

    // Create ZIP - prefix all entries with "test-skill/"
    let file = fs::File::create(&zip_path).unwrap();
    let mut zip_writer = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default();

    // Track which directories we've already added
    let mut added_dirs = std::collections::HashSet::new();
    added_dirs.insert("test-skill".to_string());

    for (filename, file_content) in content {
        let entry_name = format!("test-skill/{}", filename);

        // Add parent directory entries if not already added
        if filename.contains('/') {
            let mut parent = String::new();
            for part in filename.split('/') {
                if parent.is_empty() {
                    parent = format!("test-skill/{}", part);
                } else {
                    parent = format!("{}/{}", parent, part);
                }
                // Check if this is a directory (not the file itself)
                if !filename.ends_with(&format!("{}/", part))
                    && !parent.ends_with(part)
                {
                    let dir_entry = format!("{}/", parent);
                    if added_dirs.insert(parent.clone()) {
                        zip_writer.add_directory(&dir_entry, options).unwrap();
                    }
                }
            }
            // Add the parent directory of this file
            if let Some(parent_path) = std::path::Path::new(filename).parent() {
                let parent_str =
                    format!("test-skill/{}/", parent_path.to_string_lossy());
                if added_dirs.insert(parent_str.clone()) {
                    zip_writer.add_directory(&parent_str, options).unwrap();
                }
            }
        }

        zip_writer.start_file(&entry_name, options).unwrap();
        zip_writer.write_all(file_content.as_bytes()).unwrap();
    }
    zip_writer.finish().unwrap();

    (zip_path, temp_dir)
}

/// Build a fresh `SkillInstaller` rooted at
/// `<dir>/skills/` with a fresh `SkillRegistry`.
fn make_installer(dir: &TempDir) -> (SkillInstaller, Arc<SkillRegistry>) {
    let skills_dir = dir.path().join("skills");
    fs::create_dir_all(&skills_dir).unwrap();

    let paths = SkillPaths {
        user_dir: skills_dir.clone(),
        project_dir: dir.path().join("project"),
        builtin_dir: dir.path().join("builtin"),
    };

    let registry = Arc::new(SkillRegistry::new(paths));
    let installer = SkillInstaller::new(skills_dir, Arc::clone(&registry));
    (installer, registry)
}

/// Build the canonical `valid_skill_content`
/// fixture: a single `SKILL.md` with the standard
/// frontmatter.
fn valid_skill_content() -> Vec<(&'static str, &'static str)> {
    vec![(
        "SKILL.md",
        "---
name: test-skill
description: A test skill for installation
triggers:
  - test
version: \"1.0.0\"
tags:
  - test
---
This is the skill body content.",
    )]
}

#[test]
fn test_install_valid_zip() {
    let temp = TempDir::new().unwrap();
    let (zip_path, _zip_temp) = create_test_zip(&valid_skill_content());
    let (installer, _registry) = make_installer(&temp);

    let result = installer.install(&zip_path, None);
    assert!(result.is_ok(), "Install should succeed: {:?}", result);
    assert_eq!(result.unwrap(), "test-skill");
    assert!(temp.path().join("skills/test-skill/SKILL.md").exists());
}

#[test]
fn test_install_with_hash_verification() {
    let temp = TempDir::new().unwrap();
    let (zip_path, _zip_temp) = create_test_zip(&valid_skill_content());

    // Compute correct hash
    let bytes = fs::read(&zip_path).unwrap();
    let correct_hash = compute_sha256(&bytes);

    let (installer, _registry) = make_installer(&temp);

    // Should succeed with correct hash
    let result = installer.install(&zip_path, Some(&correct_hash));
    assert!(result.is_ok(), "Install with correct hash should succeed");

    // Should fail with wrong hash
    let temp2 = TempDir::new().unwrap();
    let (installer2, _registry2) = make_installer(&temp2);
    let result = installer2.install(&zip_path, Some("wrong_hash_value"));
    assert!(result.is_err(), "Install with wrong hash should fail");
    match result.unwrap_err() {
        synthia_core::Error::Validation(msg)
            if msg.contains("SHA-256 mismatch") => {}
        other => panic!(
            "Expected Error::Validation with hash mismatch, got: {:?}",
            other
        ),
    }
}

#[test]
fn test_install_duplicate_fails() {
    let temp = TempDir::new().unwrap();
    let (zip_path, _zip_temp) = create_test_zip(&valid_skill_content());
    let (installer, _registry) = make_installer(&temp);

    installer.install(&zip_path, None).unwrap();

    let result = installer.install(&zip_path, None);
    assert!(result.is_err());
    match result.unwrap_err() {
        synthia_core::Error::AlreadyExists(name) => {
            assert_eq!(name, "test-skill");
        }
        other => panic!("Expected Error::AlreadyExists, got: {:?}", other),
    }
}

#[test]
fn test_uninstall_success() {
    let temp = TempDir::new().unwrap();
    let (zip_path, _zip_temp) = create_test_zip(&valid_skill_content());
    let (installer, _registry) = make_installer(&temp);

    installer.install(&zip_path, None).unwrap();
    assert!(temp.path().join("skills/test-skill").exists());

    installer.uninstall("test-skill").unwrap();
    assert!(!temp.path().join("skills/test-skill").exists());
}

#[test]
fn test_uninstall_not_found() {
    let temp = TempDir::new().unwrap();
    let (installer, _registry) = make_installer(&temp);

    let result = installer.uninstall("nonexistent");
    assert!(result.is_err());
    match result.unwrap_err() {
        synthia_core::Error::NotFound(name) => {
            assert_eq!(name, "nonexistent");
        }
        other => panic!("Expected Error::NotFound, got: {:?}", other),
    }
}

#[test]
fn test_list_installed() {
    let temp = TempDir::new().unwrap();
    let (zip_path, _zip_temp) = create_test_zip(&valid_skill_content());
    let (installer, _registry) = make_installer(&temp);

    // Initially empty
    let list = installer.list_installed().unwrap();
    assert!(list.is_empty());

    // Install and check listing
    installer.install(&zip_path, None).unwrap();
    let list = installer.list_installed().unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].name, "test-skill");
    assert_eq!(list[0].version.as_deref(), Some("1.0.0"));
    assert_eq!(list[0].description, "A test skill for installation");
}

#[test]
fn test_compute_sha256() {
    let hash = compute_sha256(b"hello world");
    // SHA-256 of "hello world"
    assert_eq!(
        hash,
        "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
    );
}

#[test]
fn test_strip_top_level_prefix() {
    let path = Path::new("my-skill/SKILL.md");
    let result = strip_top_level_prefix(path);
    assert_eq!(result, Path::new("SKILL.md"));

    let path = Path::new("my-skill/subdir/file.txt");
    let result = strip_top_level_prefix(path);
    assert_eq!(result, Path::new("subdir/file.txt"));
}

#[test]
fn test_has_path_traversal() {
    assert!(has_path_traversal(Path::new("../etc/passwd")));
    assert!(has_path_traversal(Path::new("foo/../../bar")));
    assert!(!has_path_traversal(Path::new("foo/bar.txt")));
    assert!(!has_path_traversal(Path::new("SKILL.md")));
}

#[test]
fn test_install_with_subdirectories() {
    let temp = TempDir::new().unwrap();
    let content = vec![
        (
            "SKILL.md",
            "---
name: multi-file-skill
description: A skill with multiple files
triggers:
  - test
tags:
  - test
---
Multi-file skill body.",
        ),
        ("helpers/utils.py", "def helper(): pass"),
        ("data/config.json", "{\"key\": \"value\"}"),
    ];
    let (zip_path, _zip_temp) = create_test_zip(&content);

    // Debug: print what's in the zip
    let zip_file = fs::File::open(&zip_path).unwrap();
    let mut archive = zip::ZipArchive::new(zip_file).unwrap();
    for i in 0..archive.len() {
        let file = archive.by_index(i).unwrap();
        println!("ZIP entry {}: {:?}", i, file.enclosed_name());
    }
    drop(archive);

    let (installer, _registry) = make_installer(&temp);

    let result = installer.install(&zip_path, None);
    assert!(result.is_ok(), "Install should succeed: {:?}", result);

    let skill_dir = temp.path().join("skills/multi-file-skill");
    assert!(skill_dir.join("SKILL.md").exists(), "SKILL.md should exist");
    assert!(
        skill_dir.join("helpers/utils.py").exists(),
        "helpers/utils.py should exist"
    );
    assert!(
        skill_dir.join("data/config.json").exists(),
        "data/config.json should exist"
    );
}

#[test]
fn test_install_with_manifest_hash_verification() {
    let temp = TempDir::new().unwrap();

    let skill_content = "---
name: manifest-test-skill
description: A skill with manifest hash verification
triggers:
  - test
version: \"1.0.0\"
tags:
  - test
---
Skill body content.";

    let utils_content = "def helper(): pass";
    let utils_hash = compute_sha256(utils_content.as_bytes());

    let manifest_content = serde_json::json!({
        "version": "1.0.0",
        "author": "test",
        "sha256": {
            "helpers/utils.py": utils_hash
        }
    })
    .to_string();

    let content = vec![
        ("SKILL.md", skill_content),
        ("helpers/utils.py", utils_content),
        ("manifest.json", &manifest_content),
    ];
    let (zip_path, _zip_temp) = create_test_zip(&content);
    let (installer, _registry) = make_installer(&temp);

    let result = installer.install(&zip_path, None);
    assert!(
        result.is_ok(),
        "Install should succeed with valid manifest: {:?}",
        result
    );
}

#[test]
fn test_install_with_invalid_manifest_hash() {
    let temp = TempDir::new().unwrap();

    let skill_content = "---
name: manifest-invalid-skill
description: A skill with invalid manifest hash
triggers:
  - test
version: \"1.0.0\"
tags:
  - test
---
Skill body content.";

    let utils_content = "def helper(): pass";
    let wrong_hash =
        "0000000000000000000000000000000000000000000000000000000000000000";

    let manifest_content = serde_json::json!({
        "version": "1.0.0",
        "author": "test",
        "sha256": {
            "helpers/utils.py": wrong_hash
        }
    })
    .to_string();

    let content = vec![
        ("SKILL.md", skill_content),
        ("helpers/utils.py", utils_content),
        ("manifest.json", &manifest_content),
    ];
    let (zip_path, _zip_temp) = create_test_zip(&content);
    let (installer, _registry) = make_installer(&temp);

    let result = installer.install(&zip_path, None);
    assert!(
        result.is_err(),
        "Install should fail with invalid manifest hash"
    );
    match result.unwrap_err() {
        synthia_core::Error::Validation(msg)
            if msg.contains("file hash mismatch") =>
        {
            assert!(msg.contains("helpers/utils.py"));
        }
        other => panic!(
            "Expected Error::Validation with hash mismatch, got: {:?}",
            other
        ),
    }
}
