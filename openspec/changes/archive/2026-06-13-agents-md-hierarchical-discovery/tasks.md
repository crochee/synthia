## 1. AgentsMdConfig + module skeleton

- [x] 1.1 Add `AgentsMdConfig` struct in `crates/synthia-context/src/prompt/sections/agents_md.rs` with fields: `enabled: bool`, `filenames: Vec<String>`, `max_chars_per_file: usize`, `max_chars_total: usize`, and a `Default` impl
- [x] 1.2 Define `AgentsMdSection` struct holding `Arc<AgentsMdConfig>` and implement `Default` (uses default config)
- [x] 1.3 Add unit tests for `AgentsMdConfig::default()` (enabled=true, filenames=["AGENTS.md"], limits 20K/60K)
- [x] 1.4 Verify `cargo check -p synthia-context` passes with the skeleton

## 2. Ancestor walk + merge algorithm

- [x] 2.1 Implement `walk_ancestors(workspace_dir: &Path, filenames: &[String]) -> Vec<(PathBuf, String)>` returning `(absolute_path, file_contents)` pairs in farthest-to-closest order
- [x] 2.2 Handle symlink cycles: canonicalize each candidate and skip if canonical path was already visited
- [x] 2.3 Handle filesystem errors: log warn + skip on permission/UTF-8 errors; silently skip on missing/directory
- [x] 2.4 Implement merge header format: `## AGENTS.md: <absolute path>` per file, separated by `---`
- [x] 2.5 Add unit tests: single file, multi-level, no file, circular symlink, missing file, permission error, non-UTF-8

## 3. Size limit enforcement

- [x] 3.1 Implement per-file truncation at `max_chars_per_file` with marker `[... truncated at <N> chars - use read for full file ...]`
- [x] 3.2 Implement total truncation at `max_chars_total` with marker `[... total content exceeded <N> chars; further AGENTS.md files omitted ...]`
- [x] 3.3 On total truncation, ensure closest `AGENTS.md` (most-specific) is included before any earlier (less-specific) files
- [x] 3.4 Add unit tests: file at exact limit, file just over, multi-file over total limit

## 4. PromptSection trait impl

- [x] 4.1 Implement `name() -> "agents_md"` and `caching() -> SectionCaching::SessionCached`
- [x] 4.2 Implement `build(&PromptContext) -> Result<String>` that returns empty string when config disabled, else walks + merges + truncates
- [x] 4.3 Emit `tracing::debug!` per loaded file and `tracing::warn!` per load failure
- [x] 4.4 Export `AgentsMdSection` from `crates/synthia-context/src/prompt/sections/mod.rs`

## 5. PromptBuilder integration

- [x] 5.1 Add `AgentsMdSection::default()` to `PromptBuilder::default_with_sections()` between `EnvironmentSection` and `MemorySection`
- [x] 5.2 Add the same to `PromptBuilder::build_for_name(...)` at the same position
- [x] 5.3 Add unit test verifying `section_names()` contains `"agents_md"`
- [x] 5.4 Add unit test verifying position (after Environment, before Memory)

## 6. IdentitySection cleanup

- [x] 6.1 Remove `"AGENTS.md"` from `IdentitySection::WORKSPACE_FILES` constant
- [x] 6.2 Update the test `test_has_workspace_files` to reflect the new constant
- [x] 6.3 Add new test verifying AGENTS.md at workspace_dir is NOT in identity section build output

## 7. AgentConfig fields

- [x] 7.1 Add `agents_md_enabled: bool` and `agents_md_filenames: Vec<String>` to `AgentConfig` in `crates/synthia-agent/src/config/agent_config.rs`
- [x] 7.2 Add `#[serde(default)]` to both fields for backward compatibility
- [x] 7.3 Set `Default` impl: enabled=true, filenames=["AGENTS.md"]
- [x] 7.4 Update all 5 `AgentRunConfigBuilder` call sites to use `..Default::default()` (grep verified: agent.rs, server/state.rs, server/routes/ws.rs, server/routes/chat.rs, cli/repl_core/repl.rs)
- [x] 7.5 Add unit tests for serde round-trip and default values

## 8. End-to-end integration test

- [x] 8.1 Create `crates/synthia-context/tests/agents_md_discovery.rs` integration test
- [x] 8.2 Test: create temp dir with `AGENTS.md` at root and sub-dir, run `PromptBuilder::default_with_sections().resolve(ctx)`, assert content is in dynamic_content
- [x] 8.3 Test: parent-only `AGENTS.md` is found via ancestor walk
- [x] 8.4 Test: no `AGENTS.md` anywhere → section content empty (no header, no body)
- [x] 8.5 Test: section is `SessionCached` and reuses content on second resolve

## 9. Quality gates

- [x] 9.1 `cargo +nightly fmt --all` clean
- [x] 9.2 `cargo clippy --all-targets --all-features --tests --all` clean (no new warnings)
- [x] 9.3 `cargo test -p synthia-context` passes (unit + integration)
- [x] 9.4 `cargo test --workspace` passes (no regressions)
- [x] 9.5 `openspec validate agents-md-hierarchical-discovery` passes
