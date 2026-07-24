//! Unit tests for the agent-file loader family.
//!
//! All 31 tests for [`super::types::AgentChangeEvent`],
//! [`super::loader::AgentFileLoader`] (8 methods
//! exercised across 18 tests), and
//! [`super::extends::resolve_extends`] (8 tests)
//! live here.
//!
//! The `unique_tmp_dir` / `write_file` / `resolve`
//! helpers are centralised because the tests do
//! a lot of `tempdir + write + load + assert` —
//! without centralisation the test code would
//! repeat the `COUNTER.fetch_add` ceremony on
//! every fixture call (which would have caused
//! collisions on parallel runs).

use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use super::{
    extends::resolve_extends,
    loader::AgentFileLoader,
    types::AgentChangeEvent,
};
use crate::agent_file::frontmatter::FileAgentFrontmatter;

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn unique_tmp_dir(label: &str) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir =
        std::env::temp_dir().join(format!("synthia-agent-loader-{label}-{n}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create tmp dir");
    dir
}

fn write_file(dir: &Path, name: &str, content: &str) {
    let path = dir.join(name);
    fs::write(&path, content).expect("write file");
}

#[test]
fn change_event_variants_construct_and_clone() {
    let added = AgentChangeEvent::Added("foo".to_string());
    let modified = AgentChangeEvent::Modified("bar".to_string());
    let removed = AgentChangeEvent::Removed("baz".to_string());

    let _ = added.clone();
    let _ = modified.clone();
    let _ = removed.clone();
}

#[test]
fn change_events_support_equality() {
    assert_eq!(
        AgentChangeEvent::Added("x".to_string()),
        AgentChangeEvent::Added("x".to_string())
    );
    assert_ne!(
        AgentChangeEvent::Added("x".to_string()),
        AgentChangeEvent::Modified("x".to_string())
    );
}

#[test]
fn take_change_events_drains_and_returns_empty_on_second_call() {
    let loader = AgentFileLoader::new(std::env::temp_dir());
    assert!(loader.take_change_events().is_empty());
    assert!(loader.take_change_events().is_empty());
}

