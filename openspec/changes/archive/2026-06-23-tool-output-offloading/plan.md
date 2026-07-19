# Tool Output Offloading Implementation Plan

> **For agentic workers:** Use `subagent-driven-development` to implement this plan task-by-task.

**Goal:** Enhance Synthia's existing output truncation so that tool outputs exceeding 50KB or 2000 lines are persisted to `~/.synthia/tool-output/<session-id>/<tool-call-id>.txt`, with 7-day retention and `0o600` permissions.

**Architecture:** Reuse and extend `synthia-context::truncate`:
- Add `max_lines`, `session_id`, `tool_call_id` to `TruncateConfig`.
- Update `truncate_output` to check both byte and line thresholds.
- Update `spill_to_disk` for deterministic paths and `0o600` permissions.
- Add cleanup utilities in `synthia-context::truncate`.
- Wire cleanup into `synthia-agent::stream_builder::builder::tool_execution::execute_and_emit`.

**Tech Stack:** Rust 2024, Tokio async fs, `synthia-context`, `synthia-agent`.

---

## Task 1: Read existing code

- [ ] **Step 1.1:** Read `crates/synthia-context/src/truncate/truncate_output.rs`, `types.rs`, `spill.rs`, `lines.rs`.
- [ ] **Step 1.2:** Read `crates/synthia-agent/src/stream_builder/builder/tool_execution/execute.rs` to find `TruncateConfig::default()` usage.
- [ ] **Step 1.3:** Read `crates/synthia-agent/src/stream_builder/builder/run/main_loop.rs` and `entry.rs` for session startup hook points.

---

## Task 2: Enhance `TruncateConfig`

- [ ] **Step 2.1:** In `crates/synthia-context/src/truncate/types.rs`, add to `TruncateConfig`:
  ```rust
  pub max_lines: usize,
  pub session_id: Option<String>,
  pub tool_call_id: Option<String>,
  ```
- [ ] **Step 2.2:** Update `Default` impl:
  - `max_bytes: 50 * 1024`
  - `max_lines: 2000`
  - `temp_dir: dirs::home_dir().unwrap_or_else(std::env::temp_dir).join(".synthia").join("tool-output")`
  - `head_lines: 100`, `tail_lines: 100` (unchanged)
- [ ] **Step 2.3:** Add `use dirs` if needed (check if `dirs` is already a dependency).

**Test command after Task 2:**
```bash
cargo check -p synthia-context
```

---

## Task 3: Enhance `truncate_output` and `spill_to_disk`

- [ ] **Step 3.1:** In `crates/synthia-context/src/truncate/truncate_output.rs`, update the threshold check:
  ```rust
  let line_count = content.lines().count();
  if content.len() <= cfg.max_bytes && line_count <= cfg.max_lines {
      return TruncatedResult::passthrough(content);
  }
  ```
- [ ] **Step 3.2:** In `crates/synthia-context/src/truncate/spill.rs`, update `spill_to_disk` signature:
  ```rust
  pub(super) fn spill_to_disk(
      content: &str,
      temp_dir: &Path,
      session_id: Option<&str>,
      tool_call_id: Option<&str>,
  ) -> std::io::Result<PathBuf>
  ```
- [ ] **Step 3.3:** If both `session_id` and `tool_call_id` are provided, use path `temp_dir / session_id / "{tool_call_id}.txt"`; otherwise use ULID fallback.
- [ ] **Step 3.4:** Create parent directories and write file.
- [ ] **Step 3.5:** Set file permissions to `0o600` using `std::fs::set_permissions`.
- [ ] **Step 3.6:** Update `truncate_output` call to `spill_to_disk` to pass IDs.

**Test command after Task 3:**
```bash
cargo test -p synthia-context truncate -- --nocapture
```

---

## Task 4: Add cleanup utilities

