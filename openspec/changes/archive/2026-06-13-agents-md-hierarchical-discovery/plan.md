# AGENTS.md Hierarchical Discovery Implementation Plan

> **For agentic workers:** Use `superpowers:subagent-driven-development` (or sequential TDD) to implement this plan task-by-task. Each Task is one PR.

**Goal:** Add a dedicated `AgentsMdSection` to Synthia that walks
`workspace_dir`'s ancestors for `AGENTS.md` files, merges them
farthest-to-closest, enforces size limits, and injects the merged content
into the system prompt at a fixed position. Decouple AGENTS.md injection
from `IdentitySection` and add configuration to `AgentConfig`.

**Architecture:** New module `crates/synthia-context/src/prompt/sections/agents_md.rs`
holds `AgentsMdConfig` (config), `AgentsMdSection` (PromptSection impl),
and pure-function `walk_ancestors(...)` (testable IO). The
`PromptBuilder` registers the new section between `EnvironmentSection`
and `MemorySection`. `AgentConfig` exposes two new fields with
`#[serde(default)]` for backward compat. `IdentitySection` drops
`AGENTS.md` from its `WORKSPACE_FILES`.

**Tech Stack:** Rust 2024, `std::fs`, `tracing` for logging, existing
`PromptBuilder` / `PromptSection` trait. No new dependencies.

---

## Task 1: Config + module skeleton

**Files:** `crates/synthia-context/src/prompt/sections/agents_md.rs` (new),
`crates/synthia-context/src/prompt/sections/mod.rs` (export).

- [ ] **Step 1:** Create `agents_md.rs` with `#[derive(Debug, Clone)] struct AgentsMdConfig { enabled: bool, filenames: Vec<String>, max_chars_per_file: usize, max_chars_total: usize }` and `impl Default` returning `Self { enabled: true, filenames: vec!["AGENTS.md".to_string()], max_chars_per_file: 20_000, max_chars_total: 60_000 }`
- [ ] **Step 2:** Add `pub struct AgentsMdSection { config: Arc<AgentsMdConfig> }` with `impl Default` returning `Self { config: Arc::new(AgentsMdConfig::default()) }` and `pub fn with_config(config: AgentsMdConfig) -> Self`
- [ ] **Step 3:** Add `#[cfg(test)] mod tests` with `test_default_config` (asserts the 4 default values)
- [ ] **Step 4:** Add `pub mod agents_md;` and `pub use agents_md::AgentsMdSection;` to `mod.rs`
- [ ] **Step 5:** `cargo check -p synthia-context` → expect success (no compilation errors)
- [ ] **Step 6:** `cargo test -p synthia-context agents_md::tests::test_default_config` → expect pass
- [ ] **Step 7:** Commit: `feat(context): scaffold AgentsMdSection module`

---

## Task 2: Ancestor walk + merge algorithm

**Files:** `agents_md.rs` (extend)

- [ ] **Step 1:** Add `fn walk_ancestors(workspace_dir: &Path, filenames: &[String]) -> Vec<DiscoveredFile>` where `DiscoveredFile { path: PathBuf, content: String, truncated_at: Option<usize> }` (initially no truncation)
- [ ] **Step 2:** Walk: `for ancestor in workspace_dir.ancestors() { for filename in filenames { let p = ancestor.join(filename); if p.is_file() { read content; push to results } } }`
- [ ] **Step 3:** Collect in ancestor order (outermost first = last in iteration = first in result after `.rev()` is WRONG — keep natural ancestor order which goes from deepest to root; then `.rev()` for farthest-to-closest)
- [ ] **Step 4:** Symlink cycle protection: maintain `HashSet<PathBuf>` of canonical paths; if `canonicalize(...).ok()` returns Some path already in set, skip and log debug
- [ ] **Step 5:** Error handling: on `read_to_string` Err, log `tracing::warn!` with path and error, continue
- [ ] **Step 6:** Add tests: `test_walk_single_file`, `test_walk_multiple_files_farthest_first`, `test_walk_no_files`, `test_walk_skips_directory_placeholder`, `test_walk_handles_circular_symlink` (use `std::os::unix::fs::symlink` in `tempfile::tempdir`)
- [ ] **Step 7:** `cargo test -p synthia-context walk_ancestors` → expect pass
- [ ] **Step 8:** Commit: `feat(context): walk ancestors for AGENTS.md files`

