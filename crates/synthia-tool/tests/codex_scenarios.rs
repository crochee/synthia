//! Fixture-based portable test runner for codex V4A apply-patch scenarios.
//!
//! Mirrors `codex-rs/apply-patch/tests/suite/scenarios.rs` — copies each
//! scenario's `input/` directory to a tempdir, runs the V4A patch, and
//! compares the resulting filesystem state to `expected/`.
//!
//! 22 scenarios are vendored from
//! `codex-rs/apply-patch/tests/fixtures/scenarios/`. Codex's README states
//! they are "meant to be easily portable to other languages or platforms";
//! this runner preserves that portability for synthia.
//!
//! The runner uses `ApplyPatchTool { enable_move: true }` because four
//! scenarios (004, 010) require file moves. The default `enable_move = false`
//! is preserved for production use (D2.5 decision); only the test runner
//! opts in.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use synthia_tool::{
    Tool,
    ToolExecutionContext,
    ToolInput,
    ToolOutput,
    builtin::ApplyPatchTool,
};

/// A snapshot entry: either a regular file with content, or a directory.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Entry {
    File(Vec<u8>),
    Dir,
}

/// Build a `BTreeMap<relative_path, Entry>` of everything under `root`.
fn snapshot_dir(root: &Path) -> BTreeMap<PathBuf, Entry> {
    let mut entries = BTreeMap::new();
    if root.is_dir() {
        snapshot_dir_recursive(root, root, &mut entries);
    }
    entries
}

fn snapshot_dir_recursive(
    base: &Path,
    dir: &Path,
    entries: &mut BTreeMap<PathBuf, Entry>,
) {
    let Ok(read_dir) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in read_dir.flatten() {
        let path = entry.path();
        let Ok(stripped) = path.strip_prefix(base) else {
            continue;
        };
        let rel = stripped.to_path_buf();
        let Ok(metadata) = std::fs::metadata(&path) else {
            continue;
        };
        if metadata.is_dir() {
            entries.insert(rel.clone(), Entry::Dir);
            snapshot_dir_recursive(base, &path, entries);
        } else if metadata.is_file()
            && let Ok(contents) = std::fs::read(&path)
        {
            entries.insert(rel, Entry::File(contents));
        }
    }
}

/// Recursively copy `src` into `dst` (creates `dst` if needed).
fn copy_dir_recursive(src: &Path, dst: &Path) {
    let Ok(read_dir) = std::fs::read_dir(src) else {
        return;
    };
    std::fs::create_dir_all(dst).expect("create dst dir");
    for entry in read_dir.flatten() {
        let path = entry.path();
        let dest_path = dst.join(entry.file_name());
        let Ok(metadata) = std::fs::metadata(&path) else {
            continue;
        };
        if metadata.is_dir() {
            copy_dir_recursive(&path, &dest_path);
        } else if metadata.is_file() {
            if let Some(parent) = dest_path.parent() {
                std::fs::create_dir_all(parent).expect("create parent dir");
            }
            std::fs::copy(&path, &dest_path).expect("copy file");
        }
    }
}

/// Run one scenario: copy input → tempdir, run patch, compare to expected/.
///
/// Uses `enable_move = true` to honor `*** Move to:` hunks (scenarios 004, 010).
/// The default `enable_move = false` is reserved for production (D2.5).
async fn run_scenario(dir: &Path) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let input_dir = dir.join("input");
    if input_dir.is_dir() {
        copy_dir_recursive(&input_dir, tmp.path());
    }
    let patch =
        std::fs::read_to_string(dir.join("patch.txt")).expect("read patch.txt");

    let tool = ApplyPatchTool { enable_move: true };
    let input = ToolInput {
        name: "apply_patch".to_string(),
        input: serde_json::json!({ "patch": patch }),
        context: ToolExecutionContext::new(
            "codex-scenario".to_string(),
            tmp.path().to_path_buf(),
        ),
    };
    let _output: ToolOutput = tool.call(input).await;
    // We intentionally do not assert on `_output.is_error`. The codex
    // scenarios specify the final filesystem state, not the tool's exit
    // status — matches `tests/suite/scenarios.rs` semantics.

    let expected_snapshot = snapshot_dir(&dir.join("expected"));
    let actual_snapshot = snapshot_dir(tmp.path());
    assert_eq!(
        actual_snapshot,
        expected_snapshot,
        "Scenario {} did not match expected final state.\nExpected entries: {:#?}\nActual entries:   {:#?}",
        dir.display(),
        expected_snapshot,
        actual_snapshot
    );
}

