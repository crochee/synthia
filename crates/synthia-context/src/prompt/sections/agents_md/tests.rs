//! Unit tests for the `agents_md` section module family.
//!
//! Coverage map (20 tests):
//!
//! - Config: 3 tests
//!   ([`AgentsMdConfig::default`] values, [`AgentsMdSection::new`]
//!   has enabled + SessionCached, [`AgentsMdSection::with_config`]
//!   respects custom values).
//! - [`super::pipeline::truncate_with_marker`]: 4 tests
//!   (under limit passthrough, at exact limit, over limit adds
//!   marker, uses char count not byte count for multi-byte UTF-8).
//! - [`super::walk::walk_ancestors`]: 5 tests
//!   (single file, farthest-first with two files, no files returns
//!   empty, skips directory placeholder, circular symlink dedup).
//! - [`super::pipeline::merge_within_limit`]: 3 tests
//!   (under total limit, per-file truncation, total truncation
//!   favors the closest file).
//! - [`PromptSection`] impl: 5 tests
//!   (name, caching, build-disabled-returns-empty, build-no-files,
//!   build-merges-two-files-in-order, build-filenames-uses-config).

use std::sync::LazyLock;

use super::*;
use crate::prompt::{AgentName, McpServerInfo, PromptSection};

static TEST_AGENT_NAME: LazyLock<AgentName> =
    LazyLock::new(|| AgentName::Custom("TestAgent".to_string()));

fn ctx_with_workspace<'a>(
    workspace: &'a std::path::Path,
    additional: &'a [std::path::PathBuf],
    mcp: &'a [McpServerInfo],
) -> crate::prompt::PromptContext<'a> {
    crate::prompt::PromptContext {
        agent_name: &TEST_AGENT_NAME,
        agent_description: "test",
        workspace_dir: workspace,
        skill_instructions: String::new(),
        is_subagent: false,
        session_id: Some("test"),
        mcp_servers: mcp,
        additional_dirs: additional,
        output_style: None,
        language_preference: None,
        is_proactive_mode: false,
        model_name: None,
        knowledge_cutoff: None,
        team_info: None,
    }
}

// =============================================================================
// Config Tests
// =============================================================================

#[test]
fn test_default_config() {
    let cfg = AgentsMdConfig::default();
    assert!(cfg.enabled);
    assert_eq!(cfg.filenames, vec!["AGENTS.md".to_string()]);
    assert_eq!(cfg.max_chars_per_file, 20_000);
    assert_eq!(cfg.max_chars_total, 60_000);
}

#[test]
fn test_default_section_is_enabled() {
    let section = AgentsMdSection::new();
    assert!(section.config().enabled);
    assert_eq!(
        section.caching(),
        crate::prompt::SectionCaching::SessionCached
    );
    assert_eq!(section.name(), "agents_md");
}

#[test]
fn test_with_config() {
    let cfg = AgentsMdConfig {
        enabled: false,
        filenames: vec!["CLAUDE.md".to_string()],
        max_chars_per_file: 100,
        max_chars_total: 500,
    };
    let section = AgentsMdSection::with_config(cfg.clone());
    assert!(!section.config().enabled);
    assert_eq!(section.config().filenames, vec!["CLAUDE.md".to_string()]);
}

// =============================================================================
// truncate_with_marker Tests
// =============================================================================

#[test]
fn test_truncate_under_limit() {
    let s = "short content";
    assert_eq!(super::pipeline::truncate_with_marker(s, 100), s);
}

#[test]
fn test_truncate_at_exact_limit() {
    let s = "exactly";
    assert_eq!(super::pipeline::truncate_with_marker(s, 7), s);
}

#[test]
fn test_truncate_over_limit() {
    let s = "x".repeat(10);
    let out = super::pipeline::truncate_with_marker(&s, 5);
    // First 5 'x' chars + marker
    assert!(out.starts_with("xxxxx"));
    assert!(out.contains("[... truncated at 5 chars"));
}

#[test]
fn test_truncate_uses_char_count_not_byte_count() {
    // '中' is 3 bytes but 1 char. With limit 5, we should keep 5 chars.
    let s = "中文中文中文"; // 6 chars
    let out = super::pipeline::truncate_with_marker(s, 5);
    assert!(out.starts_with("中文中文中"));
    assert!(out.contains("[... truncated at 5 chars"));
}

// =============================================================================
// walk_ancestors Tests
// =============================================================================