---

## Task 3: Size limit enforcement

**Files:** `agents_md.rs` (extend)

- [ ] **Step 1:** Add `fn truncate_with_marker(s: &str, max_chars: usize) -> String` returning `s` if `s.chars().count() <= max_chars` else `s.chars().take(max_chars).collect::<String>() + "\n\n[... truncated at <N> chars - use read for full file ...]\n"`
- [ ] **Step 2:** In `build`: after collecting `DiscoveredFile` list, for each file truncate content if needed; track cumulative char count
- [ ] **Step 3:** When cumulative count exceeds `max_chars_total`, stop iterating; push a final marker `"[... total content exceeded <N> chars; further AGENTS.md files omitted ...]\n"` and break
- [ ] **Step 4:** Marker: if the LAST entry that fit is the closest `AGENTS.md`, it stays; if a farther one stayed but the closer one was cut, the closer one is NOT included (per spec: "include the closest before any earlier when both cannot fit" — but practical implementation: iterate farthest→closest, append each; the last one in the loop IS the closest, so it gets the highest chance to fit; this matches "closest included when both cannot fit" only if closest is processed last. Since we walk farthest→closest (via .rev()), the closest IS last → naturally favored)
- [ ] **Step 5:** Add tests: `test_truncate_under_limit`, `test_truncate_over_per_file_limit`, `test_truncate_over_total_limit_keeps_closest`
- [ ] **Step 6:** `cargo test -p synthia-context truncate` → expect pass
- [ ] **Step 7:** Commit: `feat(context): enforce per-file and total AGENTS.md size limits`

---

## Task 4: PromptSection trait impl

**Files:** `agents_md.rs` (extend)