/// Discover all scenario directories under `tests/fixtures/codex/`.
///
/// Returned in numeric-prefix order so test failures are reproducible.
fn discover_scenarios() -> Vec<PathBuf> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let scenarios_dir =
        manifest_dir.join("tests").join("fixtures").join("codex");
    let mut out: Vec<PathBuf> = std::fs::read_dir(&scenarios_dir)
        .expect("read fixtures/codex dir")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    out.sort();
    out
}

/// Enumerate scenario names as `&'static str` for `cargo test` output.
const SCENARIO_NAMES: &[&str] = &[
    "001_add_file",
    "002_multiple_operations",
    "003_multiple_chunks",
    "004_move_to_new_directory",
    "005_rejects_empty_patch",
    "006_rejects_missing_context",
    "007_rejects_missing_file_delete",
    "008_rejects_empty_update_hunk",
    "009_requires_existing_file_for_update",
    "010_move_overwrites_existing_destination",
    "011_add_overwrites_existing_file",
    "012_delete_directory_fails",
    "013_rejects_invalid_hunk_header",
    "014_update_file_appends_trailing_newline",
    "015_failure_after_partial_success_leaves_changes",
    "016_pure_addition_update_chunk",
    "017_whitespace_padded_hunk_header",
    "018_whitespace_padded_patch_markers",
    "019_unicode_simple",
    "020_delete_file_success",
    "020_whitespace_padded_patch_marker_lines",
    "021_update_file_deletion_only",
    "022_update_file_end_of_file_marker",
];

#[test]
fn codex_scenarios_discovered() {
    let discovered: Vec<String> = discover_scenarios()
        .into_iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    let expected: Vec<String> =
        SCENARIO_NAMES.iter().map(|s| s.to_string()).collect();
    assert_eq!(
        discovered, expected,
        "fixtures/codex must contain exactly the 22 vendored codex scenarios (plus 020 variant)"
    );
}

/// Macro to generate a `#[tokio::test]` per scenario name. Each test is
/// `codex_scenario_<sanitized_name>` so failures point at the exact
/// scenario in `cargo test` output.
macro_rules! codex_scenario_test {
    ($name:ident, $idx:expr) => {
        #[tokio::test]
        async fn $name() {
            let scenarios = discover_scenarios();
            let dir = scenarios[$idx]
                .as_path()
                .canonicalize()
                .expect("canonicalize scenario dir");
            run_scenario(&dir).await;
        }
    };
}

codex_scenario_test!(codex_scenario_001_add_file, 0);
codex_scenario_test!(codex_scenario_002_multiple_operations, 1);
codex_scenario_test!(codex_scenario_003_multiple_chunks, 2);
codex_scenario_test!(codex_scenario_004_move_to_new_directory, 3);
codex_scenario_test!(codex_scenario_005_rejects_empty_patch, 4);
codex_scenario_test!(codex_scenario_006_rejects_missing_context, 5);
codex_scenario_test!(codex_scenario_007_rejects_missing_file_delete, 6);
codex_scenario_test!(codex_scenario_008_rejects_empty_update_hunk, 7);
codex_scenario_test!(codex_scenario_009_requires_existing_file_for_update, 8);
codex_scenario_test!(
    codex_scenario_010_move_overwrites_existing_destination,
    9
);
codex_scenario_test!(codex_scenario_011_add_overwrites_existing_file, 10);
codex_scenario_test!(codex_scenario_012_delete_directory_fails, 11);
codex_scenario_test!(codex_scenario_013_rejects_invalid_hunk_header, 12);
codex_scenario_test!(
    codex_scenario_014_update_file_appends_trailing_newline,
    13
);
codex_scenario_test!(
    codex_scenario_015_failure_after_partial_success_leaves_changes,
    14
);
codex_scenario_test!(codex_scenario_016_pure_addition_update_chunk, 15);
codex_scenario_test!(codex_scenario_017_whitespace_padded_hunk_header, 16);
codex_scenario_test!(codex_scenario_018_whitespace_padded_patch_markers, 17);
codex_scenario_test!(codex_scenario_019_unicode_simple, 18);
codex_scenario_test!(codex_scenario_020_delete_file_success, 19);
codex_scenario_test!(
    codex_scenario_020_whitespace_padded_patch_marker_lines,
    20
);
codex_scenario_test!(codex_scenario_021_update_file_deletion_only, 21);
codex_scenario_test!(codex_scenario_022_update_file_end_of_file_marker, 22);
