//! End-to-end integration tests for `AgentsMdSection` hierarchical
//! discovery through the public `PromptBuilder` API.
//!
//! Covers the scenarios from
//! `openspec/changes/agents-md-hierarchical-discovery/specs/agents-md-hierarchical-discovery/spec.md`:
//! - Builder with default sections registers `agents_md` between
//!   `environment` and `memory`.
//! - `PromptBuilder::resolve` with a workspace containing two
//!   `AGENTS.md` files at different depths returns merged content
//!   in farthest-to-closest order.
//! - An empty workspace (no `AGENTS.md` in any ancestor that we can
//!   see) contributes nothing to the dynamic content.
//! - Session-cached section content is reused across two consecutive
//!   `resolve` calls; modifying the file on disk between calls does
//!   not change the cached output.

use std::sync::LazyLock;

use synthia_context::prompt::{
    AgentName,
    PromptBuilder,
    PromptContext,
    PromptState,
    SectionCaching,
};

static TEST_AGENT_NAME: LazyLock<AgentName> =
    LazyLock::new(|| AgentName::Custom("TestAgent".to_string()));

fn make_context<'a>(workspace_dir: &'a std::path::Path) -> PromptContext<'a> {
    PromptContext {
        agent_name: &TEST_AGENT_NAME,
        agent_description: "A test agent",
        workspace_dir,
        skill_instructions: String::new(),
        is_subagent: false,
        session_id: Some("agents-md-test"),
        mcp_servers: &[],
        additional_dirs: &[],
        output_style: None,
        language_preference: None,
        is_proactive_mode: false,
        model_name: Some("claude-sonnet"),
        knowledge_cutoff: Some("2026-03-01"),
        team_info: None,
    }
}

// -- 1. Registration ----------------------------------------------------

#[test]
fn builder_default_sections_register_agents_md_between_env_and_memory() {
    let builder = PromptBuilder::default_with_sections();
    let names = builder.section_names();
    let env_idx = names.iter().position(|&n| n == "environment").unwrap();
    let agents_idx = names.iter().position(|&n| n == "agents_md").unwrap();
    let mem_idx = names.iter().position(|&n| n == "memory").unwrap();
    assert!(env_idx < agents_idx);
    assert!(agents_idx < mem_idx);
}

#[test]
fn agents_md_section_caching_is_session_cached() {
    // We can't reach into `builder.sections` (private), so we
    // exercise the section's caching via a single end-to-end
    // resolve + modify-file + resolve cycle: a SectionCached or
    // Uncached section would re-read the disk; only SessionCached
    // yields the original value. See
    // `session_cache_reuses_agents_md_across_resolve_calls` below.
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("AGENTS.md"), "v1").unwrap();
    let ctx = make_context(dir.path());
    let mut state = PromptState::new();
    let builder = PromptBuilder::default_with_sections();
    let _ = builder.resolve(&ctx, &mut state).unwrap();
    let cached = state
        .get("agents_md", SectionCaching::SessionCached)
        .expect("agents_md must be in session cache after first resolve");
    let _ = cached; // existence is the assertion
}

// -- 2. End-to-end merge -------------------------------------------------

#[test]
fn resolve_merges_two_agents_md_files_farthest_to_closest() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("AGENTS.md"), "GLOBAL-MARKER").unwrap();

    let repo = dir.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    std::fs::write(repo.join("AGENTS.md"), "REPO-MARKER").unwrap();

    let ctx = make_context(&repo);
    let mut state = PromptState::new();
    let resolved = PromptBuilder::default_with_sections()
        .resolve(&ctx, &mut state)
        .unwrap();

    let dynamic = resolved.dynamic_content;
    assert!(
        dynamic.contains("GLOBAL-MARKER"),
        "farthest file content must appear, got: {dynamic}"
    );
    assert!(
        dynamic.contains("REPO-MARKER"),
        "closest file content must appear, got: {dynamic}"
    );
    let global_pos = dynamic.find("GLOBAL-MARKER").unwrap();
    let repo_pos = dynamic.find("REPO-MARKER").unwrap();
    assert!(
        global_pos < repo_pos,
        "farthest file (GLOBAL) must appear before closest file (REPO), \
         got positions: global={global_pos}, repo={repo_pos}"
    );
    assert!(
        dynamic.contains("<agents_md>") && dynamic.contains("</agents_md>"),
        "output must be wrapped in <agents_md> tags"
    );
    assert!(resolved.sections_used.iter().any(|n| n == "agents_md"));
}