- [ ] **Step 1:** `impl PromptSection for AgentsMdSection`: `name() -> "agents_md"`, `caching() -> SectionCaching::SessionCached`
- [ ] **Step 2:** `build(ctx: &PromptContext) -> Result<String>`: if `!config.enabled` return `Ok(String::new())`; else call `walk_ancestors(ctx.workspace_dir, &config.filenames)`; if empty return `Ok(String::new())`; else format with header per file `## AGENTS.md: <absolute path>` + body, separated by `\n\n---\n\n`
- [ ] **Step 3:** Emit `tracing::debug!(path = %file.path.display(), chars = file.content.len(), "agents_md loaded")` per file in result
- [ ] **Step 4:** Wrap final section in `# Project Agent Instructions\n\n<agents_md>\n...\n</agents_md>` for clear visual boundary (mirroring `EnvironmentSection`'s `<env>` style)
- [ ] **Step 5:** Add tests: `test_section_name`, `test_section_caching`, `test_section_build_disabled`, `test_section_build_empty_workspace`, `test_section_build_merges_two_files`
- [ ] **Step 6:** `cargo test -p synthia-context agents_md` → expect pass
- [ ] **Step 7:** Commit: `feat(context): implement AgentsMdSection as PromptSection`

---

## Task 5: PromptBuilder integration

**Files:** `crates/synthia-context/src/prompt/builder.rs` (modify)

- [ ] **Step 1:** In `PromptBuilder::default_with_sections()`, insert `Box::new(AgentsMdSection::default())` AFTER `Box::new(EnvironmentSection::new())` and BEFORE `Box::new(MemorySection::new())`
- [ ] **Step 2:** Repeat the same insertion in `PromptBuilder::build_for_name(...)` at the same position
- [ ] **Step 3:** Add `test_default_sections_include_agents_md` (asserts `section_names().contains(&"agents_md")`)
- [ ] **Step 4:** Add `test_agents_md_position` (asserts env_idx < agents_idx < mem_idx)
- [ ] **Step 5:** `cargo test -p synthia-context builder` → expect pass
- [ ] **Step 6:** Commit: `feat(context): register AgentsMdSection in PromptBuilder`

---

## Task 6: IdentitySection cleanup

**Files:** `crates/synthia-context/src/prompt/sections/identity.rs` (modify)

- [ ] **Step 1:** Change `WORKSPACE_FILES` constant from `&["AGENTS.md", "IDENTITY.md", "USER.md", "MEMORY.md"]` to `&["IDENTITY.md", "USER.md", "MEMORY.md"]`
- [ ] **Step 2:** Update `test_has_workspace_files` to create the same files but pass the new constant list
- [ ] **Step 3:** Add new test `test_workspace_files_excludes_agents_md` asserting the constant does not contain "AGENTS.md"
- [ ] **Step 4:** `cargo test -p synthia-context identity` → expect pass
- [ ] **Step 5:** Commit: `refactor(context): remove AGENTS.md from IdentitySection workspace files`

---

## Task 7: AgentConfig fields

**Files:** `crates/synthia-agent/src/config/agent_config.rs` (modify),
5 call sites (grep verified)

- [ ] **Step 1:** Add `#[serde(default)] pub agents_md_enabled: bool` and `#[serde(default)] pub agents_md_filenames: Vec<String>` to `AgentConfig`
- [ ] **Step 2:** In `impl Default for AgentConfig`, set `agents_md_enabled: true` and `agents_md_filenames: vec!["AGENTS.md".to_string()]`
- [ ] **Step 3:** Run `cargo check -p synthia-agent` → expect errors at 5 call sites that use struct literal
- [ ] **Step 4:** At each call site (agent.rs:412, server/state.rs, server/routes/ws.rs, server/routes/chat.rs, cli/repl_core/repl.rs), add `..Default::default()` to the struct literal (or, if not using struct literal, add the fields explicitly)
- [ ] **Step 5:** `cargo check --workspace` → expect success
- [ ] **Step 6:** Add `test_agent_config_default_agents_md` (asserts default values)
- [ ] **Step 7:** Add `test_agent_config_serde_backward_compat` (deserialize a TOML with no agents_md fields, assert defaults)
- [ ] **Step 8:** `cargo test -p synthia-agent config` → expect pass
- [ ] **Step 9:** Commit: `feat(agent): add agents_md_enabled and agents_md_filenames to AgentConfig`

---

## Task 8: End-to-end integration test

**Files:** `crates/synthia-context/tests/agents_md_discovery.rs` (new)

- [ ] **Step 1:** Create tempdir with structure: `<tmp>/AGENTS.md` (global) and `<tmp>/repo/AGENTS.md` (specific)
- [ ] **Step 2:** Construct `PromptContext` with `workspace_dir = <tmp>/repo`
- [ ] **Step 3:** Call `PromptBuilder::default_with_sections().resolve(&ctx, &mut state)`
- [ ] **Step 4:** Assert `result.dynamic_content` contains both file paths' headers in correct order
- [ ] **Step 5:** Add test: no AGENTS.md anywhere → section contributes nothing
- [ ] **Step 6:** Add test: two consecutive `resolve` calls with same state → second call's section content comes from session cache (verify via a counter file: write 1st time, modify, but cache should still return 1st content until clear_session)
- [ ] **Step 7:** `cargo test -p synthia-context --test agents_md_discovery` → expect pass
- [ ] **Step 8:** Commit: `test(context): e2e AGENTS.md hierarchical discovery`

---

## Task 9: Quality gates

- [ ] **Step 1:** `cargo +nightly fmt --all` (no diff expected after reformat)
- [ ] **Step 2:** `cargo clippy --all-targets --all-features --tests --all` → fix any new warnings
- [ ] **Step 3:** `cargo test --workspace` → expect all green (except pre-existing synthia-session tests/session_persistence.rs compile error on master, unrelated)
- [ ] **Step 4:** `openspec validate agents-md-hierarchical-discovery` → expect valid
- [ ] **Step 5:** Generate `verify.md` per the verify template
- [ ] **Step 6:** Commit: `chore: pass quality gates for agents-md-hierarchical-discovery`