- [ ] **Step 4.1:** Create `crates/synthia-context/src/truncate/cleanup.rs` with:
  - `pub fn cleanup_tool_output_store(base_dir: &Path, retention: Duration) -> std::io::Result<usize>`
  - `pub async fn cleanup_tool_output_store_async(base_dir: &Path, retention: Duration) -> tokio::io::Result<usize>`
- [ ] **Step 4.2:** Recursively walk `base_dir`, delete files whose `modified()` time is older than `retention`.
- [ ] **Step 4.3:** Export cleanup functions from `crates/synthia-context/src/truncate/mod.rs`.

**Test command after Task 4:**
```bash
cargo test -p synthia-context cleanup -- --nocapture
```

---

## Task 5: Wire cleanup and deterministic paths into agent runtime

- [ ] **Step 5.1:** In `crates/synthia-agent/src/stream_builder/builder/tool_execution/execute.rs`:
  - Import `synthia_context::truncate::cleanup_tool_output_store_async`.
  - At session startup (or before tool execution loop), spawn cleanup task:
    ```rust
    let base_dir = dirs::home_dir().unwrap_or_else(std::env::temp_dir).join(".synthia").join("tool-output");
    tokio::spawn(async move {
        let _ = cleanup_tool_output_store_async(&base_dir, Duration::from_secs(7 * 24 * 3600)).await;
    });
    ```
  - Construct `TruncateConfig` with `session_id` and each `tool_call_id`:
    ```rust
    let truncate_cfg = TruncateConfig {
        session_id: Some(session_id.to_string()),
        tool_call_id: Some(result.tool_call_id.clone()),
        ..TruncateConfig::default()
    };
    ```
- [ ] **Step 5.2:** After processing all tool results in a phase, if any truncation occurred, spawn another cleanup task.
- [ ] **Step 5.3:** Ensure cleanup failures are logged but never panic or block.

**Test command after Task 5:**
```bash
cargo test -p synthia-agent tool_execution -- --nocapture
```

---

## Task 6: Add unit tests

- [ ] **Step 6.1:** In `crates/synthia-context/src/truncate/tests.rs` or `truncate_output.rs`, add test `small_output_not_offloaded`.
- [ ] **Step 6.2:** Add test `large_output_offloaded_by_bytes`.
- [ ] **Step 6.3:** Add test `large_output_offloaded_by_lines`.
- [ ] **Step 6.4:** Add test `deterministic_path_uses_session_and_call_id`.
- [ ] **Step 6.5:** Add test `summary_contains_path_marker`.
- [ ] **Step 6.6:** Add test `file_permissions_are_0o600`.
- [ ] **Step 6.7:** Add test `cleanup_removes_stale_files`.

**Test command after Task 6:**
```bash
cargo test -p synthia-context -- --nocapture
```

---

## Task 7: Add integration test

- [ ] **Step 7.1:** Extend `crates/synthia-context/tests/truncate_test.rs` with `offload_with_session_and_call_id`.
- [ ] **Step 7.2:** Build an output > 50KB, call `truncate_output` with `session_id` and `tool_call_id`, assert deterministic path exists and contains full content.

**Test command after Task 7:**
```bash
cargo test -p synthia-context --test truncate_test -- --nocapture
```

---

## Task 8: Format, lint, and full test

- [ ] **Step 8.1:** Run `cargo +nightly fmt --all`.
- [ ] **Step 8.2:** Run `cargo clippy --all-targets --all-features --tests --all` and fix all warnings.
- [ ] **Step 8.3:** Run `cargo test --all` and fix failures.
- [ ] **Step 8.4:** Run `cargo check -p synthia-agent -p synthia-context`.

**Final verification commands:**
```bash
cargo +nightly fmt --all
cargo clippy --all-targets --all-features --tests --all
cargo test --all
```

---

## Commit Points

- Commit after Task 3: `feat(context): add line threshold, deterministic paths, and permissions to truncate`
- Commit after Task 4: `feat(context): add tool-output cleanup utility`
- Commit after Task 5: `feat(agent): wire deterministic offloading and cleanup into tool execution`
- Commit after Task 8: `test(context): add offloading tests`