#[test]
fn load_emits_added_on_first_call() {
    let dir = unique_tmp_dir("event-add");
    write_file(&dir, "fresh.md", "---\nmode: x\n---\nbody\n");
    let loader = AgentFileLoader::new(dir.clone());

    let _ = loader.reload("fresh").expect("first load");
    let events = loader.take_change_events();
    assert_eq!(events, vec![AgentChangeEvent::Added("fresh".to_string())]);

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn load_emits_modified_when_content_hash_changes() {
    let dir = unique_tmp_dir("event-mod");
    let path = dir.join("agent.md");
    write_file(&dir, "agent.md", "---\nmode: x\n---\nfirst body\n");
    let loader = AgentFileLoader::new(dir.clone());

    let _ = loader.reload("agent").expect("first load");
    let _ = loader.take_change_events();

    fs::write(&path, "---\nmode: x\n---\nsecond body\n").expect("rewrite");
    let _ = loader.reload("agent").expect("second load");

    let events = loader.take_change_events();
    assert_eq!(
        events,
        vec![AgentChangeEvent::Modified("agent".to_string())]
    );

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn load_emits_no_event_when_content_unchanged() {
    let dir = unique_tmp_dir("event-unchanged");
    write_file(&dir, "stable.md", "---\nmode: x\n---\nbody\n");
    let loader = AgentFileLoader::new(dir.clone());

    let _ = loader.reload("stable").expect("first load");
    let _ = loader.take_change_events();

    let _ = loader.reload("stable").expect("second load");
    assert!(loader.take_change_events().is_empty());

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn load_does_not_emit_added_twice_for_same_id() {
    let dir = unique_tmp_dir("event-add-once");
    let path = dir.join("agent.md");
    write_file(&dir, "agent.md", "---\nmode: x\n---\nfirst body\n");
    let loader = AgentFileLoader::new(dir.clone());

    let _ = loader.reload("agent").expect("first load");
    let _ = loader.take_change_events();

    fs::write(&path, "---\nmode: x\n---\nsecond body\n").expect("rewrite");
    let _ = loader.reload("agent").expect("second load");
    let _ = loader.take_change_events();

    fs::write(&path, "---\nmode: x\n---\nthird body\n").expect("rewrite again");
    let _ = loader.reload("agent").expect("third load");

    let events = loader.take_change_events();
    assert_eq!(
        events,
        vec![AgentChangeEvent::Modified("agent".to_string())],
        "third load should emit Modified, not Added"
    );

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn detect_removals_emits_removed_for_missing_files() {
    let dir = unique_tmp_dir("event-remove");
    let path_a = dir.join("a.md");
    let path_b = dir.join("b.md");
    write_file(&dir, "a.md", "---\nmode: a\n---\n");
    write_file(&dir, "b.md", "---\nmode: b\n---\n");
    let loader = AgentFileLoader::new(dir.clone());

    let _ = loader.load("a").expect("load a");
    let _ = loader.load("b").expect("load b");
    let _ = loader.take_change_events();

    fs::remove_file(&path_a).expect("remove a");
    fs::remove_file(&path_b).expect("remove b");

    let removed = loader.detect_removals();
    let mut removed_sorted = removed.clone();
    removed_sorted.sort();
    assert_eq!(removed_sorted, vec!["a".to_string(), "b".to_string()]);

    let events = loader.take_change_events();
    let mut events_sorted = events.clone();
    events_sorted.sort_by(|a, b| match (a, b) {
        (AgentChangeEvent::Removed(x), AgentChangeEvent::Removed(y)) => {
            x.cmp(y)
        }
        _ => std::cmp::Ordering::Equal,
    });
    assert_eq!(
        events_sorted,
        vec![
            AgentChangeEvent::Removed("a".to_string()),
            AgentChangeEvent::Removed("b".to_string()),
        ]
    );

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn detect_removals_returns_empty_when_no_files_removed() {
    let dir = unique_tmp_dir("event-remove-none");
    write_file(&dir, "a.md", "---\nmode: a\n---\n");
    let loader = AgentFileLoader::new(dir.clone());

    let _ = loader.load("a").expect("load a");
    let _ = loader.take_change_events();

    assert!(loader.detect_removals().is_empty());
    assert!(loader.take_change_events().is_empty());

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn detect_removals_clears_seen_state_so_subsequent_load_emits_added() {
    let dir = unique_tmp_dir("event-remove-readd");
    let path = dir.join("agent.md");
    write_file(&dir, "agent.md", "---\nmode: a\n---\nbody\n");
    let loader = AgentFileLoader::new(dir.clone());

    let _ = loader.reload("agent").expect("first load");
    let _ = loader.take_change_events();

    fs::remove_file(&path).expect("remove");
    let _ = loader.detect_removals();
    let _ = loader.take_change_events();

    write_file(&dir, "agent.md", "---\nmode: a\n---\nbody\n");
    let _ = loader.reload("agent").expect("re-load");

    let events = loader.take_change_events();
    assert_eq!(
        events,
        vec![AgentChangeEvent::Added("agent".to_string())],
        "id should be re-emitted as Added after detection of removal"
    );

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn watch_returns_watcher_for_existing_directory() {
    let dir = std::env::temp_dir();
    let loader = AgentFileLoader::new(dir);
    let watcher = loader.watch().expect("watch should succeed on temp dir");
    drop(watcher);
}

#[test]
fn list_ids_returns_md_stems_in_directory() {
    let dir = unique_tmp_dir("list-basic");
    write_file(&dir, "alpha.md", "---\nmode: a\n---\nbody a\n");
    write_file(&dir, "beta.md", "---\nmode: b\n---\nbody b\n");

    let loader = AgentFileLoader::new(dir.clone());
    let mut ids = loader.list_ids();
    ids.sort();

    assert_eq!(ids, vec!["alpha".to_string(), "beta".to_string()]);
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn list_ids_ignores_non_md_files() {
    let dir = unique_tmp_dir("list-filter");
    write_file(&dir, "agent.md", "---\nmode: x\n---\n");
    write_file(&dir, "notes.txt", "ignore me");
    write_file(&dir, "README", "ignore me");

    let loader = AgentFileLoader::new(dir.clone());
    let mut ids = loader.list_ids();
    ids.sort();

    assert_eq!(ids, vec!["agent".to_string()]);
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn list_ids_returns_empty_when_directory_missing() {
    let dir = std::env::temp_dir().join("synthia-agent-loader-missing-dir-xyz");
    let _ = fs::remove_dir_all(&dir);

    let loader = AgentFileLoader::new(dir);
    assert!(loader.list_ids().is_empty());
}

#[test]
fn load_reads_and_parses_existing_file() {
    let dir = unique_tmp_dir("load-ok");
    write_file(
        &dir,
        "reviewer.md",
        "---\nmode: reviewer\nsteps: 3\n---\nreview body\n",
    );

    let loader = AgentFileLoader::new(dir.clone());
    let parsed = loader.load("reviewer").expect("load should succeed");

    let fm = parsed.frontmatter.expect("frontmatter present");
    assert_eq!(fm.mode.as_deref(), Some("reviewer"));
    assert_eq!(fm.steps, Some(3));
    assert_eq!(parsed.body, "review body");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn load_returns_error_for_missing_file() {
    let dir = unique_tmp_dir("load-missing");
    let loader = AgentFileLoader::new(dir.clone());

    let err = loader.load("ghost").expect_err("missing file should fail");
    assert!(err.contains("ghost"), "got: {err}");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn load_returns_error_for_invalid_frontmatter_yaml() {
    let dir = unique_tmp_dir("load-bad-yaml");
    write_file(&dir, "bad.md", "---\nmodel: [unterminated\n---\nbody\n");

    let loader = AgentFileLoader::new(dir.clone());
    let err = loader.load("bad").expect_err("invalid yaml should fail");
    assert!(err.contains("YAML parse error"), "got: {err}");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn load_returns_error_for_missing_closing_marker() {
    let dir = unique_tmp_dir("load-no-close");
    write_file(&dir, "open.md", "---\nmodel: x\nno closer\n");

    let loader = AgentFileLoader::new(dir.clone());
    let err = loader.load("open").expect_err("missing closer should fail");
    assert!(err.contains("Missing closing"), "got: {err}");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn load_serves_from_cache_on_second_call() {
    let dir = unique_tmp_dir("load-cache");
    let path = dir.join("cached.md");
    write_file(&dir, "cached.md", "---\nmode: cached\n---\nfirst body\n");

    let loader = AgentFileLoader::new(dir.clone());
    let first = loader.load("cached").expect("first load");
    assert_eq!(first.body, "first body");

    fs::write(&path, "---\nmode: cached\n---\nsecond body\n").expect("rewrite");
    let second = loader.load("cached").expect("second load");
    assert_eq!(
        second.body, "first body",
        "second load should come from cache, not disk"
    );

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn load_does_not_cache_files_without_frontmatter() {
    let dir = unique_tmp_dir("load-no-fm-cache");
    let path = dir.join("nofm.md");
    write_file(&dir, "nofm.md", "no frontmatter body\n");

    let loader = AgentFileLoader::new(dir.clone());
    let first = loader.load("nofm").expect("first load");
    assert!(first.frontmatter.is_none());
    assert_eq!(first.body, "no frontmatter body\n");

    fs::write(&path, "changed body\n").expect("rewrite");
    let second = loader.load("nofm").expect("second load");
    assert_eq!(second.body, "changed body\n");

    fs::remove_dir_all(&dir).ok();
}

fn resolve(dir: &Path, id: &str) -> Result<FileAgentFrontmatter, String> {
    let loader = AgentFileLoader::new(dir.to_path_buf());
    let mut visited = Vec::new();
    resolve_extends(id, &loader, &mut visited)
}

#[test]
fn resolve_extends_returns_own_frontmatter_when_no_extends() {
    let dir = unique_tmp_dir("resolve-no-extends");
    write_file(
        &dir,
        "solo.md",
        "---\nmode: architect\nsteps: 3\n---\nbody\n",
    );

    let merged = resolve(&dir, "solo").expect("should resolve");
    assert_eq!(merged.mode.as_deref(), Some("architect"));
    assert_eq!(merged.steps, Some(3));
    assert!(merged.extends.is_none());
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn resolve_extends_merges_parent_into_child_with_child_priority() {
    let dir = unique_tmp_dir("resolve-merge");
    write_file(
        &dir,
        "base.md",
        "---\nmode: architect\nmodel: parent-model\nsteps: 5\ncolor: \"#ff0000\"\n---\nparent body\n",
    );
    write_file(
        &dir,
        "child.md",
        "---\nextends: base\nmode: plan\nmodel: child-model\n---\nchild body\n",
    );

    let merged = resolve(&dir, "child").expect("should resolve");
    assert_eq!(merged.model.as_deref(), Some("child-model"));
    assert_eq!(merged.mode.as_deref(), Some("plan"));
    assert_eq!(merged.steps, Some(5), "parent steps inherited");
    assert_eq!(merged.color.as_deref(), Some("#ff0000"));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn resolve_extends_supports_multi_level_chain() {
    let dir = unique_tmp_dir("resolve-chain");
    write_file(
        &dir,
        "root.md",
        "---\nmode: architect\nmodel: root-model\nhidden: true\n---\n",
    );
    write_file(&dir, "mid.md", "---\nextends: root\nsteps: 7\n---\n");
    write_file(&dir, "leaf.md", "---\nextends: mid\nmode: plan\n---\n");

    let merged = resolve(&dir, "leaf").expect("should resolve");
    assert_eq!(merged.model.as_deref(), Some("root-model"));
    assert_eq!(merged.mode.as_deref(), Some("plan"));
    assert_eq!(merged.steps, Some(7));
    assert_eq!(merged.hidden, Some(true));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn resolve_extends_detects_direct_self_cycle() {
    let dir = unique_tmp_dir("resolve-self-cycle");
    write_file(&dir, "loop.md", "---\nextends: loop\nmode: plan\n---\n");

    let err = resolve(&dir, "loop").expect_err("self cycle should fail");
    assert!(err.contains("circular extends"), "got: {err}");
    assert!(err.contains("loop"), "got: {err}");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn resolve_extends_detects_indirect_cycle() {
    let dir = unique_tmp_dir("resolve-indirect-cycle");
    write_file(&dir, "a.md", "---\nextends: b\nmode: plan\n---\n");
    write_file(&dir, "b.md", "---\nextends: a\nmode: plan\n---\n");

    let err = resolve(&dir, "a").expect_err("indirect cycle should fail");
    assert!(err.contains("circular extends"), "got: {err}");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn resolve_extends_allows_chain_of_exactly_max_depth() {
    let dir = unique_tmp_dir("resolve-max-depth");
    write_file(&dir, "d0.md", "---\nmodel: root\n---\n");
    write_file(&dir, "d1.md", "---\nextends: d0\nmode: a\n---\n");
    write_file(&dir, "d2.md", "---\nextends: d1\nmode: b\n---\n");
    write_file(&dir, "d3.md", "---\nextends: d2\nmode: c\n---\n");

    let merged = resolve(&dir, "d3").expect("chain of 4 should be allowed");
    assert_eq!(merged.model.as_deref(), Some("root"));
    assert_eq!(merged.mode.as_deref(), Some("c"));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn resolve_extends_rejects_chain_exceeding_max_depth() {
    let dir = unique_tmp_dir("resolve-depth-limit");
    write_file(&dir, "d0.md", "---\nmode: a\n---\n");
    write_file(&dir, "d1.md", "---\nextends: d0\nmode: b\n---\n");
    write_file(&dir, "d2.md", "---\nextends: d1\nmode: c\n---\n");
    write_file(&dir, "d3.md", "---\nextends: d2\nmode: d\n---\n");
    write_file(&dir, "d4.md", "---\nextends: d3\nmode: e\n---\n");
    write_file(&dir, "d5.md", "---\nextends: d4\nmode: f\n---\n");

    let err = resolve(&dir, "d5").expect_err("chain of 5 should fail");
    assert!(err.contains("depth exceeded"), "got: {err}");
    assert!(err.contains("4"), "got: {err}");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn resolve_extends_propagates_load_errors_for_missing_parent() {
    let dir = unique_tmp_dir("resolve-missing-parent");
    write_file(&dir, "child.md", "---\nextends: ghost\nmode: plan\n---\n");

    let err = resolve(&dir, "child").expect_err("missing parent should fail");
    assert!(err.contains("ghost"), "got: {err}");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn resolve_extends_returns_default_for_file_without_frontmatter() {
    let dir = unique_tmp_dir("resolve-no-frontmatter");
    write_file(&dir, "nofm.md", "just a body, no frontmatter\n");

    let merged = resolve(&dir, "nofm").expect("should resolve to default");
    assert!(merged.model.is_none());
    assert!(merged.extends.is_none());
    assert!(merged.permission_rules.is_empty());
    fs::remove_dir_all(&dir).ok();
}