// -- 3. Empty workspace --------------------------------------------------

#[test]
fn resolve_with_no_agents_md_omits_section() {
    // Create a tempdir with no AGENTS.md inside, AND use it as the
    // workspace so the ancestor walk has nothing to find (we filter
    // out unrelated /tmp finds via path-prefix check below).
    let dir = tempfile::tempdir().unwrap();
    let ctx = make_context(dir.path());
    let mut state = PromptState::new();
    let resolved = PromptBuilder::default_with_sections()
        .resolve(&ctx, &mut state)
        .unwrap();

    // The section is registered and may be cached, but if it produced
    // no content, it must not be in `sections_used`.
    assert!(
        !resolved.sections_used.iter().any(|n| n == "agents_md"),
        "agents_md section should be omitted from sections_used when no \
         AGENTS.md is found within the workspace's ancestor chain, got \
         sections_used={:?}",
        resolved.sections_used
    );

    // If somehow content did appear (e.g. a stray AGENTS.md outside
    // our tempdir that happened to be on the path), it must reference
    // a path under our tempdir — this guards against the ancestor
    // walk leaking unrelated files.
    if !resolved.dynamic_content.is_empty() {
        for line in resolved.dynamic_content.lines() {
            if let Some(rest) = line.strip_prefix("## AGENTS.md: ") {
                let p = std::path::Path::new(rest.trim());
                assert!(
                    p.starts_with(dir.path()),
                    "agents_md surfaced a path outside the tempdir: {p:?}"
                );
            }
        }
    }
}

// -- 4. Session caching across resolve calls -----------------------------

#[test]
fn session_cache_reuses_agents_md_across_resolve_calls() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("AGENTS.md"), "V1-CONTENT").unwrap();

    let ctx = make_context(dir.path());
    let mut state = PromptState::new();
    let builder = PromptBuilder::default_with_sections();

    let first = builder.resolve(&ctx, &mut state).unwrap();
    let first_dynamic = first.dynamic_content.clone();
    assert!(first_dynamic.contains("V1-CONTENT"));

    // Mutate the file on disk between calls.
    std::fs::write(dir.path().join("AGENTS.md"), "V2-CONTENT").unwrap();

    let second = builder.resolve(&ctx, &mut state).unwrap();
    let second_dynamic = second.dynamic_content.clone();
    // SessionCached → second call must reuse the cached V1 content.
    assert!(
        second_dynamic.contains("V1-CONTENT"),
        "second resolve should use the session-cached V1 content, got: \
         {second_dynamic}"
    );
    assert!(
        !second_dynamic.contains("V2-CONTENT"),
        "second resolve must NOT see the disk-mutated V2 (SessionCached), \
         got: {second_dynamic}"
    );

    // After clearing the session, the next resolve must observe V2.
    state.clear_session();
    let third = builder.resolve(&ctx, &mut state).unwrap();
    assert!(
        third.dynamic_content.contains("V2-CONTENT"),
        "after clear_session, the resolve must observe the disk-mutated \
         V2, got: {}",
        third.dynamic_content
    );
}

// -- 5. Sanity ---------------------------------------------------------

#[test]
fn section_name_is_agents_md() {
    let builder = PromptBuilder::default_with_sections();
    let names = builder.section_names();
    assert!(names.contains(&"agents_md"));
}
