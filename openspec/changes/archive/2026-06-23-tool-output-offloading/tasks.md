## 1. Explore existing code

- [x] 1.1 Read `synthia_context::truncate::truncate_output`, `TruncateConfig`, `spill_to_disk` implementation and callers
- [x] 1.2 Read `synthia-agent::stream_builder::builder::tool_execution::execute_and_emit` to see where `TruncateConfig::default()` is built
- [x] 1.3 Read `synthia-agent::stream_builder::builder::run` startup sequence to find cleanup hook point

## 2. Enhance `TruncateConfig`

- [x] 2.1 Add `max_lines: usize` field to `TruncateConfig`
- [x] 2.2 Change default `max_bytes` to `50 * 1024`
- [x] 2.3 Change default `temp_dir` to `~/.synthia/tool-output/`
- [x] 2.4 Add optional `session_id: Option<String>` and `tool_call_id: Option<String>` fields for deterministic spill paths

## 3. Enhance `truncate_output` and `spill_to_disk`

- [x] 3.1 Update `truncate_output` to trigger offloading when `content.len() > max_bytes` OR `lines > max_lines`
- [x] 3.2 Update `spill_to_disk` to use deterministic path `<temp_dir>/<session-id>/<tool-call-id>.txt` when IDs are present; fall back to ULID when absent
- [x] 3.3 Set file permissions to `0o600` after writing
- [x] 3.4 Ensure non-offloaded outputs continue to pass through unchanged

## 4. Add cleanup utilities

- [x] 4.1 Add `truncate::cleanup_tool_output_store(base_dir, retention) -> std::io::Result<usize>` (synchronous, for tests)
- [x] 4.2 Add `truncate::cleanup_tool_output_store_async(base_dir, retention) -> tokio::io::Result<usize>` (asynchronous, for runtime)
- [x] 4.3 Implement recursive deletion of files older than 7 days

## 5. Wire cleanup into agent runtime

- [x] 5.1 Import cleanup utility in `synthia-agent::stream_builder::builder::run` or `tool_execution::execute_and_emit`
- [x] 5.2 Spawn async cleanup task at session startup (non-blocking)
- [x] 5.3 Spawn async cleanup task after each tool execution phase that writes offloads

## 6. Add tests

- [x] 6.1 Unit test: output below byte and line thresholds is not offloaded
- [x] 6.2 Unit test: output exceeding byte threshold is offloaded
- [x] 6.3 Unit test: output exceeding line threshold is offloaded
- [x] 6.4 Unit test: deterministic path uses `session_id`/`tool_call_id`
- [x] 6.5 Unit test: summary contains path marker
- [x] 6.6 Unit test: file permissions are `0o600`
- [x] 6.7 Unit test: stale files are deleted by cleanup
- [x] 6.8 Integration test: full flow through `truncate_output` with IDs

## 7. Verification

- [x] 7.1 Run `cargo +nightly fmt --all`
- [x] 7.2 Run `cargo clippy --all-targets --all-features --tests --all`
- [x] 7.3 Run `cargo test` for affected crates
- [x] 7.4 Fix all warnings and errors