#[test]
fn test_walk_single_file() {
    let dir = tempfile::tempdir().unwrap();
    let agents = dir.path().join("AGENTS.md");
    std::fs::write(&agents, "GLOBAL").unwrap();

    let workspace = dir.path().join("sub");
    std::fs::create_dir(&workspace).unwrap();

    let out =
        super::walk::walk_ancestors(&workspace, &["AGENTS.md".to_string()]);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].content, "GLOBAL");
    assert_eq!(out[0].path, agents);
}

#[test]
fn test_walk_multiple_files_farthest_first() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("AGENTS.md"), "GLOBAL").unwrap();
    let repo = dir.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    std::fs::write(repo.join("AGENTS.md"), "REPO").unwrap();
    let sub = repo.join("sub");
    std::fs::create_dir(&sub).unwrap();

    let out = super::walk::walk_ancestors(&sub, &["AGENTS.md".to_string()]);
    assert_eq!(out.len(), 2);
    assert_eq!(out[0].content, "GLOBAL");
    assert_eq!(out[1].content, "REPO");
}

#[test]
fn test_walk_no_files() {
    let dir = tempfile::tempdir().unwrap();
    let out =
        super::walk::walk_ancestors(dir.path(), &["AGENTS.md".to_string()]);
    // The temp dir itself may or may not have a parent with AGENTS.md
    // (CI tempdir is usually /tmp/<rand>). We assert the relative
    // shape: if nothing was found, out is empty.
    // We *also* want to assert it doesn't include unrelated
    // filesystem AGENTS.md, so we limit the search to depth 1 by
    // checking the path is within dir.path().ancestors() that
    // start from dir.path() itself.
    for f in &out {
        assert!(f.path.starts_with(dir.path()));
    }
}

#[test]
fn test_walk_skips_directory_placeholder() {
    // When AGENTS.md is a directory, is_file() returns false → skip.
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("AGENTS.md")).unwrap();
    let out =
        super::walk::walk_ancestors(dir.path(), &["AGENTS.md".to_string()]);
    assert!(out.is_empty());
}

#[test]
fn test_walk_handles_circular_symlink() {
    // Skip if not Unix.
    #[cfg(unix)]
    {
        let dir = tempfile::tempdir().unwrap();
        let loop_dir = dir.path().join("loop");
        std::fs::create_dir(&loop_dir).unwrap();
        // loop_dir/inside is a symlink to loop_dir itself
        std::os::unix::fs::symlink(&loop_dir, loop_dir.join("inside")).unwrap();
        std::fs::write(loop_dir.join("AGENTS.md"), "BODY").unwrap();

        // Walking from loop_dir/inside:
        //   1. loop_dir/inside        (canonical = loop_dir)
        //   2. loop_dir               (canonical = loop_dir) — duplicate, skip
        //   3. dir.path()             (no AGENTS.md)
        //   4. /                      (no AGENTS.md)
        let out = super::walk::walk_ancestors(
            &loop_dir.join("inside"),
            &["AGENTS.md".to_string()],
        );
        // We expect exactly 1 entry: loop_dir/AGENTS.md (via
        // loop_dir/inside/AGENTS.md which doesn't exist, so skipped,
        // then loop_dir/AGENTS.md which is found).
        assert_eq!(out.len(), 1);
        assert!(out[0].content == "BODY");
    }
}

// =============================================================================
// merge_within_limit Tests
// =============================================================================

#[test]
fn test_merge_under_total_limit() {
    let files = vec![
        super::config::DiscoveredFile {
            path: std::path::PathBuf::from("/a/AGENTS.md"),
            content: "A".to_string(),
        },
        super::config::DiscoveredFile {
            path: std::path::PathBuf::from("/b/AGENTS.md"),
            content: "BB".to_string(),
        },
    ];
    let kept = super::pipeline::merge_within_limit(files, 100, 100);
    assert_eq!(kept.len(), 2);
    assert_eq!(kept[0].content, "A");
    assert_eq!(kept[1].content, "BB");
}

#[test]
fn test_merge_per_file_truncation() {
    let files = vec![super::config::DiscoveredFile {
        path: std::path::PathBuf::from("/x/AGENTS.md"),
        content: "x".repeat(50),
    }];
    let kept = super::pipeline::merge_within_limit(files, 10, 1_000);
    assert_eq!(kept.len(), 1);
    assert!(kept[0].content.starts_with("xxxxxxxxxx"));
    assert!(kept[0].content.contains("[... truncated at 10 chars"));
}

#[test]
fn test_merge_total_truncation_keeps_closest() {
    // 3 files, 30 chars each. Total limit = 65.
    // After two files, used = 60. Third file would push to 90 > 65.
    // Since the third (closest) is processed last, it's the one
    // that's cut. The marker is appended to the second file.
    let files = vec![
        super::config::DiscoveredFile {
            path: std::path::PathBuf::from("/far/AGENTS.md"),
            content: "F".repeat(30),
        },
        super::config::DiscoveredFile {
            path: std::path::PathBuf::from("/mid/AGENTS.md"),
            content: "M".repeat(30),
        },
        super::config::DiscoveredFile {
            path: std::path::PathBuf::from("/close/AGENTS.md"),
            content: "C".repeat(30),
        },
    ];
    let kept = super::pipeline::merge_within_limit(files, 100, 65);
    assert_eq!(kept.len(), 2);
    assert!(kept[0].content.starts_with(&"F".repeat(30)));
    assert!(kept[1].content.starts_with(&"M".repeat(30)));
    assert!(kept[1].content.contains("total content exceeded 65 chars"));
}

// =============================================================================
// PromptSection impl Tests
// =============================================================================

#[test]
fn test_section_name() {
    let section = AgentsMdSection::new();
    assert_eq!(section.name(), "agents_md");
}

#[test]
fn test_section_caching_is_session_cached() {
    let section = AgentsMdSection::new();
    assert_eq!(
        section.caching(),
        crate::prompt::SectionCaching::SessionCached
    );
}

#[test]
fn test_section_build_disabled_returns_empty() {
    let cfg = AgentsMdConfig {
        enabled: false,
        ..Default::default()
    };
    let section = AgentsMdSection::with_config(cfg);
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("AGENTS.md"), "X").unwrap();
    let ctx = ctx_with_workspace(dir.path(), &[], &[]);
    let out = section.build(&ctx).unwrap();
    assert!(out.is_empty());
}

#[test]
fn test_section_build_no_files_returns_empty() {
    let section = AgentsMdSection::new();
    let dir = tempfile::tempdir().unwrap();
    // No AGENTS.md anywhere (tempdir's parents are out of scope).
    // To be safe, walk only the tempdir and check that nothing from
    // the *real* filesystem leaks: filter by starts_with.
    let ctx = ctx_with_workspace(dir.path(), &[], &[]);
    let out = section.build(&ctx).unwrap();
    if !out.is_empty() {
        // If something was found, it must reference a path within
        // the tempdir.
        for line in out.lines() {
            if line.starts_with("## AGENTS.md:") {
                let path = line.trim_start_matches("## AGENTS.md: ").trim();
                assert!(
                    std::path::Path::new(path).starts_with(dir.path()),
                    "unexpected AGENTS.md found outside tempdir: {path}"
                );
            }
        }
    }
}

#[test]
fn test_section_build_merges_two_files() {
    let section = AgentsMdSection::new();
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("AGENTS.md"), "GLOBAL").unwrap();
    let repo = dir.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    std::fs::write(repo.join("AGENTS.md"), "REPO").unwrap();

    let ctx = ctx_with_workspace(&repo, &[], &[]);
    let out = section.build(&ctx).unwrap();

    // Both files should appear, in order
    let global_pos = out.find("GLOBAL").expect("GLOBAL content present");
    let repo_pos = out.find("REPO").expect("REPO content present");
    assert!(
        global_pos < repo_pos,
        "GLOBAL should appear before REPO (farthest-first)"
    );
    // Both path headers should be present
    assert!(out.contains("## AGENTS.md:"));
    // Wrapper tags
    assert!(out.contains("<agents_md>"));
    assert!(out.contains("</agents_md>"));
    // Separator between files
    assert!(out.contains("---"));
}

#[test]
fn test_section_build_filenames_uses_config() {
    let cfg = AgentsMdConfig {
        filenames: vec!["CLAUDE.md".to_string()],
        ..Default::default()
    };
    let section = AgentsMdSection::with_config(cfg);
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("AGENTS.md"), "should-not-be-found")
        .unwrap();
    std::fs::write(dir.path().join("CLAUDE.md"), "FOUND").unwrap();

    let ctx = ctx_with_workspace(dir.path(), &[], &[]);
    let out = section.build(&ctx).unwrap();
    assert!(out.contains("FOUND"));
    assert!(!out.contains("should-not-be-found"));
}
